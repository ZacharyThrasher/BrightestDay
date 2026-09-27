//! Ring console — type a phrase, the ring builds it (docs/DESIGN.md north star).
//!
//! * **Open** with Enter or T (rebindable). A single minimal line appears at bottom-centre; while it is
//!   open `UiFocus::typing` sets the TYPING bit (movement keys become text) but mouse aim, mouse-side
//!   tethers, reel and momentum stay live.
//! * History ↑/↓ (the player's own casts only — the ring never lists or suggests vocabulary: "the
//!   mystery is the fun"), Enter casts and closes, Shift+Enter casts and keeps typing, empty Enter
//!   recasts the last phrase, Esc closes.
//! * **Quick-cast** slots 1–4 cast the loadout phrases (lobby) when the console is closed.
//! * **Resolution**: `World::parse_power_phrase` (power shouts → `PowerMove`, incantation prefix →
//!   cast with the power's delivery) → `Lexicon::resolve` → `SimCommand::Cast`. Graft phrases set
//!   `aim.modifier` (the sim grafts onto the own construct under the crosshair / animates terrain).
//!   Stand-rush tokens (ORA / MUDA / …) are sent live as `StandRush(n)` while typing.
//! * **Feels instant**: [`Caster::commit_feedback`] fires on the commit frame (ring flare, cast pose, sound)
//!   before the sim answers. The construct itself is the only feedback — nothing on screen explains
//!   what the ring understood. Improvised phrases are silently appended to `logs/unresolved.txt` so
//!   playtests grow the vocabulary.
//! * No per-frame allocation while idle: every system early-outs on a closed console.
//!
//! [`Caster`] is the shared cast path (voice uses it too).

use crate::audio::PlaySfx;
use crate::camera::ViewAnchor;
use crate::input::{LookAngles, UiFocus};
use crate::lobby::prefs::{Action, KeyBinds, UiPrefs};
use crate::lobby::UiSet;
use crate::sim_bridge::{ActiveWorld, Cmd, SimCommand, WorldViews};
use crate::state::{AppState, InGameMenu};
use crate::util::core_v3;
use bd_core::corps::Corps;
use bd_lexicon::{Lexicon, Resolution, ResolutionKind};
use bd_sim::powers::{PowerPhrase, PowerTable};
use bd_sim::{Aim, TargetHint};
use bevy::ecs::system::SystemParam;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::ButtonState;
use bevy::picking::Pickable;
use bevy::prelude::*;
use std::io::Write as _;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};

/// Longest phrase the console accepts (the lexicon truncates at 120 chars anyway).
const MAX_CHARS: usize = 96;
const HISTORY: usize = 64;

// ------------------------------------------------------------------------------------------ lexicon

/// The loaded vocabulary (inserted once the background load finishes; shared, read-only).
#[derive(Resource, Clone)]
pub struct RingLexicon(pub Arc<Lexicon>);

#[derive(Resource)]
struct LexiconLoad(Mutex<Option<Receiver<Lexicon>>>);

fn start_lexicon_load() -> LexiconLoad {
    let (tx, rx) = std::sync::mpsc::channel();
    let spawned = std::thread::Builder::new().name("lexicon-load".into()).spawn(move || {
        let dir = crate::assets_dir();
        let t = std::time::Instant::now();
        let lex = match Lexicon::load(dir.join("lexicon.bin")) {
            Ok(l) => l,
            Err(e) => {
                warn!("lexicon.bin unavailable ({e}); ring falls back to curated icons");
                Lexicon::icons_only(&Lexicon::icon_files(&dir))
            }
        };
        info!("lexicon: {} entries + {} icons in {:.0} ms", lex.len(), lex.icon_count(), t.elapsed().as_secs_f64() * 1e3);
        let _ = tx.send(lex);
    });
    if let Err(e) = spawned {
        warn!("lexicon: could not spawn loader thread: {e}");
    }
    LexiconLoad(Mutex::new(Some(rx)))
}

fn poll_lexicon(mut commands: Commands, load: Option<Res<LexiconLoad>>) {
    let Some(load) = load else { return };
    let Ok(mut guard) = load.0.lock() else { return };
    let Some(rx) = guard.as_ref() else { return };
    match rx.try_recv() {
        Ok(lex) => {
            commands.insert_resource(RingLexicon(Arc::new(lex)));
            *guard = None;
            commands.remove_resource::<LexiconLoad>();
        }
        Err(std::sync::mpsc::TryRecvError::Empty) => {}
        Err(_) => {
            *guard = None;
            commands.remove_resource::<LexiconLoad>();
        }
    }
}

// ------------------------------------------------------------------------------------------ state

/// Local, instant feedback for the viewmodel / audio (fires on the key press, before the sim).
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub enum RingCue {
    /// A cast left the ring (voice or typed).
    Forming { voice: bool },
    /// A stand-rush token went out.
    Rush,
    /// The console opened (ring wakes up) / closed.
    Wake,
    Sleep,
}

