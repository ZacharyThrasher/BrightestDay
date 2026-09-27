//! THE TOOLBOX, client side (docs/toolbox/): what a typed / spoken line means when it is a kit move
//! ("reinhardt charge"), a borrowed move (the shout of a power you don't hold), a held move said by
//! name, or a CHAIN ("nightcrawler teleport -> reinhardt ultimate -> doomfist punch", "amaterasu
//! then chidori", "summon a dragon, rasengan"). Such a line becomes ONE `Cmd::Toolbox` (the host
//! fires the steps in order, each when the previous move's active phase ends — bd_sim
//! `powers::chain`). Anything else (a plain construct, an incantation, a stand rush, a single power
//! NAME that grants) returns `None` and takes the console's usual path untouched.
//!
//! Voice parity: the Whisper transcript goes through the same function (commas, "then", "and then",
//! "into", "followed by" all split), and the pure part is shared with the phrase preview tool and
//! the `powers_check` validator (`bd_sim::powers::toolbox`).

use bd_lexicon::{Lexicon, ResolutionKind};
use bd_core::PlayerId;
use bd_sim::powers::chain::ToolStep;
use bd_sim::powers::compose::{self, ComposedMove};
use bd_sim::powers::toolbox::{split_chain_with, MoveIndex, Reading};
use bd_sim::powers::{PlayerPower, PowerPhrase, PowerTable};
use bd_sim::Aim;

/// How one step of a line reads for this player (`grant` filter applied: a phrase the lexicon
/// resolves to a GRANT — a power / tier name, "power up", a random fruit — is never a borrow).
pub fn read_step(lex: &Lexicon, held: Option<&PlayerPower>, powers_on: bool, text: &str, spoken: bool) -> Reading {
    let t = PowerTable::get();
    let ix = MoveIndex::shipped();
    let held = held.filter(|p| p.id.is_some());
    let r = if powers_on {
        ix.read(t, held, text)
    } else {
        Reading::Plain
    };
    match r {
        Reading::Borrow(_) => {
            let res = if spoken { lex.resolve_spoken(text) } else { lex.resolve(text) };
            if res.recipe.grants.is_some() { Reading::Plain } else { r }
        }
        // VOICE PARITY: Whisper spells a shout it doesn't know by sound ("razengan", "chi dori").
        // A spoken line the lexicon would only improvise is matched to a shout by its phonetic key.
        Reading::Plain if spoken && powers_on => match spoken_shout(text) {
            Some(mv) if !Lexicon::is_clean_thing(&lex.resolve_spoken(text)) => Reading::Borrow(mv),
            _ => Reading::Plain,
        },
        other => other,
    }
}

/// A borrowable shout that SOUNDS like `text` (phonetic key ≥ 5 letters; the first claim wins).
fn spoken_shout(text: &str) -> Option<bd_sim::powers::toolbox::MoveRef> {
    use std::collections::HashMap;
    use std::sync::OnceLock;
    static BY_SOUND: OnceLock<HashMap<String, bd_sim::powers::toolbox::MoveRef>> = OnceLock::new();
    let map = BY_SOUND.get_or_init(|| {
        let mut all: Vec<(&str, bd_sim::powers::toolbox::MoveRef)> = MoveIndex::shipped().shouts().collect();
        all.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(b.0))); // table order: deterministic first claim
        let mut m = HashMap::with_capacity(all.len());
        for (s, mv) in all {
            let k = bd_lexicon::phonetic_key(s);
            if k.len() >= 5 {
                m.entry(k).or_insert(mv);
            }
        }
        m
    });
    let k = bd_lexicon::phonetic_key(&bd_sim::powers::norm_words(text));
    if k.len() < 5 {
        return None;
    }
    map.get(&k).copied()
}

/// The lexicon reads `s` as one clean THING (a construct / grant it fully understood — not a bare
/// modifier like "a big", not an improvisation).
pub fn understood(lex: &Lexicon, s: &str, spoken: bool) -> bool {
    let r = if spoken { lex.resolve_spoken(s) } else { lex.resolve(s) };
    Lexicon::is_clean_thing(&r)
}

/// The toolbox command for a line, or `None` (not a toolbox line: the console casts it as ever).
/// `aim` = the crosshair ray at commit time (aims the first step if it fires at once).
pub fn command(lex: &Lexicon, held: Option<&PlayerPower>, powers_on: bool, text: &str, spoken: bool) -> Option<Vec<ToolStep>> {
    command_for(lex, held, powers_on, text, spoken, &[])
}

