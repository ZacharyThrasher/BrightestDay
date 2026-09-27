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
use bd_sim::powers::chain::ToolStep;
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
        other => other,
    }
}

/// The lexicon reads `s` as one clean thing (a construct / grant it fully understood).
pub fn understood(lex: &Lexicon, s: &str, spoken: bool) -> bool {
    let r = if spoken { lex.resolve_spoken(s) } else { lex.resolve(s) };
    !Lexicon::is_fallback(&r)
}

/// The toolbox command for a line, or `None` (not a toolbox line: the console casts it as ever).
/// `aim` = the crosshair ray at commit time (aims the first step if it fires at once).
pub fn command(lex: &Lexicon, held: Option<&PlayerPower>, powers_on: bool, text: &str, spoken: bool) -> Option<Vec<ToolStep>> {
    let is_move = |s: &str| read_step(lex, held, powers_on, s, spoken).is_move();
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
            _ => None,
        };
    }
    let mut steps = Vec::with_capacity(parts.len());
    for p in &parts {
        steps.push(match read_step(lex, held, powers_on, p, spoken) {
            Reading::Kit(mv) => ToolStep::Move { mv, kit: true },
            Reading::Borrow(mv) => ToolStep::Move { mv, kit: false },
            Reading::Held(PowerPhrase::Move(sel)) => ToolStep::Held(sel),
            Reading::Held(PowerPhrase::Incantation { rest }) => cast_step(lex, &rest, p, spoken, true),
            Reading::Held(PowerPhrase::Plain) | Reading::Plain => cast_step(lex, p, p, spoken, false),
        });
    }
    Some(steps)
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
        assert!(out[4].is_none(), "a plain construct keeps the console path");
        assert!(out[5].is_none(), "a power NAME grants (lexicon), never a toolbox command");
    }
}
