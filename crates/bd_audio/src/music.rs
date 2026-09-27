//! The soundtrack: bd_music's adaptive chip player on the Music bus, and the game-side director
//! that picks tracks and fires stingers.
//!
//! * [`ChipMusic`] adapts [`bd_music::MusicPlayer`] to [`MusicSource`] (the mixer owns it on the
//!   audio thread; no allocation there).
//! * [`MusicDirector`] (game thread): which track for the scene / map / state (explore ↔ combat
//!   with hysteresis, final stock, sudden death, party, results), stingers (KO throttled to one per
//!   2 s, streak, match point, last standing, victory / defeat) and the Style setting.
//!   Intensity and tempo come from the mix snapshots (`music_intensity` / `music_tempo`).
//! * [`ThemeDirector`] (inside the music director): the franchise theme layer — the local
//!   player's franchise powers swell their theme in, most recent franchise wins, never spammy
//!   (minimum hold, re-entry cooldown, keep-alive fade-out). Settings → Sound → Franchise themes.

use std::sync::Arc;

use bd_music::{MusicLibrary, MusicPlayer, StingerKind, TrackId};
use serde::{Deserialize, Serialize};

use crate::director::Scene;
use crate::engine::AudioEngine;
use crate::mixer::{MusicCommand, MusicSource};

/// [`MusicSource`] over bd_music's player.
pub struct ChipMusic {
    player: MusicPlayer,
}

impl ChipMusic {
    pub fn new(lib: Arc<MusicLibrary>, sample_rate: u32) -> Self {
        Self { player: MusicPlayer::new(lib, sample_rate) }
    }
}

impl MusicSource for ChipMusic {
    fn render(&mut self, out: &mut [f32]) {
        self.player.render(out);
    }
    fn command(&mut self, cmd: MusicCommand) {
        let c = match cmd {
            MusicCommand::Play { track, skip_intro } => bd_music::MusicCommand::Play { track: TrackId(track), skip_intro },
            MusicCommand::Stop => bd_music::MusicCommand::Stop,
            MusicCommand::Intensity(x) => bd_music::MusicCommand::SetIntensity(x),
            MusicCommand::Tempo { multiplier, ramp_secs } => bd_music::MusicCommand::SetTempo { multiplier, ramp_secs },
            MusicCommand::Stinger(k) => match StingerKind::ALL.get(k as usize) {
                Some(&k) => bd_music::MusicCommand::Stinger(k),
                None => return,
            },
            MusicCommand::Style(s) => bd_music::MusicCommand::SetStyle(match s {
                1 => bd_music::Style::Remix(None),
                2 => bd_music::Style::Shuffle,
                _ => bd_music::Style::Classic,
            }),
            MusicCommand::Overlay { layer, on } => bd_music::MusicCommand::Overlay { id: bd_music::OverlayId(layer), on },
            MusicCommand::Theme { track } => bd_music::MusicCommand::Theme { track: track.map(TrackId) },
        };
        self.player.command(c);
    }
    fn sub_bass_hpf(&self) -> Option<f32> {
        let s = self.player.status();
        s.sub_bass.then_some(s.hpf_hz)
    }
}

/// Settings → Music style.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MusicStyle {
    /// The chip arrangements.
    #[default]
    Classic,
    /// The remix variants (house, lofi, synthwave, trap, dnb, phonk, chip-EDM; 808 sub-bass).
    Remixes,
    /// Mix of both per track.
    Shuffle,
}

impl MusicStyle {
    pub fn next(self, dir: i32) -> Self {
        let all = [MusicStyle::Classic, MusicStyle::Remixes, MusicStyle::Shuffle];
        let i = all.iter().position(|s| *s == self).unwrap_or(0) as i32;
        all[(i + dir).rem_euclid(3) as usize]
    }
    pub fn label(self) -> &'static str {
        match self {
            MusicStyle::Classic => "CLASSIC",
            MusicStyle::Remixes => "REMIXES",
            MusicStyle::Shuffle => "SHUFFLE",
        }
    }
}