/// [`command`] with the other players' names (`roster`), for described moves that name someone
/// ("shadow teleport behind ethan" — VERB GRAMMAR, docs/toolbox/COMPOSED_MOVE.md).
///
/// PRECEDENCE per step: a held move / kit move / borrowed shout (TOOLBOX CORE) → a power NAME or a
/// curated thing (the lexicon: grant, signature, icon) → a DESCRIBED move (the verb grammar: every
/// word understood, exactly one self-action verb) → a construct.
pub fn command_for(lex: &Lexicon, held: Option<&PlayerPower>, powers_on: bool, text: &str, spoken: bool, roster: &[(PlayerId, &str)]) -> Option<Vec<ToolStep>> {
    let fuzzy = |ws: &[&str]| sound_alike(lex, roster, ws, spoken);
    let names = compose::Names { players: roster, fuzzy: Some(&fuzzy) };
    let is_move = |s: &str| read_step(lex, held, powers_on, s, spoken).is_move() || composed(lex, powers_on, s, spoken, &names).is_some();
    let understood = |s: &str| understood(lex, s, spoken);
    let parts = split_chain_with(text, &is_move, Some(&understood));
    if parts.is_empty() {
        return None;
    }
    if parts.len() == 1 {
        // A single phrase: only a move reading becomes a toolbox command (a held move said by
        // name goes this way too, so it gets the input buffer); incantations keep their path.
        return match read_step(lex, held, powers_on, &parts[0], spoken) {
            Reading::Kit(mv) => Some(vec![ToolStep::Move { mv, kit: true }]),
            Reading::Borrow(mv) => Some(vec![ToolStep::Move { mv, kit: false }]),
            Reading::Held(PowerPhrase::Move(sel)) => Some(vec![ToolStep::Held(sel)]),
            Reading::Plain | Reading::Held(PowerPhrase::Plain) => composed(lex, powers_on, &parts[0], spoken, &names).map(|c| vec![ToolStep::Composed(c)]),
            _ => None,
        };
    }
    let mut steps = Vec::with_capacity(parts.len());
    let mut specs: Vec<Option<ComposedMove>> = Vec::with_capacity(parts.len());
    for p in &parts {
        let step = match read_step(lex, held, powers_on, p, spoken) {
            Reading::Kit(mv) => ToolStep::Move { mv, kit: true },
            Reading::Borrow(mv) => ToolStep::Move { mv, kit: false },
            Reading::Held(PowerPhrase::Move(sel)) => ToolStep::Held(sel),
            Reading::Held(PowerPhrase::Incantation { rest }) => cast_step(lex, &rest, p, spoken, true),
            Reading::Held(PowerPhrase::Plain) | Reading::Plain => match composed(lex, powers_on, p, spoken, &names) {
                Some(c) => ToolStep::Composed(c),
                None => cast_step(lex, p, p, spoken, false),
            },
        };
        specs.push(match &step {
            ToolStep::Composed(c) => Some(*c),
            _ => None,
        });
        steps.push(step);
    }
    // "teleport behind ethan, fire uppercut": the untargeted strike goes for the same target
    compose::grammar::link_targets(&mut specs);
    for (st, sp) in steps.iter_mut().zip(specs) {
        if let (ToolStep::Composed(c), Some(s)) = (st, sp) {
            *c = s;
        }
    }
    Some(steps)
}

/// The DESCRIBED-move reading of one step (VERB GRAMMAR), if it has one and the lexicon doesn't
/// know the exact phrase as something curated (a signature, a grant, an icon it fully read).
pub fn composed(lex: &Lexicon, powers_on: bool, text: &str, spoken: bool, names: &compose::Names) -> Option<ComposedMove> {
    if !powers_on {
        return None;
    }
    let r = compose::read(text, names)?;
    let res = if spoken { lex.resolve_spoken(text) } else { lex.resolve(text) };
    (!compose::curated_by_lexicon(text, res.signature.is_some(), res.recipe.grants.is_some() || res.stand_rush.is_some(), &res.recipe.model)).then_some(r.spec)
}

/// Voice: a sound-alike player name at the start of `ws` ("eaton" → Ethan), via the lexicon's own
/// name targeting (the same matcher "missile at Jake" uses). `(player, words used)`.
fn sound_alike(lex: &Lexicon, roster: &[(PlayerId, &str)], ws: &[&str], spoken: bool) -> Option<(PlayerId, usize)> {
    if roster.is_empty() || ws.is_empty() {
        return None;
    }
    let names: Vec<&str> = roster.iter().map(|r| r.1).collect();
    // at most a two-word name, and only when the lexicon consumed exactly those words
    for n in (1..=ws.len().min(2)).rev() {
        let probe = format!("zap at {}", ws[..n].join(" "));
        if let Some(t) = lex.target_player(&probe, &names, spoken)
            && t.rest.trim().eq_ignore_ascii_case("zap")
        {
            return roster.get(t.player).map(|r| (r.0, n));
        }
    }
    None
}