#[derive(Resource, Default)]
pub struct RingConsole {
    pub open: bool,
    pub buf: String,
    history: Vec<String>,
    hist: Option<usize>,
    draft: String,
    pub last: Option<String>,
    rush_sent: u16,
    dirty: bool,
    caret_t: f32,
    /// Phrases cast before the lexicon finished loading.
    pending: Vec<(String, bool)>,
}

impl RingConsole {
    fn set_buf(&mut self, s: &str) {
        self.buf.clear();
        self.buf.push_str(s);
        self.dirty = true;
    }

    /// Dev scenarios (screenshots): open the console with `s` typed (not cast).
    pub fn dev_type(&mut self, s: &str) {
        self.open = true;
        self.set_buf(s);
    }
}

#[derive(Component)]
struct ConsoleRoot;
#[derive(Component)]
struct ConsoleText;
#[derive(Component)]
struct ConsoleCaret;
#[derive(Component)]
struct ConsoleLine;

pub struct ConsolePlugin;

impl Plugin for ConsolePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(start_lexicon_load())
            .init_resource::<RingConsole>()
            .add_message::<RingCue>()
            .add_systems(Startup, spawn_console_ui)
            .add_systems(PreUpdate, poll_lexicon)
            .add_systems(Update, (console_keys, quick_cast, flush_pending).chain().in_set(UiSet::Input))
            .add_systems(Update, draw_console.in_set(UiSet::Draw))
            .add_systems(OnExit(AppState::InGame), close_on_exit);
    }
}

fn close_on_exit(mut c: ResMut<RingConsole>, mut focus: ResMut<UiFocus>) {
    c.open = false;
    c.buf.clear();
    c.dirty = true;
    focus.typing = false;
}

pub fn corps_color(c: Corps) -> Color {
    let k = c.color();
    Color::srgb(k[0], k[1], k[2])
}

// ------------------------------------------------------------------------------------------ cast path

/// Everything a cast needs. Shared by the console, quick-cast slots and voice.
#[derive(SystemParam)]
pub struct Caster<'w> {
    lex: Option<Res<'w, RingLexicon>>,
    active: Option<Res<'w, ActiveWorld>>,
    views: Res<'w, WorldViews>,
    look: Res<'w, LookAngles>,
    anchor: Res<'w, ViewAnchor>,
    aim_ray: Option<Res<'w, crate::present::camera3p::AimRay>>,
    prefs: Res<'w, UiPrefs>,
    cmds: MessageWriter<'w, SimCommand>,
    cues: MessageWriter<'w, RingCue>,
    sfx: MessageWriter<'w, PlaySfx>,
    /// Shout callouts (present::shout): the phrase bursts out as big letters.
    shouts: MessageWriter<'w, crate::present::shout::ShoutCast>,
    /// SIGNATURE HOOK: how hard a signature was shouted (CAPS / "!!!" / amplifier words).
    shout_table: Option<Res<'w, bd_fx::ShoutTable>>,
    /// SIGNATURE HOOK: peak mic level of the push-to-talk utterance (a spoken signature's power).
    sig_voice: Option<Res<'w, crate::present::signature::SignatureVoice>>,
    /// FREEDOM HOOK: the last phrase ("again!" repeats it).
    memo: Option<ResMut<'w, crate::lobby::feed::CastMemo>>,
}

/// What a phrase turned into.
#[derive(Debug)]
pub enum Resolved {
    PowerMove(u8),
    Rush(u16),
    Cast { res: Box<Resolution>, incantation: bool },
}

/// A plain phrase already read by the intent stages (`plan_cast` → `Lexicon::intent`): cast that reading.
fn resolved_from(res: Resolution) -> Resolved {
    match res.stand_rush {
        Some(n) if res.kind == ResolutionKind::Graft => Resolved::Rush(n),
        _ => Resolved::Cast { res: Box::new(res), incantation: false },
    }
}

/// Resolution order: power phrase → lexicon.
pub fn resolve_phrase(lex: &Lexicon, power: PowerPhrase, text: &str, spoken: bool) -> Resolved {
    let (t, incantation) = match power {
        PowerPhrase::Move(slot) => return Resolved::PowerMove(slot),
        PowerPhrase::Incantation { rest } => (rest, true),
        PowerPhrase::Plain => (text.to_string(), false),
    };
    let res = if spoken { lex.resolve_spoken(&t) } else { lex.resolve(&t) };
    if let Some(n) = res.stand_rush
        && res.kind == ResolutionKind::Graft
        && !incantation
    {
        return Resolved::Rush(n);
    }
    Resolved::Cast { res: Box::new(res), incantation }
}

