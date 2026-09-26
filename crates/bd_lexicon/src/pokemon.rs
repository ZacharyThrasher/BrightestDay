//! Pokémon trainer grammar (vocabulary only — every result is an ordinary construct or graft):
//!
//! * `<pokemon> use <move>` / `<pokemon>, use <move>!` / `go <pokemon>, use <move>` / `<pokemon> <move>`
//!   → the Pokémon construct with the move's graft (mechanics + element + temperament) pre-applied.
//!   Any graft phrase works as the "move" ("charizard use wings", "pikachu use fire").
//! * `go <pokemon>` / `<pokemon> go` / `<pokemon> i choose you` / `i choose you <pokemon>` → the Pokémon.
//! * `use <move>` → the move as a graft (a bare move name that is also an everyday word, like "surf" or
//!   "earthquake", keeps its everyday reading on its own; "use surf" is the move).
//!
//! Moves come from the lexicon's `MOVS` table (PokeAPI via tools/lexicon_build/moves.py); curated
//! Pokémon move icons ("hyper beam", "volt tackle") count too. What the tags *do* is bd_sim's business.

use bd_core::mechanics::{Element, Form, FormCategory, Mechanic, ModelSource, Recipe, Temperament};

use crate::data::{F_POKEMON, MoveRec, NONE8, fnv64};
use crate::fuzzy::dl_distance;
use crate::norm::Tok;
use crate::parse::{Gram, bits_to_mechs, gram_of};
use crate::tables::{ALL_ELEMENT, ALL_FORM, ALL_TEMPERAMENT};
use crate::{Graft, Lexicon, Resolution, ResolutionKind};

/// What a move adds when grafted.
#[derive(Clone, Debug)]
pub(crate) struct MoveGraft {
    pub name: String,
    pub mechanics: Vec<Mechanic>,
    pub element: Option<Element>,
    pub temperament: Option<Temperament>,
    pub payload: Option<Box<Recipe>>,
}

impl MoveGraft {
    fn of_rec(m: &MoveRec) -> MoveGraft {
        MoveGraft {
            name: if m.d.is_empty() { m.k.clone() } else { m.d.clone() },
            mechanics: bits_to_mechs(m.mech),
            element: (m.element != NONE8).then(|| ALL_ELEMENT.get(m.element as usize).copied()).flatten().filter(|e| *e != Element::Light),
            temperament: (m.temper != NONE8).then(|| ALL_TEMPERAMENT.get(m.temper as usize).copied()).flatten(),
            payload: None,
        }
    }

    fn describe(&self) -> String {
        let mut parts: Vec<String> = self.mechanics.iter().map(|m| crate::tables::mechanic_name(*m).to_string()).collect();
        if let Some(e) = self.element {
            parts.push(crate::tables::element_name(e).to_string());
        }
        if let Some(t) = self.temperament {
            parts.push(format!("{t:?}"));
        }
        if parts.is_empty() {
            parts.push("nothing happens".to_string());
        }
        parts.join("+")
    }
}

const USE: &[&str] = &["use", "uses", "used", "using"];

impl Lexicon {
    /// Is `words` (or a 1–3 word window of it) a Pokémon name?
    fn pokemon_span(&self, words: &[&str]) -> bool {
        for len in 1..=3usize.min(words.len()) {
            for start in 0..=words.len() - len {
                let k = words[start..start + len].join(" ");
                if let Some(d) = self.icons.alias.get(&k).and_then(|&i| self.icons.defs.get(i)) {
                    let src = d.source.to_lowercase();
                    if (src.contains("pokemon") || src.contains("pokémon")) && d.recipe().form.category() == FormCategory::Creature {
                        return true;
                    }
                }
                if self.entry(&k).is_some_and(|(_, r)| r.flags & F_POKEMON != 0) {
                    return true;
                }
            }
        }
        false
    }