/// A construct step of a chain: the lexicon's reading + the costume summon + a signature, like the
/// console's single cast (without the one-off FREEDOM steps: "it", "again", "at Jake").
fn cast_step(lex: &Lexicon, phrase: &str, said: &str, spoken: bool, incantation: bool) -> ToolStep {
    let mut res = if spoken { lex.resolve_spoken(phrase) } else { lex.resolve(phrase) };
    let modifier = res.kind == ResolutionKind::Graft;
    let summon = res.signature.is_none() && !incantation && !modifier;
    if summon
        && res.recipe.opening.is_none()
        && let Some(bare) = bd_shapes::costume::summon::mechanics_phrase(phrase, &res.recipe)
    {
        let b = lex.resolve(&bare);
        (res.recipe, res.graft, res.shape, res.alt_shape) = (b.recipe, b.graft, b.shape, b.alt_shape);
    }
    if summon && bd_shapes::costume::summon::apply(phrase, &mut res.recipe, &mut res.shape) {
        res.alt_shape = None;
    }
    if let Some(s) = res.signature.as_ref().filter(|_| !incantation) {
        let dummy = Aim::new(bd_core::Vec3::ZERO, bd_core::Vec3::NEG_Z);
        if let crate::sim_bridge::Cmd::Signature { recipe, shape, sig, .. } = crate::console::signature_cmd(res.recipe.clone(), res.shape.clone(), dummy, s, said, None, None) {
            return ToolStep::Cast { recipe, shape, alt_shape: None, incantation: false, signature: Some(sig) };
        }
    }
    ToolStep::Cast { recipe: Box::new(res.recipe), shape: res.shape, alt_shape: res.alt_shape, incantation, signature: None }
}