impl Caster<'_> {
    pub fn lexicon_ready(&self) -> bool {
        self.lex.is_some()
    }

    /// Does the local player hold a stand (rush tokens drive it live)?
    pub fn has_stand(&self) -> bool {
        let Some(a) = self.active.as_ref() else { return false };
        a.local.and_then(|id| a.world.player_power(id)).is_some_and(|d| d.companion.is_some())
    }

    pub fn rush(&mut self, n: u16) {
        if n == 0 {
            return;
        }
        self.cmds.write(SimCommand::local(Cmd::StandRush(n)));
        self.cues.write(RingCue::Rush);
    }

    /// The instant local "catch" on the commit frame (Enter, a quick-cast slot or the voice phrase
    /// finalising — identical for all): ring flare + hand cast pose (`RingCue::Forming` →
    /// viewmodel / viewmodel_anim) and the confirm sound, before the sim or network answers.
    pub fn commit_feedback(&mut self, spoken: bool) {
        self.cues.write(RingCue::Forming { voice: spoken });
        self.sfx.write(PlaySfx::new("cast_confirm").gain(0.55));
    }

    /// Cast `text`. Returns false when there is no world to cast into (nothing happens).
    /// `rush_already` = stand-rush tokens of this phrase already sent live.
    pub fn cast(&mut self, text: &str, spoken: bool, rush_already: u16) -> bool {
        self.cast_heard(text, spoken, rush_already, None)
    }

    /// [`Self::cast`] with the speech recogniser's confidence for a spoken phrase (intent log only).
    pub fn cast_heard(&mut self, text: &str, spoken: bool, rush_already: u16, heard_conf: Option<f32>) -> bool {
        let text = text.trim();
        if text.is_empty() {
            return false;
        }
        let Some(active) = self.active.as_ref() else { return false };
        let Some(local) = active.local else { return false };
        let Some(lex) = self.lex.as_ref().map(|l| l.0.clone()) else { return false };
        let power = active.world.parse_power_phrase(local, text);
        // FREEDOM HOOK: "it" = your newest construct, "again!" / "two more" re-cast, "missile at Jake"
        // aims at Jake — text steps before the resolver (lobby/feed.rs); what doesn't apply casts as said.
        let plan = match &power {
            PowerPhrase::Plain => crate::lobby::feed::plan_cast(&lex, &active.world, local, text, spoken, self.memo.as_ref().and_then(|m| m.last.as_deref())),
            _ => crate::lobby::feed::CastPlan::plain(text),
        };
        let voice_peak = spoken.then(|| self.sig_voice.as_ref().map_or(0.8, |v| v.peak));
        let energy = crate::lobby::feed::cast_energy(text, voice_peak, self.shout_table.as_deref());
        // Instant local feedback on the same frame as the key press.
        self.commit_feedback(spoken);
        // A plain phrase was already read by the intent stages (one order for typed and spoken words).
        let resolved = match (&power, plan.intent.as_ref()) {
            (PowerPhrase::Plain, Some(it)) => resolved_from(it.resolution.clone()),
            _ => resolve_phrase(&lex, power, &plan.text, spoken),
        };
        // "Did it hear me right?": the title letters show exactly what was heard, or the canonical
        // name of a recognised signature ("final flesh" heard → "FINAL FLASH!!").
        self.shouts.write(crate::present::shout::ShoutCast { text: shout_text(text, &resolved), spoken });
        match resolved {
            Resolved::PowerMove(slot) => {
                let (eye, dir) = self.crosshair();
                self.cmds.write(SimCommand::local(Cmd::PowerMove(slot, Some(Aim::new(core_v3(eye), core_v3(dir))))));
            }
            Resolved::Rush(n) => {
                let n = n.saturating_sub(rush_already);
                self.rush(n);
            }
            Resolved::Cast { res, incantation } => {
                // The crosshair ray (third person aims from the eye toward what the camera sees).
                let (eye, dir) = self.crosshair();
                let modifier = res.kind == ResolutionKind::Graft || plan.graft_onto.is_some();
                let target_hint = match (plan.graft_onto, plan.target) {
                    (Some(id), _) => Some(TargetHint::Construct(id)),
                    // FREEDOM HOOK: a named player is delivered to by the host (bd_sim::freedom::deliver)
                    (None, Some(p)) => Some(TargetHint::Named { player: p, via: plan.delivery.unwrap_or(bd_sim::freedom::Delivery::Hunt) }),
                    (None, None) => self.aimed_enemy(eye, dir).map(TargetHint::Player),
                };
                let aim = Aim { origin: core_v3(eye), dir: core_v3(dir), target_hint, modifier };
                if Lexicon::is_fallback(&res) {
                    log_unresolved(text, spoken, &res);
                }
                log_intent(text, spoken, heard_conf, &plan, &res); // local-only playtest log (logs/intent.log)
                let mut res = *res;
                // COSTUME HOOK: costume words ("gojo but bald", "red goku", "gojo with pink hair") dress
                // the summon only — the construct's mechanics come from the bare character.
                let summon = res.signature.is_none() && !incantation && !modifier;
                // (A named move — "reinhardt charge" — keeps its reading: the move IS the mechanics.)
                if summon
                    && res.recipe.opening.is_none()
                    && let Some(bare) = bd_shapes::costume::summon::mechanics_phrase(&plan.text, &res.recipe)
                {
                    let b = lex.resolve(&bare);
                    (res.recipe, res.graft, res.shape, res.alt_shape) = (b.recipe, b.graft, b.shape, b.alt_shape);
                }
                if let Some(n) = plan.count {
                    res.recipe.count = n.max(1);
                }
                // COSTUME HOOK: a plain summon naming a character ("goku", "GIANT GOKU") is built as
                // that character's costume (bd_shapes::costume::summon; derived from the text on every
                // peer — no new net data). Powers / signatures / incantations / grafts never get here.
                if summon && bd_shapes::costume::summon::apply(&plan.text, &mut res.recipe, &mut res.shape) {
                    res.alt_shape = None;
                }
                // FREEDOM HOOK: the words (kill feed), shout energy and an "it" resize go just before the cast.
                let resize = if plan.graft_onto.is_some() { res.graft.scale_mul } else { 1.0 };
                let meta = bd_sim::freedom::CastMeta { resize, ..bd_sim::freedom::CastMeta::new(&plan.said, energy) };
                self.cmds.write(SimCommand::local(Cmd::CastMeta(meta)));
                if !plan.repeat
                    && let Some(m) = self.memo.as_mut()
                {
                    m.last = Some(text.to_string());
                }
                // SIGNATURE HOOK (COMBAT.md §5j): "domain expansion …", "kamehameha" → a signature cast.
                let cmd = match res.signature.as_ref().filter(|_| !incantation) {
                    Some(s) => {
                        let level = spoken.then(|| self.sig_voice.as_ref().map_or(0.8, |v| v.peak));
                        signature_cmd(res.recipe, res.shape, aim, s, text, self.shout_table.as_deref(), level)
                    }
                    None => Cmd::Cast { recipe: Box::new(res.recipe), shape: res.shape, alt_shape: res.alt_shape, aim, incantation },
                };
                self.cmds.write(SimCommand::local(cmd));
            }
        }
        true
    }

    /// The crosshair ray `(eye, dir)`: third person converges from the eye on what the camera sees.
    pub fn crosshair(&self) -> (Vec3, Vec3) {
        match self.aim_ray.as_ref() {
            Some(r) if r.dir.is_finite() && r.dir.length_squared() > 0.5 => (r.origin, r.dir),
            _ => (self.anchor.eye, self.look.forward()),
        }
    }

    /// Nearest living enemy within a small cone of the crosshair (for autonomous constructs).
    fn aimed_enemy(&self, eye: Vec3, dir: Vec3) -> Option<bd_core::PlayerId> {
        let mut best: Option<(f32, bd_core::PlayerId)> = None;
        for p in &self.views.players {
            if p.is_local || !p.alive() {
                continue;
            }
            let to = p.pos + Vec3::Y * 1.0 - eye;
            let d = to.length();
            if !(0.5..150.0).contains(&d) {
                continue;
            }
            let ang = to.normalize().dot(dir).clamp(-1.0, 1.0).acos();
            // Wider cone up close (a body is big when near).
            let limit = 0.10f32.max((1.2 / d).atan());
            if ang < limit && best.is_none_or(|(b, _)| ang < b) {
                best = Some((ang, p.id));
            }
        }
        best.map(|(_, id)| id)
    }
}