    /// A move by (normalised) name: the move table (exact, squashed "thunder bolt" → thunderbolt, then a
    /// small typo distance), then curated Pokémon move icons ("hyper beam", "volt tackle").
    pub(crate) fn find_move(&self, key: &str, spoken: bool) -> Option<MoveGraft> {
        let key = key.trim();
        if key.is_empty() {
            return None;
        }
        if let Some(d) = self.data.as_ref() {
            if let Some(m) = d.move_rec(key) {
                return Some(MoveGraft::of_rec(m));
            }
            let squashed: String = key.chars().filter(|c| !c.is_whitespace()).collect();
            if let Some(m) = d.moves.iter().find(|m| m.k.len() >= squashed.len() && m.k.bytes().filter(|b| *b != b' ').eq(squashed.bytes()))
            {
                return Some(MoveGraft::of_rec(m));
            }
            let maxd = if key.len() >= 10 || spoken {
                2
            } else if key.len() >= 5 {
                1
            } else {
                0
            };
            if maxd > 0 {
                let first = key.as_bytes().first().copied();
                let mut best: Option<(usize, &MoveRec)> = None;
                for m in &d.moves {
                    if m.k.as_bytes().first().copied() != first || m.k.len().abs_diff(key.len()) > maxd {
                        continue;
                    }
                    if let Some(dist) = dl_distance(key.as_bytes(), m.k.as_bytes(), maxd)
                        && best.is_none_or(|b| dist < b.0)
                    {
                        best = Some((dist, m));
                    }
                }
                if let Some((_, m)) = best {
                    return Some(MoveGraft::of_rec(m));
                }
            }
        }
        let d = self.icons.alias.get(key).and_then(|&i| self.icons.defs.get(i))?;
        let src = d.source.to_lowercase();
        if !(src.contains("pokemon") || src.contains("pokémon")) || d.recipe().form.category() == FormCategory::Creature {
            return None;
        }
        let r = d.recipe();
        Some(MoveGraft {
            name: r.name.clone(),
            mechanics: r.mechanics.clone(),
            element: Some(r.element).filter(|e| *e != Element::Light),
            temperament: Some(r.temperament).filter(|t| *t != Temperament::Calm),
            payload: None,
        })
    }

    /// A bare move (not a lexicon entry) as a graft resolution: its construct when cast alone, its graft
    /// when aimed at your own construct.
    fn move_resolution(&self, key: &str, mv: &MoveGraft) -> Resolution {
        let rec = self.data.as_ref().and_then(|d| d.move_rec(key)).cloned();
        let mut recipe = Recipe::unknown(&mv.name);
        if let Some(m) = &rec {
            recipe.form = ALL_FORM.get(m.form as usize).copied().unwrap_or(Form::Blob0);
            recipe.scale = m.scale;
            recipe.mass = m.mass;
        }
        recipe.mechanics = mv.mechanics.clone();
        recipe.element = mv.element.unwrap_or_default();
        if let Some(t) = mv.temperament {
            recipe.temperament = t;
            recipe.intensity = 0.6;
        }
        recipe.model = ModelSource::Procedural { seed: fnv64(key.as_bytes()) as u32 };
        Resolution {
            kind: ResolutionKind::Graft,
            recipe: recipe.normalized(),
            graft: Graft {
                mechanics: mv.mechanics.clone(),
                element: mv.element,
                scale_mul: 1.0,
                temperament: mv.temperament,
                intensity: mv.temperament.map(|_| 0.6),
                ..Default::default()
            },
            shape: None,
            alt_shape: None,
            incantation: None,
            incantation_powers: Vec::new(),
            stand_rush: None,
            confidence: 0.9,
            explanation: vec![format!("{} \u{2192} {} (Pok\u{e9}mon move)", mv.name, mv.describe())],
            ignored: Vec::new(),
        }
    }