/// What the soundtrack follows.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MusicInputs {
    pub scene: Scene,
    /// Map key (`archipelago`, `miami`, `orbit`, `tutorial`, …).
    pub map: &'static str,
    /// Combat weight 0..1 (the mix director's).
    pub combat: f32,
    /// Someone is on their last stock / the last minute.
    pub final_stock: bool,
    /// Overtime.
    pub sudden_death: bool,
    /// Party mode match / card pick.
    pub party: bool,
    /// Match over: Some(true) = local player won.
    pub results: Option<bool>,
}

/// Franchise theme layer timing (seconds). See [`ThemeDirector`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThemeTiming {
    /// The theme fades out after this long without a use of its franchise.
    pub keep_alive: f32,
    /// A theme plays at least this long before another franchise can take over.
    pub min_hold: f32,
    /// After a theme ends, no theme starts for this long.
    pub reentry: f32,
    /// A use this recent still counts when a hold / cooldown runs out.
    pub fresh: f32,
    /// While a theme plays, re-assert it to the player at most this often (the player drops a
    /// theme under the big stingers; the next use brings it back).
    pub resend: f32,
}

impl Default for ThemeTiming {
    fn default() -> Self {
        Self { keep_alive: 12.0, min_hold: 6.0, reentry: 8.0, fresh: 4.0, resend: 1.5 }
    }
}

/// What the theme layer should do (→ [`MusicCommand::Theme`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeCmd {
    On(u16),
    Off,
}

/// The franchise theme layer's rules (pure; unit-tested). Themes are track indices of the music
/// library's `themes/` songs; the game resolves a used power to its franchise's theme.
///
/// * Local player only: the game calls [`Self::used`] for the local player's own power uses.
/// * The first use swells the theme in (the player lands it on the next beat).
/// * Using the same franchise keeps it up; [`ThemeTiming::keep_alive`] seconds without a use fade
///   it out.
/// * Most recent franchise wins, but a theme holds at least [`ThemeTiming::min_hold`] seconds —
///   a use of another franchise meanwhile is remembered and takes over when the hold ends (if
///   it is still fresh).
/// * After a theme ends, [`ThemeTiming::reentry`] seconds pass before the next one can start.
/// * Disabled (the setting) or outside a live match: off at once, uses ignored.
#[derive(Clone, Debug)]
pub struct ThemeDirector {
    pub timing: ThemeTiming,
    enabled: bool,
    allowed: bool,
    current: Option<u16>,
    since_use: f32,
    held: f32,
    since_end: f32,
    since_sent: f32,
    /// A use that is waiting for the hold / cooldown: (theme, seconds since that use).
    queued: Option<(u16, f32)>,
}

impl Default for ThemeDirector {
    fn default() -> Self {
        Self {
            timing: ThemeTiming::default(),
            enabled: true,
            allowed: false,
            current: None,
            since_use: 0.0,
            held: 0.0,
            since_end: f32::INFINITY,
            since_sent: f32::INFINITY,
            queued: None,
        }
    }
}