/// The title letters for a cast: what was heard / typed, or a recognised signature's canonical name
/// yelled ("FINAL FLASH!!"). An incantation keeps the words said.
fn shout_text(heard: &str, resolved: &Resolved) -> String {
    match resolved {
        Resolved::Cast { res, incantation: false } => match res.signature.as_ref().filter(|s| !s.title.trim().is_empty()) {
            Some(s) => signature_shout(&s.title),
            None => heard.to_string(),
        },
        _ => heard.to_string(),
    }
}

/// "Final Flash" → "FINAL FLASH!!" (≤ the shout length: the "!!" never gets cut off a long title).
pub fn signature_shout(title: &str) -> String {
    let t: String = title.trim().to_uppercase().chars().take(bd_fx::SHOUT_MAX_CHARS - 2).collect();
    format!("{}!!", t.trim_end())
}

/// SIGNATURE HOOK: a signature resolution → `Cmd::Signature` (the sim amplifies it). The title rides
/// in `recipe.name` (cut-in label, staleness); `power` 0..1 = how hard it was shouted (typed energy:
/// CAPS / "!!!" / stretched letters / amplifier words; `bd_fx::typed_energy` ≈ 0.35 calm … 2.3).
/// `voice` = the utterance's peak mic level when spoken (`bd_fx::voice_energy`). A data windup
/// (`Merge(a, b)`) rides along for the presentation.
pub fn signature_cmd(
    mut recipe: bd_core::mechanics::Recipe,
    shape: Option<bd_core::shape::ShapeSpec>,
    aim: Aim,
    s: &bd_lexicon::SignatureCall,
    text: &str,
    table: Option<&bd_fx::ShoutTable>,
    voice: Option<f32>,
) -> Cmd {
    use bd_lexicon::SignatureKind as L;
    use bd_sim::signature::{SignatureCast, SignatureKind as S};
    let kind = match s.kind {
        L::Domain => S::Domain,
        L::Beam => S::Beam,
        L::Transformation => S::Transformation,
        L::Summon => S::Summon,
    };
    let fallback = bd_fx::ShoutTable::default();
    let t = table.unwrap_or(&fallback);
    let energy = match voice {
        Some(level) => bd_fx::voice_energy(level, text, t),
        None => bd_fx::typed_energy(text, t),
    };
    let power = ((energy - 0.35) / 1.4).clamp(0.0, 1.0);
    if !s.title.is_empty() {
        recipe.name = s.title.clone();
    }
    Cmd::Signature { recipe: Box::new(recipe), shape, aim: Aim { modifier: false, ..aim }, sig: match s.windup {
        Some((a, b)) => SignatureCast::new(kind, power).with_merge(a, b),
        None => SignatureCast::new(kind, power),
    } }
}