    /// The trainer grammar (see module docs). `None` = not a trainer command; resolve normally.
    pub(crate) fn pokemon_command(&self, toks: &[Tok], bangs: usize, spoken: bool) -> Option<Resolution> {
        let is_filler = |w: &str| matches!(gram_of(w), Some(Gram::Article | Gram::Filler | Gram::Cast));
        let mut a = 0;
        let mut b = toks.len();
        while a < b && is_filler(&toks[a].text) {
            a += 1;
        }
        while b > a && matches!(gram_of(&toks[b - 1].text), Some(Gram::Filler)) {
            b -= 1;
        }
        let mut t: Vec<Tok> = toks[a..b].to_vec();
        if t.len() < 2 {
            return None;
        }
        let whole = t.iter().map(|x| x.text.as_str()).collect::<Vec<_>>().join(" ");
        if self.icons.alias.contains_key(&whole) || self.entry(&whole).is_some() {
            return None; // an exact name ("go kart") is never a command
        }
        // "i choose you" (either end)
        let mut chose = false;
        for pat in
            [&["i", "choose", "you"][..], &["i", "choose", "u"], &["i", "choose", "thee"], &["choose", "you"], &["i", "chose", "you"]]
        {
            let n = pat.len();
            if t.len() > n && t[t.len() - n..].iter().zip(pat).all(|(x, p)| x.text == *p) {
                t.truncate(t.len() - n);
                chose = true;
                break;
            }
            if t.len() > n && t[..n].iter().zip(pat).all(|(x, p)| x.text == *p) {
                t.drain(..n);
                chose = true;
                break;
            }
        }
        // "go <x>", "<x> go", "go go <x>"
        let mut go = false;
        while t.len() > 1 && t[0].text == "go" {
            t.remove(0);
            go = true;
        }
        while t.len() > 1 && t.last().is_some_and(|x| x.text == "go") {
            t.pop();
            go = true;
        }
        while !t.is_empty() && is_filler(&t[0].text) {
            t.remove(0);
        }
        let words: Vec<String> = t.iter().map(|x| x.text.clone()).collect();
        let w: Vec<&str> = words.iter().map(|s| s.as_str()).collect();
        // split "<x> use <move>"
        let use_at = w.iter().position(|x| USE.contains(x));
        let (left, right, explicit_use): (Vec<Tok>, Vec<Tok>, bool) = match use_at {
            Some(i) => (t[..i].to_vec(), t[i + 1..].to_vec(), true),
            None => {
                // implicit "<pokemon> <move>": the longest move suffix after a Pokémon name
                let mut split = None;
                for j in 1..w.len() {
                    let r = w[j..].join(" ");
                    if self.pokemon_span(&w[..j]) && self.data.as_ref().is_some_and(|d| d.move_rec(&r).is_some()) {
                        split = Some(j);
                        break;
                    }
                }
                match split {
                    Some(j) => (t[..j].to_vec(), t[j..].to_vec(), false),
                    None => (t.clone(), Vec::new(), false),
                }
            }
        };
        let left_words: Vec<&str> = left.iter().map(|x| x.text.as_str()).filter(|x| !is_filler(x)).collect();
        let right_key = right.iter().map(|x| x.text.as_str()).filter(|x| !is_filler(x)).collect::<Vec<_>>().join(" ");
        let is_mon = self.pokemon_span(&left_words);
        let mv = if right_key.is_empty() { None } else { self.find_move(&right_key, spoken) };

        // "use <move>" alone
        if left_words.is_empty() {
            if !explicit_use {
                return None;
            }
            let mv = mv?;
            // a move that is also a lexicon entry resolves through the normal path (it is a graft there)
            if self.entry(&right_key).is_some_and(|(_, r)| r.flags & crate::data::F_MOVE != 0) {
                return Some(self.resolve_tokens(right, bangs, spoken, 0));
            }
            return Some(self.move_resolution(&right_key, &mv));
        }
        if right_key.is_empty() {
            // "go pikachu", "pikachu i choose you"
            if !(chose || (go && is_mon)) {
                return None;
            }
            let mut res = self.resolve_tokens(left, bangs, spoken, 0);
            res.explanation.insert(0, "trainer call \u{2192} summon".to_string());
            return Some(res);
        }
        if !explicit_use && mv.is_none() {
            return None;
        }
        if !(is_mon || mv.is_some() || chose || go) {
            return None; // "a sword i can use on him" is not a trainer command
        }
        let mut res = self.resolve_tokens(left, bangs, spoken, 0);
        if res.kind != ResolutionKind::Construct {
            return None;
        }
        // the move: a real move, else any graft phrase ("use fire", "use wings")
        let mv = match mv {
            Some(m) => m,
            None => {
                let g = self.resolve_tokens(right.clone(), bangs, spoken, 0);
                res.ignored.extend(g.ignored.iter().cloned());
                MoveGraft {
                    name: right_key.clone(),
                    mechanics: g.graft.mechanics.clone(),
                    element: g.graft.element,
                    temperament: g.graft.temperament,
                    payload: g.graft.payload.clone(),
                }
            }
        };
        let r = &mut res.recipe;
        r.mechanics.extend(mv.mechanics.iter().copied());
        if let Some(e) = mv.element {
            r.element = e;
        }
        if let Some(tp) = mv.temperament {
            r.temperament = tp;
            r.intensity = r.intensity.max(0.6);
        }
        if mv.payload.is_some() {
            r.payload = mv.payload.clone();
        }
        res.recipe = std::mem::replace(&mut res.recipe, Recipe::unknown("")).normalized();
        res.graft.mechanics.extend(mv.mechanics.iter().copied());
        res.graft.mechanics.sort();
        res.graft.mechanics.dedup();
        if mv.element.is_some() {
            res.graft.element = mv.element;
        }
        if mv.temperament.is_some() {
            res.graft.temperament = mv.temperament;
            res.graft.intensity = Some(0.6);
        }
        res.explanation.insert(0, format!("use {} \u{2192} +{} (move)", mv.name, mv.describe()));
        res.confidence = res.confidence.max(0.8);
        Some(res)
    }
}