impl ThemeDirector {
    /// The theme the layer should be playing now.
    pub fn current(&self) -> Option<u16> {
        self.current
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    fn start(&mut self, t: u16) -> Option<ThemeCmd> {
        self.current = Some(t);
        self.since_use = 0.0;
        self.held = 0.0;
        self.since_sent = 0.0;
        self.queued = None;
        Some(ThemeCmd::On(t))
    }

    fn end(&mut self) -> Option<ThemeCmd> {
        self.queued = None;
        self.current.take().map(|_| {
            self.since_end = 0.0;
            ThemeCmd::Off
        })
    }

    /// The local player used a power whose franchise theme is `theme`.
    pub fn used(&mut self, theme: u16) -> Option<ThemeCmd> {
        if !(self.enabled && self.allowed) {
            return None;
        }
        match self.current {
            Some(c) if c == theme => {
                self.since_use = 0.0;
                if self.since_sent >= self.timing.resend {
                    self.since_sent = 0.0;
                    return Some(ThemeCmd::On(theme));
                }
                None
            }
            Some(_) if self.held >= self.timing.min_hold => self.start(theme),
            None if self.since_end >= self.timing.reentry => self.start(theme),
            _ => {
                self.queued = Some((theme, 0.0));
                None
            }
        }
    }

    /// Per frame. `allowed` = in a live match (not menus, loading, or the results screen).
    pub fn update(&mut self, dt: f32, allowed: bool) -> Option<ThemeCmd> {
        let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        self.allowed = allowed;
        self.since_use += dt;
        self.held += dt;
        self.since_end += dt;
        self.since_sent += dt;
        if let Some(q) = self.queued.as_mut() {
            q.1 += dt;
        }
        if !(self.enabled && allowed) {
            return self.end();
        }
        if self.current.is_some() && self.since_use >= self.timing.keep_alive {
            return self.end();
        }
        match self.queued {
            Some((_, age)) if age > self.timing.fresh => {
                self.queued = None;
                None
            }
            Some((t, _)) if self.current.is_some() && self.held >= self.timing.min_hold => self.start(t),
            Some((t, _)) if self.current.is_none() && self.since_end >= self.timing.reentry => self.start(t),
            _ => None,
        }
    }

    /// The setting. Turning it off ends a playing theme at once.
    pub fn set_enabled(&mut self, on: bool) -> Option<ThemeCmd> {
        self.enabled = on;
        if on { None } else { self.end() }
    }

    /// Leaving a match / stopping the music: off, and no cooldown carried over.
    pub fn reset(&mut self) -> Option<ThemeCmd> {
        let c = self.end();
        self.since_end = f32::INFINITY;
        c
    }
}

/// Game-thread soundtrack director.
pub struct MusicDirector {
    lib: Arc<MusicLibrary>,
    pub style: MusicStyle,
    sent_style: Option<MusicStyle>,
    current: Option<&'static str>,
    fighting: bool,
    calm_for: f32,
    since_ko: f32,
    ended: bool,
    /// Overlay layers switched on (bit per `bd_music::OverlayId`).
    overlays: u8,
    /// The franchise theme layer.
    pub themes: ThemeDirector,
}

impl MusicDirector {
    pub fn new(lib: Arc<MusicLibrary>, style: MusicStyle, seed: u64) -> Self {
        let _ = seed;
        Self {
            lib,
            style,
            sent_style: None,
            current: None,
            fighting: false,
            calm_for: 0.0,
            since_ko: 99.0,
            ended: false,
            overlays: 0,
            themes: ThemeDirector::default(),
        }
    }

    fn send_theme(&self, c: Option<ThemeCmd>, engine: &mut AudioEngine) {
        match c {
            Some(ThemeCmd::On(t)) => engine.music(MusicCommand::Theme { track: Some(t) }),
            Some(ThemeCmd::Off) => engine.music(MusicCommand::Theme { track: None }),
            None => {}
        }
    }

    /// The franchise theme a power belongs to (`power_id`, its source file `base_one_piece.ron`,
    /// its own `music` key), resolved through `themes/franchises.ron`. `None` = no theme.
    pub fn franchise_theme(&self, power_id: &str, file: Option<&str>, key: Option<&str>) -> Option<u16> {
        bd_music::franchise::resolve(&self.lib.franchises, power_id, file, key).map(|f| f.theme.0)
    }

    /// The local player used a power of the franchise whose theme is `theme` (see
    /// [`Self::franchise_theme`]).
    pub fn theme_used(&mut self, theme: u16, engine: &mut AudioEngine) {
        let c = self.themes.used(theme);
        self.send_theme(c, engine);
    }

    /// Settings → Sound → Franchise themes.
    pub fn set_themes_enabled(&mut self, on: bool, engine: &mut AudioEngine) {
        let c = self.themes.set_enabled(on);
        self.send_theme(c, engine);
    }

    /// Theme layer off (leaving a match).
    pub fn themes_off(&mut self, engine: &mut AudioEngine) {
        let c = self.themes.reset();
        self.send_theme(c, engine);
    }