/// Append a fallback phrase to `logs/unresolved.txt` (next to `assets/`).
/// Every cast → one line of `logs/intent.log` (LOCAL ONLY — never sent anywhere; for the lead to improve
/// recognition after playtests with `cargo run -p bd_lexicon --example intent_report`). Tab-separated:
/// `time  mode  heard-confidence  phrase  stages  target  delivery  summary` (see bd_lexicon
/// `Intent::summary`). The target is written as `P<n>` (the n-th other player), never a name.
fn log_intent(text: &str, spoken: bool, heard_conf: Option<f32>, plan: &crate::lobby::feed::CastPlan, res: &Resolution) {
    if std::env::var_os("BD_NO_INTENT_LOG").is_some() {
        return;
    }
    let dir = crate::assets_dir().parent().map(|p| p.join("logs")).unwrap_or_else(|| "logs".into());
    let (stages, target, summary) = match plan.intent.as_ref() {
        Some(it) => {
            // the summary of what is actually cast (count / costume overrides included), no names
            let summary = bd_lexicon::Intent { resolution: res.clone(), target: None, ..(**it).clone() }.summary(&[]);
            let target = it.target.map(|t| format!("P{}{}", t + 1, if it.name_dist > 0 { format!("~{}", it.name_dist) } else { String::new() }));
            (it.stages.join(","), target.unwrap_or_else(|| "-".into()), summary)
        }
        None => ("power".to_string(), "-".to_string(), format!("{} conf={:.2}", res.signature(), res.confidence)),
    };
    let line = format!(
        "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
        timestamp(),
        if spoken { "voice" } else { "typed" },
        heard_conf.map_or("-".to_string(), |c| format!("{c:.2}")),
        text.replace(['\t', '\n', '\r'], " "),
        if stages.is_empty() { "-".to_string() } else { stages },
        target,
        plan.delivery.map_or("-".to_string(), |d| format!("{d:?}").to_lowercase()),
        summary.replace(['\t', '\n', '\r'], " "),
    );
    let write = || -> std::io::Result<()> {
        std::fs::create_dir_all(&dir)?;
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("intent.log"))?;
        f.write_all(line.as_bytes())
    };
    if let Err(e) = write() {
        warn!("could not write the intent log: {e}");
    }
}

fn log_unresolved(text: &str, spoken: bool, res: &Resolution) {
    let dir = crate::assets_dir().parent().map(|p| p.join("logs")).unwrap_or_else(|| "logs".into());
    let line = format!(
        "{}\t{}\t{}\t{:?}\tignored=[{}]\tconf={:.2}\n",
        timestamp(),
        if spoken { "voice" } else { "typed" },
        text.replace(['\t', '\n', '\r'], " "),
        res.recipe.form,
        res.ignored.join(", "),
        res.confidence
    );
    let write = || -> std::io::Result<()> {
        std::fs::create_dir_all(&dir)?;
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("unresolved.txt"))?;
        f.write_all(line.as_bytes())
    };
    if let Err(e) = write() {
        warn!("could not log unresolved phrase: {e}");
    }
}

/// UTC "YYYY-MM-DD HH:MM:SS" without a date crate (civil-from-days).
pub fn timestamp() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}", rem / 3600, rem / 60 % 60, rem % 60)
}

/// Rush tokens typed so far in `buf` (complete tokens only; a trailing partial token counts once
/// it completes). Words that aren't pure rush words count 0.
pub fn count_rush_tokens(buf: &str) -> u16 {
    let rules = &PowerTable::get().rules;
    let mut total = 0u16;
    for w in buf.split(|c: char| c.is_whitespace() || c == ',' || c == '!' || c == '-') {
        let w: String = w.chars().filter(|c| c.is_ascii_alphabetic()).map(|c| c.to_ascii_uppercase()).collect();
        if w.is_empty() {
            continue;
        }
        let mut i = 0;
        let mut n = 0u16;
        let mut last: Option<char> = None;
        let b = w.as_bytes();
        let mut ok = true;
        while i < b.len() {
            let rest = &w[i..];
            if let Some(t) = rules.rush_tokens.iter().find(|t| rest.starts_with(t.as_str())) {
                n += 1;
                i += t.len();
                last = t.chars().last();
            } else if last == Some(b[i] as char) {
                // WRYYYY: trailing repeats of a token's last letter.
                i += 1;
            } else if rules.rush_tokens.iter().any(|t| t.starts_with(rest)) {
                break; // a token being typed
            } else {
                ok = false;
                break;
            }
        }
        if ok {
            total = total.saturating_add(n);
        }
    }
    total
}

// ------------------------------------------------------------------------------------------ input

fn gameplay(state: &AppState, menu: &InGameMenu) -> bool {
    *state == AppState::InGame && !menu.0
}