/// Wire hygiene for a client's steps (host side): at most `MAX_CHAIN`, shapes size-capped like a
/// cast's, signatures sanitised. The sim validates every step again as it fires.
pub fn sanitize_steps(steps: Vec<ToolStep>, small: impl Fn(Option<bd_core::shape::ShapeSpec>) -> Option<bd_core::shape::ShapeSpec>) -> Vec<ToolStep> {
    steps
        .into_iter()
        .take(bd_sim::powers::toolbox::MAX_CHAIN)
        .map(|s| match s {
            ToolStep::Cast { recipe, shape, alt_shape, incantation, signature } => {
                ToolStep::Cast { recipe, shape: small(shape), alt_shape: small(alt_shape), incantation, signature: signature.map(|g| g.sanitized()) }
            }
            other => other,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex() -> Option<Lexicon> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        Lexicon::load(dir.join("lexicon.bin")).ok()
    }

    /// Typing / voice → a toolbox command: how long the client-side reading takes (the sim then
    /// fires the first step on the next tick). Printed for the report; bounded generously.
    #[test]
    fn toolbox_lines_resolve_fast_and_right() {
        let Some(lex) = lex() else { return };
        let lines = [
            "almighty push",
            "amaterasu then chidori",
            "summon a dragon, rasengan",
            "bamf -> almighty push -> universal pull -> gum gum pistol",
            "a big, flaming hammer",
            "star platinum",
        ];
        // warm the indices (first use builds them once)
        let _ = command(&lex, None, true, "almighty push", false);
        let t0 = std::time::Instant::now();
        let mut out = Vec::new();
        for l in lines {
            out.push(command(&lex, None, true, l, false));
        }
        let per = t0.elapsed().as_secs_f64() * 1000.0 / lines.len() as f64;
        eprintln!("toolbox line → command: {per:.3} ms per line");
        assert!(per < 25.0, "{per} ms");
        assert!(matches!(out[0].as_deref(), Some([ToolStep::Move { kit: false, .. }])));
        assert_eq!(out[1].as_ref().map(|s| s.len()), Some(2));
        assert!(matches!(out[2].as_deref(), Some([ToolStep::Cast { .. }, ToolStep::Move { .. }])));
        assert_eq!(out[3].as_ref().map(|s| s.len()), Some(4));
        assert!(out[4].is_none(), "a plain construct keeps the console path: {:?}", out[4]);
        assert!(out[5].is_none(), "a power NAME grants (lexicon), never a toolbox command");
    }

    /// VERB GRAMMAR precedence: named moves stay named, constructs stay constructs, described moves
    /// become `ToolStep::Composed`, mixed chains work, voice matches typing.
    #[test]
    fn described_moves_and_precedence() {
        let Some(lex) = lex() else { return };
        let roster = [(PlayerId(2), "Ethan"), (PlayerId(3), "Mia")];
        let cmd = |s: &str, spoken: bool| command_for(&lex, None, true, s, spoken, &roster);
        let one = |s: &str| match cmd(s, false).as_deref() {
            Some([ToolStep::Composed(c)]) => Some(*c),
            _ => None,
        };
        let c = one("shadow teleport behind ethan").expect("described move");
        assert_eq!(c.target, compose::Target::Player(PlayerId(2)));
        assert_eq!(cmd("Shadow teleport behind Ethan.", true).as_deref(), Some(&[ToolStep::Composed(c)][..]), "voice == typing");
        for s in ["gravity punch", "fire uppercut", "lightning dash through him", "rewind 3 seconds", "pull everyone to me", "leap onto the tower", "swap with the dragon", "turn invisible"] {
            assert!(one(s).is_some(), "{s:?} should be a described move: {:?}", cmd(s, false));
        }
        // named moves keep their path
        assert!(matches!(cmd("almighty push", false).as_deref(), Some([ToolStep::Move { .. }])));
        // constructs keep theirs (the console casts them)
        for s in ["a dragon", "giant sword", "hammer", "a big flaming hammer", "drop a meteor on ethan", "throw a fireball", "star platinum"] {
            assert!(cmd(s, false).is_none(), "{s:?} must stay a construct / grant: {:?}", cmd(s, false));
        }
        // powers off: no described moves
        assert!(command_for(&lex, None, false, "gravity punch", false, &roster).is_none());
        // mixed chain: construct → described (targets "it") → described (same target)
        let ch = cmd("summon a dragon, shadow teleport behind it, fire uppercut", false).expect("chain");
        assert_eq!(ch.len(), 3, "{ch:?}");
        assert!(matches!(ch[0], ToolStep::Cast { .. }));
        assert!(matches!(ch[1], ToolStep::Composed(c) if c.target == compose::Target::It));
        assert!(matches!(ch[2], ToolStep::Composed(c) if c.target == compose::Target::Same));
        // "and" splits two described moves
        let ch = cmd("blink above him and slam", false).expect("blink + slam");
        assert_eq!(ch.len(), 2, "{ch:?}");
        // named + described
        let ch = cmd("bamf -> fire uppercut -> almighty push", false).expect("chain");
        assert_eq!(ch.len(), 3);
    }

    /// TOOLBOX: `Cmd::Toolbox` → `NetCmd::Toolbox` → wire → host `Cmd::Toolbox`: every step survives
    /// (moves, held selectors, casts with shapes / signatures), capped at MAX_CHAIN.
    #[test]
    fn toolbox_command_round_trips_the_wire() {
        use crate::net::proto::{NetCmd, C2S};
        use crate::sim_bridge::Cmd;
        use bd_sim::powers::toolbox::MoveRef;
        let recipe = Box::new(bd_core::mechanics::Recipe::unknown("dragon"));
        let mut steps = vec![
            ToolStep::Move { mv: MoveRef::new(412, 2, 5), kit: true },
            ToolStep::Held(bd_sim::powers::move_sel::move_of(1, 6)),
            ToolStep::Cast { recipe, shape: None, alt_shape: None, incantation: false, signature: None },
        ];
        steps.extend((0..10).map(|k| ToolStep::Move { mv: MoveRef::new(k, 0, 0), kit: false }));
        let aim = Aim::new(bd_core::Vec3::new(1.0, 2.0, 3.0), bd_core::Vec3::NEG_Z);
        let nc = NetCmd::from_cmd(&Cmd::Toolbox { steps: steps.clone(), aim: Some(aim) }, 7).expect("travels");
        let back: C2S = bd_net::codec::decode(&bd_net::codec::encode(&C2S::Cmd(nc))).unwrap();
        let C2S::Cmd(nc) = back else { panic!("not a command") };
        let Some(Cmd::Toolbox { steps: got, aim: Some(a) }) = nc.into_cmd() else { panic!("not a toolbox command") };
        assert_eq!(got.len(), bd_sim::powers::toolbox::MAX_CHAIN, "capped");
        assert_eq!(&got[..], &steps[..bd_sim::powers::toolbox::MAX_CHAIN]);
        assert_eq!((a.origin, a.dir), (aim.origin, aim.dir));
    }
}