    /// Turn a named overlay layer ("drums": Gear Fifth) on / off. Unknown names do nothing.
    /// Returns whether the name is a layer.
    pub fn overlay(&mut self, name: &str, on: bool, engine: &mut AudioEngine) -> bool {
        let Some(id) = bd_music::overlay_id(name) else { return false };
        let bit = 1u8 << (id.0 & 7);
        if (self.overlays & bit != 0) != on {
            self.overlays ^= bit;
            engine.music(MusicCommand::Overlay { layer: id.0, on });
        }
        true
    }

    /// Overlay layers currently on (bit per `bd_music::OverlayId`).
    pub fn overlays(&self) -> u8 {
        self.overlays
    }

    /// Every overlay layer off (leaving a match).
    pub fn overlays_off(&mut self, engine: &mut AudioEngine) {
        for i in 0..8u8 {
            if self.overlays & (1 << i) != 0 {
                engine.music(MusicCommand::Overlay { layer: i, on: false });
            }
        }
        self.overlays = 0;
    }

    pub fn current(&self) -> Option<&'static str> {
        self.current
    }

    /// The track a state wants.
    fn want(&mut self, inp: &MusicInputs, dt: f32) -> &'static str {
        match inp.scene {
            Scene::Menu => return "menu",
            Scene::Lobby => return if inp.party { "party" } else { "lobby" },
            Scene::InGame => {}
        }
        if let Some(_won) = inp.results {
            return if inp.party { "party_results" } else { "results" };
        }
        if inp.sudden_death {
            return "sudden_death";
        }
        if inp.final_stock {
            return "final_stock";
        }
        if inp.party {
            return "party";
        }
        // Explore ↔ combat with hysteresis: fight starts at 0.5, ends after 8 s under 0.15.
        if inp.combat > 0.5 {
            self.fighting = true;
            self.calm_for = 0.0;
        } else if inp.combat < 0.15 {
            self.calm_for += dt;
            if self.calm_for > 8.0 {
                self.fighting = false;
            }
        }
        match (inp.map, self.fighting) {
            ("tutorial", _) => "tutorial",
            ("miami", _) => "miami",
            ("orbit", false) => "orbit_float",
            ("orbit", true) => "orbit_combat",
            // Backrooms: the lobby muzak drifting through empty offices; Orbit's tension when it kicks off.
            ("backrooms", false) => "lobby",
            ("backrooms", true) => "orbit_combat",
            // Sakura Keep: Vivaldi's Spring for the blossom garden, its Summer Storm when it kicks off.
            ("sakura", false) => "archipelago_explore",
            ("sakura", true) => "archipelago_combat",
            // Ember: Summer Storm's brooding G-minor tremolo over the lava; the Ride of the Valkyries
            // (Orbit's combat track — its trap / DnB / synthwave remixes under the Remix style) when it kicks off.
            ("ember", false) => "archipelago_combat",
            ("ember", true) => "orbit_combat",
            // Mirrorfold: the Blue Danube waltzing while the city turns over itself (its synthwave /
            // lofi remixes under the Remix style); Vivaldi's Summer Presto when it kicks off.
            ("mirror", false) => "orbit_float",
            ("mirror", true) => "archipelago_combat",
            // High Roller: Alla Turca as a neon lounge (its house / synthwave / phonk remixes under the
            // Remix style) on the casino floor; Offenbach's Can-can galop when it kicks off.
            ("casino", false) => "miami",
            ("casino", true) => "party",
            // Valley of the End: Satie's Gymnopédie (lofi / synthwave remixes under the Remix style) for
            // the solemn gorge; the Ride of the Valkyries for the duel at the falls.
            ("valley", false) => "lobby",
            ("valley", true) => "orbit_combat",
            // Cybertron: Orbit's floating space theme over the metal plains (its lofi / synthwave remixes
            // under the Remix style); the Ride of the Valkyries (trap / DnB / synthwave remixes) when it kicks off.
            ("cybertron", false) => "orbit_float",
            ("cybertron", true) => "orbit_combat",
            // Turtle Lair: Eine kleine Nachtmusik — "a little night music" under the city (its lo-fi /
            // house remixes under the Remix style); Vivaldi's Summer Storm (DnB / trap / chip-EDM
            // remixes) when it kicks off.
            ("lair", false) => "menu",
            ("lair", true) => "archipelago_combat",
            // Giant's Room: the Minuet in G with its music-box sparkle (a kid's bedroom; chip-EDM /
            // house remixes under the Remix style); Offenbach's Can-can galop when it kicks off.
            ("giant", false) => "tutorial",
            ("giant", true) => "party",
            // Hyperline: the chase — Flight of the Bumblebee (its DnB remix at 174 under the Remix style)
            // riding the train; In the Hall of the Mountain King (168 bpm; DnB / trap / phonk) when it kicks off.
            ("hyperline", false) => "final_stock",
            ("hyperline", true) => "sudden_death",
            // The Lookout: Vivaldi's Spring floating over the clouds (its lo-fi / synthwave remixes under
            // the Remix style); the Ride of the Valkyries when the beams start flying. (Using a Dragon
            // Ball kit/power still swells its ORIGINAL franchise theme on the theme layer.)
            ("lookout", false) => "archipelago_explore",
            ("lookout", true) => "orbit_combat",
            (_, false) => "archipelago_explore",
            (_, true) => "archipelago_combat",
        }
    }

    /// Per frame.
    pub fn update(&mut self, dt: f32, inp: &MusicInputs, engine: &mut AudioEngine) {
        self.since_ko += dt.max(0.0);
        if self.sent_style != Some(self.style) {
            self.sent_style = Some(self.style);
            let s = match self.style {
                MusicStyle::Classic => 0,
                MusicStyle::Remixes => 1,
                MusicStyle::Shuffle => 2,
            };
            engine.music(MusicCommand::Style(s));
        }
        if inp.results.is_some() && inp.scene == Scene::InGame {
            if !self.ended {
                self.ended = true;
                let k = if inp.results == Some(true) { StingerKind::Victory } else { StingerKind::Defeat };
                self.stinger(k, engine);
            }
        } else {
            self.ended = false;
        }
        let live = inp.scene == Scene::InGame && inp.results.is_none();
        let c = self.themes.update(dt, live);
        self.send_theme(c, engine);
        let want = self.want(inp, dt);
        if self.current != Some(want) {
            // Victory/defeat end the track by themselves; results music follows once it has.
            if let Some(id) = self.lib.track_id(want) {
                engine.music(MusicCommand::Play { track: id.0, skip_intro: false });
            }
            self.current = Some(want);
        }
    }

    /// Fire a stinger (KOs throttled to one per 2 s).
    pub fn stinger(&mut self, k: StingerKind, engine: &mut AudioEngine) {
        if k == StingerKind::Ko {
            if self.since_ko < 2.0 {
                return;
            }
            self.since_ko = 0.0;
        }
        engine.music(MusicCommand::Stinger(k.index() as u8));
    }

    /// Stop the music (loading, alt-tab).
    pub fn stop(&mut self, engine: &mut AudioEngine) {
        let _ = self.themes.reset();
        engine.music(MusicCommand::Stop);
        self.current = None;
    }
}