#[allow(clippy::too_many_arguments)]
fn console_keys(
    mut typed: MessageReader<KeyboardInput>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    binds: Res<KeyBinds>,
    state: Res<State<AppState>>,
    menu: Res<InGameMenu>,
    time: Res<Time<Real>>,
    mut focus: ResMut<UiFocus>,
    mut console: ResMut<RingConsole>,
    oath: Res<crate::oath::OathState>,
    mut caster: Caster,
) {
    let live = gameplay(state.get(), &menu) && caster.active.is_some() && !oath.open;
    if !live {
        typed.clear();
        if console.open {
            console.open = false;
            console.dirty = true;
        }
        if focus.typing && !oath.open {
            focus.typing = false;
        }
        return;
    }
    let console = &mut *console;
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let open_keys = [binds.get(Action::Console), binds.get(Action::ConsoleAlt)];
    let mut eat_esc = false;
    let mut edited = false;
    if !console.open {
        // Mouse-bound console key.
        if open_keys.iter().any(|b| matches!(b, crate::lobby::prefs::Bind::Mouse(_)) && b.just_pressed(&keys, &mouse)) {
            console.open = true;
            console.dirty = true;
            caster.cues.write(RingCue::Wake);
        }
    }
    for ev in typed.read() {
        if ev.state != ButtonState::Pressed {
            continue;
        }
        if !console.open {
            if keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]) {
                continue; // Alt+Enter = fullscreen
            }
            if open_keys.iter().any(|b| b.key() == Some(ev.key_code)) || (ev.key_code == KeyCode::NumpadEnter && open_keys.contains(&crate::lobby::prefs::Bind::Key(KeyCode::Enter))) {
                console.open = true;
                console.dirty = true;
                console.hist = None;
                console.rush_sent = 0;
                caster.cues.write(RingCue::Wake);
            }
            continue;
        }
        match &ev.logical_key {
            Key::Enter => {
                let text = std::mem::take(&mut console.buf);
                let phrase = if text.trim().is_empty() { console.last.clone() } else { Some(text.trim().to_string()) };
                if let Some(p) = phrase {
                    if console.history.last() != Some(&p) {
                        if console.history.len() >= HISTORY {
                            console.history.remove(0);
                        }
                        console.history.push(p.clone());
                    }
                    console.last = Some(p.clone());
                    if caster.lexicon_ready() {
                        caster.cast(&p, false, console.rush_sent);
                    } else {
                        console.pending.push((p, false));
                        caster.commit_feedback(false);
                    }
                }
                console.rush_sent = 0;
                console.hist = None;
                console.dirty = true;
                if !shift {
                    console.open = false;
                    caster.cues.write(RingCue::Sleep);
                }
                // Swallow the rest of this frame's keys? No: keep reading so nothing is lost — any
                // further keys this frame reopen/type as usual.
            }
            Key::Escape => {
                console.open = false;
                console.buf.clear();
                console.hist = None;
                console.rush_sent = 0;
                console.dirty = true;
                eat_esc = true;
                caster.cues.write(RingCue::Sleep);
            }
            Key::Backspace => {
                if ctrl {
                    let t = console.buf.trim_end().len();
                    let cut = console.buf[..t].rfind(' ').map(|i| i + 1).unwrap_or(0);
                    console.buf.truncate(cut);
                } else {
                    console.buf.pop();
                }
                edited = true;
            }
            Key::ArrowUp | Key::ArrowDown => {
                let n = console.history.len();
                if n == 0 {
                    continue;
                }
                let up = matches!(ev.logical_key, Key::ArrowUp);
                let next = match (console.hist, up) {
                    (None, true) => {
                        console.draft = console.buf.clone();
                        Some(n - 1)
                    }
                    (None, false) => None,
                    (Some(i), true) => Some(i.saturating_sub(1)),
                    (Some(i), false) => (i + 1 < n).then_some(i + 1),
                };
                console.hist = next;
                let s = match next {
                    Some(i) => console.history[i].clone(),
                    None => std::mem::take(&mut console.draft),
                };
                console.set_buf(&s);
            }
            _ => {
                let text = ev.text.as_ref().map(|t| t.as_str()).or(match &ev.logical_key {
                    Key::Character(c) => Some(c.as_str()),
                    Key::Space => Some(" "),
                    _ => None,
                });
                if let Some(t) = text {
                    for c in t.chars() {
                        if !c.is_control() && console.buf.chars().count() < MAX_CHARS {
                            console.buf.push(c);
                            edited = true;
                        }
                    }
                }
            }
        }
    }
    if eat_esc {
        // Esc closed the console: it must not also open the pause menu this frame.
        keys.clear_just_pressed(KeyCode::Escape);
    }
    if console.open {
        // Keys that are text while typing must not also act as game keys this frame (every bound
        // key: readers also check `UiFocus::typing`, this covers the frame focus changes).
        for a in Action::ALL.into_iter().filter(|a| !matches!(a, Action::Console | Action::ConsoleAlt)) {
            if let Some(k) = binds.get(a).key() {
                keys.clear_just_pressed(k);
            }
        }
        console.caret_t += time.delta_secs();
    }
    if edited {
        console.dirty = true;
        console.hist = None;
        // Live stand rush: every completed ORA is one punch now.
        if caster.has_stand() {
            let n = count_rush_tokens(&console.buf);
            if n > console.rush_sent {
                caster.rush(n - console.rush_sent);
                console.rush_sent = n;
            }
        }
    }
    let want = console.open;
    if focus.typing != want && !oath.open {
        focus.typing = want;
    }
}

/// 1–4: cast the loadout phrases (console closed).
#[allow(clippy::too_many_arguments)]
fn quick_cast(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    binds: Res<KeyBinds>,
    state: Res<State<AppState>>,
    menu: Res<InGameMenu>,
    console: Res<RingConsole>,
    oath: Res<crate::oath::OathState>,
    party: Option<Res<crate::lobby::party::PartyUi>>,
    mut caster: Caster,
) {
    if console.open || oath.open || !gameplay(state.get(), &menu) || caster.active.is_none() || party.as_ref().is_some_and(|p| p.phase == Some(bd_sim::party::PartyPhase::Pick)) {
        return;
    }
    for (i, a) in [Action::Slot1, Action::Slot2, Action::Slot3, Action::Slot4].into_iter().enumerate() {
        if binds.just_pressed(a, &keys, &mouse) {
            let phrase = caster.prefs.loadout[i].clone();
            if !phrase.trim().is_empty() {
                caster.cast(&phrase, false, 0);
            }
        }
    }
}

/// Casts typed before the lexicon finished loading go out as soon as it is ready.
fn flush_pending(mut console: ResMut<RingConsole>, mut caster: Caster) {
    if console.pending.is_empty() || !caster.lexicon_ready() {
        return;
    }
    for (p, spoken) in std::mem::take(&mut console.pending) {
        caster.cast(&p, spoken, 0);
    }
}

// ------------------------------------------------------------------------------------------ drawing

fn spawn_console_ui(mut commands: Commands) {
    commands
        .spawn((
            Name::new("RingConsole"),
            ConsoleRoot,
            Node {
                position_type: PositionType::Absolute,
                left: percent(50),
                bottom: percent(16),
                width: px(720),
                margin: UiRect::left(px(-360)),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(6),
                ..default()
            },
            Visibility::Hidden,
            Pickable::IGNORE,
            GlobalZIndex(55),
        ))
        .with_children(|root| {
            root.spawn((
                ConsoleText,
                Text::new(""),
                TextFont::from_font_size(30.0),
                TextColor(Color::WHITE),
                TextShadow { offset: Vec2::new(0.0, 0.0), color: Color::srgba(0.2, 1.0, 0.35, 0.8) },
                TextLayout::justify(Justify::Center),
            ))
            .with_children(|t| {
                t.spawn((ConsoleCaret, TextSpan::new("_"), TextFont::from_font_size(30.0), TextColor(Color::WHITE)));
            });
            // The hard-light underline: a thin corps-coloured beam, brighter in the middle.
            root.spawn((
                ConsoleLine,
                Node { width: px(420), height: px(2), ..default() },
                BackgroundGradient::from(bevy::ui::LinearGradient::new(
                    bevy::ui::LinearGradient::TO_RIGHT,
                    vec![
                        bevy::ui::ColorStop::new(Color::NONE, percent(0)),
                        bevy::ui::ColorStop::new(Color::srgba(0.3, 1.0, 0.45, 0.95), percent(50)),
                        bevy::ui::ColorStop::new(Color::NONE, percent(100)),
                    ],
                )),
            ));
        });
}