#[cfg(test)]
mod theme_tests {
    use super::{ThemeCmd, ThemeDirector};

    const A: u16 = 3;
    const B: u16 = 7;

    /// Run `secs` of frames at 60 Hz, collecting the commands.
    fn run(d: &mut ThemeDirector, secs: f32, live: bool) -> Vec<ThemeCmd> {
        let mut out = Vec::new();
        for _ in 0..(secs * 60.0).round() as usize {
            out.extend(d.update(1.0 / 60.0, live));
        }
        out
    }

    fn live() -> ThemeDirector {
        let mut d = ThemeDirector::default();
        let _ = d.update(0.0, true);
        d
    }

    #[test]
    fn first_use_swells_in_and_keep_alive_fades_it_out() {
        let mut d = live();
        let t = d.timing;
        assert_eq!(d.used(A), Some(ThemeCmd::On(A)));
        assert_eq!(d.current(), Some(A));
        // Using it keeps it up (re-asserted at most every `resend` seconds, never spammed).
        let mut sent = 0;
        for _ in 0..20 {
            assert!(run(&mut d, 0.5, true).is_empty());
            sent += usize::from(d.used(A).is_some());
        }
        assert!((5..=8).contains(&sent), "re-asserted {sent} times in 10 s of constant use");
        // 12 s without a use: off.
        let off = run(&mut d, t.keep_alive - 0.1, true);
        assert!(off.is_empty(), "still up just before the keep-alive runs out");
        assert_eq!(run(&mut d, 0.2, true), vec![ThemeCmd::Off]);
        assert_eq!(d.current(), None);
    }