#[allow(clippy::type_complexity)]
fn draw_console(
    mut console: ResMut<RingConsole>,
    active: Option<Res<ActiveWorld>>,
    prefs: Res<UiPrefs>,
    mut root: Query<&mut Visibility, With<ConsoleRoot>>,
    mut text: Query<(&mut Text, &mut TextShadow), With<ConsoleText>>,
    mut caret: Query<&mut TextColor, With<ConsoleCaret>>,
    mut line: Query<&mut BackgroundGradient, With<ConsoleLine>>,
) {
    if console.open {
        // Caret blink (no allocation).
        let on = (console.caret_t * 2.2) as i32 % 2 == 0;
        for mut c in &mut caret {
            let want = Color::srgba(1.0, 1.0, 1.0, if on { 0.9 } else { 0.0 });
            if c.0 != want {
                c.0 = want;
            }
        }
    }
    if !console.dirty {
        return;
    }
    console.dirty = false;
    let vis = if console.open { Visibility::Inherited } else { Visibility::Hidden };
    for mut v in &mut root {
        *v = vis;
    }
    let corps = active.as_ref().and_then(|a| a.local_player().map(|p| p.corps)).unwrap_or(prefs.corps);
    let c = corps_color(corps);
    for (mut t, mut sh) in &mut text {
        t.0.clear();
        t.0.push_str(&console.buf);
        sh.color = c.with_alpha(0.85);
    }
    for mut bg in &mut line {
        if let Some(bevy::ui::Gradient::Linear(l)) = bg.0.first_mut()
            && l.stops.len() == 3
        {
            l.stops[1].color = c.with_alpha(0.95);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// "Did it hear me right?": a recognised signature shows its canonical name (typed or spoken);
    /// anything else shows exactly what was heard.
    #[test]
    fn title_letters_show_the_canonical_signature_name() {
        let Ok(lex) = Lexicon::load(&crate::assets_dir().join("lexicon.bin")) else { return };
        let shout = |heard: &str, spoken: bool| shout_text(heard, &resolve_phrase(&lex, PowerPhrase::Plain, heard, spoken));
        assert_eq!(shout("final flash", false), "FINAL FLASH!!");
        assert_eq!(shout("final flash", true), "FINAL FLASH!!");
        assert_eq!(shout("KAMEHAMEHA!!!", false), "KAMEHAMEHA!!");
        assert_eq!(shout("galic gun", false), "GALICK GUN!!");
        assert_eq!(shout("giant hammer", false), "giant hammer");
        assert_eq!(shout("GIANT HAMMER!!!", true), "GIANT HAMMER!!!");
        assert!(signature_shout(&"Domain Expansion: Very Long Words ".repeat(3)).ends_with("!!"));
        assert!(signature_shout(&"x".repeat(200)).len() <= bd_fx::SHOUT_MAX_CHARS);
    }

    #[test]
    fn rush_counts_live() {
        assert_eq!(count_rush_tokens("ORA"), 1);
        assert_eq!(count_rush_tokens("ora ora or"), 2);
        assert_eq!(count_rush_tokens("ORAORAORA"), 3);
        assert_eq!(count_rush_tokens("mudamuda"), 2);
        assert_eq!(count_rush_tokens("WRYYYYY"), 1);
        assert_eq!(count_rush_tokens("hammer"), 0);
        assert_eq!(count_rush_tokens("orange"), 0);
    }

    /// The owner's bug end to end: "DROP A METEOR ON ETHAN" (typed or heard) names Ethan, and the host
    /// drops it on Ethan — not in front of the caster, who is looking the other way.
    #[test]
    fn drop_a_meteor_on_ethan_end_to_end() {
        use bd_core::{PlayerId, Vec3};
        let Ok(lex) = Lexicon::load(&crate::assets_dir().join("lexicon.bin")) else { return };
        let cfg = bd_sim::SimConfig { arena: bd_sim::ArenaSource::Flat { size_m: 128.0 }, ..Default::default() };
        let mut w = bd_sim::World::new(cfg);
        let (me, ethan, jake) = (PlayerId(1), PlayerId(2), PlayerId(3));
        for (id, name) in [(me, "Caster"), (ethan, "Ethan"), (jake, "Jake")] {
            w.add_player(id, name.into(), Corps::from_index(id.0)).unwrap();
        }
        w.start_live();
        w.tick(&[]);
        w.cfg.rules.willpower = bd_sim::rules::WillpowerMode::Infinite;
        for (id, pos) in [(me, Vec3::new(0.0, 8.0, 0.0)), (ethan, Vec3::new(8.0, 8.0, -30.0)), (jake, Vec3::new(-20.0, 8.0, 10.0))] {
            let p = w.player_mut(id).unwrap();
            p.spawn_shield = 0.0;
            p.mv.pos = pos;
        }
        w.tick(&[]);
        for (text, spoken) in [("DROP A METEOR ON ETHAN", false), ("drop a meteor on eaton.", true), ("drop a meteor on e than", true)] {
            let plan = crate::lobby::feed::plan_cast(&lex, &w, me, text, spoken, None);
            assert_eq!(plan.target, Some(ethan), "{text:?}");
            assert_eq!(plan.delivery, Some(bd_sim::freedom::Delivery::Onto), "{text:?}");
            let Some(Resolved::Cast { res, .. }) = plan.intent.as_ref().map(|i| resolved_from(i.resolution.clone())) else { panic!("{text:?}") };
            // the caster looks along +X; only the name can put it on Ethan
            let eye = w.player(me).unwrap().eye(w.params());
            let hint = Some(TargetHint::Named { player: ethan, via: plan.delivery.unwrap() });
            let id = w.cast(me, res.recipe, None, Aim { origin: eye, dir: Vec3::X, target_hint: hint, modifier: false }).unwrap();
            let c = w.constructs.get(id).unwrap();
            let at = w.player(ethan).unwrap().mv.pos;
            assert!(Vec3::new(c.pos.x - at.x, 0.0, c.pos.z - at.z).length() < 1.5, "{text:?}: formed at {:?}, Ethan at {at:?}", c.pos);
        }
        // "again!" re-runs the stages on the last phrase: still at Jake
        let plan = crate::lobby::feed::plan_cast(&lex, &w, me, "again!", false, Some("missile at jake"));
        assert!(plan.repeat);
        assert_eq!((plan.target, plan.delivery), (Some(jake), Some(bd_sim::freedom::Delivery::At)));
    }

    #[test]
    fn timestamp_shape() {
        let t = timestamp();
        assert_eq!(t.len(), 19);
        assert!(t.starts_with("20"));
    }
}