    #[test]
    fn most_recent_franchise_wins_after_the_minimum_hold() {
        let mut d = live();
        let t = d.timing;
        assert_eq!(d.used(A), Some(ThemeCmd::On(A)));
        let _ = run(&mut d, 2.0, true);
        // Too soon: remembered, not switched.
        assert_eq!(d.used(B), None);
        assert_eq!(d.current(), Some(A));
        // The hold runs out while the B use is still fresh: B takes over.
        let c = run(&mut d, t.min_hold - 2.0 + 0.1, true);
        assert_eq!(c, vec![ThemeCmd::On(B)]);
        assert_eq!(d.current(), Some(B));
        // After the hold, a use switches immediately.
        let _ = run(&mut d, t.min_hold, true);
        assert_eq!(d.used(A), Some(ThemeCmd::On(A)));
    }

    #[test]
    fn a_stale_queued_use_is_dropped() {
        let mut d = live();
        let t = d.timing;
        let _ = d.used(A);
        let _ = run(&mut d, 0.5, true);
        let _ = d.used(B);
        // B's single use (at 0.5 s) goes stale (`fresh`) before A's hold ends (`min_hold`).
        assert!(t.fresh < t.min_hold - 0.5);
        let c = run(&mut d, t.min_hold, true);
        assert!(c.is_empty(), "{c:?}");
        assert_eq!(d.current(), Some(A), "a single old use does not yank the music over");
    }

    #[test]
    fn reentry_cooldown_after_a_theme_ends() {
        let mut d = live();
        let t = d.timing;
        let _ = d.used(A);
        let _ = run(&mut d, t.keep_alive + 0.1, true);
        assert_eq!(d.current(), None);
        // Right after the fade-out: no theme (cooldown), the use is queued...
        assert_eq!(d.used(A), None);
        let _ = run(&mut d, 2.0, true);
        assert_eq!(d.current(), None);
        // ...and a fresh use when the cooldown ends brings it back.
        let _ = run(&mut d, t.reentry - 2.0 - t.fresh + 0.5, true);
        assert_eq!(d.used(B), None);
        let c = run(&mut d, t.fresh, true);
        assert_eq!(c, vec![ThemeCmd::On(B)]);
    }

    #[test]
    fn the_setting_and_leaving_the_match_turn_it_off() {
        let mut d = live();
        let _ = d.used(A);
        assert_eq!(d.set_enabled(false), Some(ThemeCmd::Off));
        assert_eq!(d.used(A), None, "disabled: uses ignored");
        assert!(run(&mut d, 20.0, true).is_empty(), "stays off");
        assert_eq!(d.set_enabled(true), None);
        assert!(d.enabled());
        assert_eq!(d.used(A), Some(ThemeCmd::On(A)), "back on: no cooldown from the setting");
        // Results screen / menus: off at once and uses are ignored there.
        assert_eq!(run(&mut d, 0.1, false), vec![ThemeCmd::Off]);
        assert_eq!(d.used(B), None);
        // Leaving the match resets the cooldown.
        let _ = d.update(0.1, true);
        let _ = d.reset();
        assert_eq!(d.used(B), Some(ThemeCmd::On(B)));
        assert_eq!(d.reset(), Some(ThemeCmd::Off));
    }
}
