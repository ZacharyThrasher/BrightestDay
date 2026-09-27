//! Transformer grammar: every way a player asks for a transforming robot ("a helicopter robot", "a
//! robot that turns into a train", "a transformer that turns into a t rex", "a transforming wolf", "a
//! car that transforms into a robot", "a jet bot", "a dinobot", "a predacon", "an autobot car") becomes
//! ONE `Transforms` construct whose form / features / mass describe the ALT MODE: bd_shapes builds the
//! robot mode from the alt model, and bd_sim picks the combat archetype from the same tags
//! (`assets/tuning/combat.ron` `transformer.rules`). Piloted comes later, from the intent stage
//! (`intent.rs::naturally_piloted`: a single thing that Transforms is driven by default).
//!
//! Runs first in `parse.rs::resolve_tokens` (depth 0). The alt-mode words resolve with the ordinary
//! grammar (this stage off); `data/transformer.ron` then corrects the alt mode by its head noun (a bike
//! is Wheels + Exhaust without a cab, a train gets its plow, a gorilla walks on four legs), sizes it as a
//! transformer (clamped to 4-6 m; "small / mini / suit" = the power suit you wear, <= 2.6 m; "giant" =
//! bigger as usual) and makes it ONE machine (no packs, no drops). Everything is vocabulary + rules from
//! data: no name is special-cased here.
//!
//! Anything that doesn't fit returns None and the ordinary grammar reads the phrase exactly as before:
//! "robot", "robot dog" / "robot dinosaur" (a robot + an animal is a mechanical animal, not a
//! transformer — only a transform word or a franchise word makes it one), "robot arm", "a car", curated
//! names ("optimus prime", "bumblebee robot"), commands ("transform", "roll out", "combine").

use std::sync::OnceLock;

use bd_core::mechanics::{Element, Feature, Form, Mechanic, ModelSource, Temperament};
use serde::Deserialize;

use crate::fuzzy::dl_distance;
use crate::norm::{Tok, singulars};
use crate::parse::{Gram, gram_of};
use crate::{Lexicon, Resolution, ResolutionKind};

/// Alt-mode corrections by head noun (`alts` in `data/transformer.ron`).
#[derive(Deserialize)]
struct AltRow {
    words: Vec<String>,
    #[serde(default)]
    alias: Vec<(String, String)>,
    #[serde(default)]
    form: Option<Form>,
    #[serde(default)]
    add: Vec<Feature>,
    #[serde(default)]
    any: Vec<Feature>,
    #[serde(default)]
    drop: Vec<Feature>,
    #[serde(default)]
    mech_add: Vec<Mechanic>,
    #[serde(default)]
    mech_drop: Vec<Mechanic>,
    #[serde(default)]
    element: Option<Element>,
    #[serde(default)]
    min_scale: Option<f32>,
    #[serde(default)]
    max_scale: Option<f32>,
    #[serde(default)]
    min_mass: Option<f32>,
    #[serde(default)]
    max_mass: Option<f32>,
}

/// A franchise word that means "a transformer" ("dinobot", "predacon", "autobot").
#[derive(Deserialize)]
struct Franchise {
    words: Vec<String>,
    #[serde(default)]
    plural: Vec<String>,
    /// What it turns into when no alt mode is named; None = the bare word keeps its own reading.
    #[serde(default)]
    alt: Option<String>,
    #[serde(default)]
    temperament: Option<Temperament>,
}

#[derive(Deserialize)]
struct SizeWord {
    words: String,
    #[serde(default)]
    suit: Option<f32>,
    #[serde(default)]
    mul: Option<f32>,
}

#[derive(Deserialize)]
struct TfData {
    fold: Vec<(String, String)>,
    fuzzy: Vec<String>,
    guard_phrases: Vec<String>,
    robot_words: Vec<String>,
    robot_plural: Vec<String>,
    transformer_words: Vec<String>,
    transformer_plural: Vec<String>,
    transforming_words: Vec<String>,
    subject_words: Vec<String>,
    into_verbs: Vec<String>,
    bare_verbs: Vec<String>,
    blockers: Vec<String>,
    franchises: Vec<Franchise>,
    min_scale: f32,
    max_scale: f32,
    max_mass: f32,
    sizes: Vec<SizeWord>,
    drop_mechanics: Vec<Mechanic>,
    alt_forms: Vec<Form>,
    confidence: f32,
    plural_count: u8,
    max_count: u8,
    alts: Vec<AltRow>,
}

fn data() -> Option<&'static TfData> {
    static D: OnceLock<Option<TfData>> = OnceLock::new();
    D.get_or_init(|| ron::from_str::<TfData>(include_str!("../data/transformer.ron")).ok()).as_ref()
}

/// Length in words of `phrase` if it starts at `w[i]`.
fn at(w: &[String], i: usize, phrase: &str) -> Option<usize> {
    let mut n = 0;
    for p in phrase.split(' ') {
        if w.get(i + n)? != p {
            return None;
        }
        n += 1;
    }
    Some(n)
}

/// Longest phrase of `list` starting at `w[i]`.
fn longest(w: &[String], i: usize, list: &[String]) -> Option<usize> {
    list.iter().filter_map(|p| at(w, i, p)).max()
}

/// A word (span) that says "this is a transformer".
#[derive(Clone, Copy)]
struct Trigger<'d> {
    len: usize,
    /// Only "robot" / "bot": the alt mode must then be a vehicle.
    weak: bool,
    plural: bool,
    franchise: Option<&'d Franchise>,
}

fn trigger<'d>(d: &'d TfData, w: &[String], i: usize) -> Option<Trigger<'d>> {
    let best = d
        .franchises
        .iter()
        .flat_map(|f| [(longest(w, i, &f.words), false, f), (longest(w, i, &f.plural), true, f)])
        .filter_map(|(n, plural, f)| n.map(|n| (n, plural, f)))
        .max_by_key(|(n, _, _)| *n);
    if let Some((len, plural, f)) = best {
        return Some(Trigger { len, weak: false, plural, franchise: Some(f) });
    }
    let t = w.get(i)?;
    let hit = |list: &[String]| list.iter().any(|x| x == t);
    let (weak, plural) = if hit(&d.transformer_words) || hit(&d.transforming_words) {
        (false, false)
    } else if hit(&d.transformer_plural) {
        (false, true)
    } else if hit(&d.robot_words) {
        (true, false)
    } else if hit(&d.robot_plural) {
        (true, true)
    } else {
        return None;
    };
    Some(Trigger { len: 1, weak, plural, franchise: None })
}

/// `w` is exactly one trigger or subject word ("robot", "a transformer", "mech", "dinobot").
fn whole_subject<'d>(d: &'d TfData, w: &[String]) -> Option<Trigger<'d>> {
    if w.is_empty() {
        return None;
    }
    if let Some(t) = trigger(d, w, 0).filter(|t| t.len == w.len()) {
        return Some(t);
    }
    let one = w.join(" ");
    d.subject_words.contains(&one).then_some(Trigger { len: w.len(), weak: false, plural: one.ends_with('s'), franchise: None })
}

/// How the phrase asked for a transformer: which words (indices) are the alt mode.
struct Parse<'d> {
    alt: Vec<usize>,
    franchise: Option<&'d Franchise>,
    weak: bool,
    plural: bool,
    /// No alt mode named, only the transformer word and a size ("a transformer suit", "a small
    /// autobot"): the plain transformer, sized. `alt` then holds the transformer words.
    bare: bool,
    how: &'static str,
}

impl Lexicon {
    /// The transformer reading of a phrase, or None (the ordinary grammar reads it). See the module docs.
    pub(crate) fn transformer(&self, toks: &[Tok], bangs: usize, spoken: bool) -> Option<Resolution> {
        let d = data()?;
        let lead = toks
            .iter()
            .take_while(|t| matches!(gram_of(&t.text), Some(Gram::Article | Gram::Filler | Gram::Cast | Gram::Pronoun | Gram::MakeIt)))
            .count();
        let raw = &toks[lead.min(toks.len())..];
        if raw.is_empty() {
            return None;
        }
        let whole: String = raw.iter().map(|t| t.text.as_str()).collect::<Vec<_>>().join(" ");
        if self.tf_guarded(d, &whole) {
            return None;
        }
        let (w, caps, folded) = self.tf_fold(d, raw);
        let phrase = w.join(" ");
        if folded && self.tf_guarded(d, &phrase) {
            return Some(self.resolve_tokens_plain(to_toks(&w, &caps), bangs, spoken, 0));
        }
        let size = self.tf_sizes(d, &w);
        let res = self.tf_parse(d, &w, &size).and_then(|p| {
            // a lexicon name that already transforms ("convertible robot") keeps its own reading
            if p.franchise.is_none() && self.lookup(&phrase).is_some_and(|r| r.has(Mechanic::Transforms)) {
                return None;
            }
            self.tf_build(d, &w, &caps, &size, p, bangs, spoken)
        });
        match res {
            Some(r) => Some(r),
            // "trans former", "dino bot" said alone: the repaired words, read the ordinary way
            None if folded => Some(self.resolve_tokens_plain(to_toks(&w, &caps), bangs, spoken, 0)),
            None => None,
        }
    }

    /// Curated names, idioms and commands are never re-read.
    fn tf_guarded(&self, d: &TfData, phrase: &str) -> bool {
        self.icons.alias.contains_key(phrase)
            || d.guard_phrases.iter().any(|g| g == phrase)
            || crate::parse::action_of(phrase).is_some()
    }

    /// Mishearings / typos of the transformer words (`fold`, `fuzzy`). Returns (words, caps, whether a
    /// transformer word came out of a repair).
    fn tf_fold(&self, d: &TfData, raw: &[Tok]) -> (Vec<String>, Vec<bool>, bool) {
        let words: Vec<String> = raw.iter().map(|t| t.text.clone()).collect();
        let mut w = Vec::with_capacity(words.len());
        let mut caps = Vec::with_capacity(words.len());
        let mut folded_trigger = false;
        let mut i = 0;
        while i < words.len() {
            let hit = d.fold.iter().filter_map(|(from, to)| at(&words, i, from).map(|n| (n, to))).max_by_key(|(n, _)| *n);
            if let Some((n, to)) = hit {
                for t in to.split(' ') {
                    w.push(t.to_string());
                    caps.push(raw[i].caps);
                }
                let tw: Vec<String> = to.split(' ').map(String::from).collect();
                folded_trigger |= trigger(d, &tw, 0).is_some();
                i += n;
                continue;
            }
            let t = &words[i];
            let fixed = (t.len() >= 7 && !self.known_token(t))
                .then(|| {
                    let max = if t.len() >= 10 { 2 } else { 1 };
                    d.fuzzy.iter().find(|f| dl_distance(t.as_bytes(), f.as_bytes(), max).is_some())
                })
                .flatten();
            match fixed {
                Some(f) => {
                    w.push(f.clone());
                    folded_trigger = true;
                }
                None => w.push(t.clone()),
            }
            caps.push(raw[i].caps);
            i += 1;
        }
        (w, caps, folded_trigger)
    }

    /// Per word: the size it asks for (`sizes`), only before a "with …" tail ("a robot car with a
    /// giant sword": the sword is giant). A size word that starts a curated name or an alt-mode word
    /// ("big rig", "mini cooper") is not one.
    fn tf_sizes(&self, d: &TfData, w: &[String]) -> Vec<Option<(Option<f32>, Option<f32>)>> {
        let mut out = vec![None; w.len()];
        let end = w.iter().position(|x| matches!(gram_of(x), Some(Gram::With | Gram::MadeOf))).unwrap_or(w.len());
        let mut i = 0;
        while i < end {
            let hit = d.sizes.iter().filter_map(|s| at(&w[..end], i, &s.words).map(|n| (n, s))).max_by_key(|(n, _)| *n);
            let Some((n, s)) = hit else {
                i += 1;
                continue;
            };
            if n == 1 && i + 1 < w.len() {
                let two = format!("{} {}", w[i], w[i + 1]);
                let named = self.icons.alias.contains_key(&two) || d.alts.iter().any(|r| r.words.contains(&two));
                if named {
                    i += 1;
                    continue;
                }
            }
            for slot in out.iter_mut().skip(i).take(n) {
                *slot = Some((s.suit, s.mul));
            }
            i += n;
        }
        out
    }

    fn tf_parse<'d>(&self, d: &'d TfData, w: &[String], size: &[Option<(Option<f32>, Option<f32>)>]) -> Option<Parse<'d>> {
        let n = w.len();
        let content = |r: std::ops::Range<usize>| -> Vec<usize> { r.filter(|&k| size[k].is_none()).collect() };
        let words = |ix: &[usize]| -> Vec<String> { ix.iter().map(|&k| w[k].clone()).collect() };
        // 1. "<robot> that turns into X" / "X that transforms into a robot"
        for i in 0..n {
            let Some(vl) = longest(w, i, &d.into_verbs) else { continue };
            let mut l_end = i;
            while l_end > 0 && matches!(gram_of(&w[l_end - 1]), Some(Gram::Clause)) {
                l_end -= 1;
            }
            let mut r0 = i + vl;
            while r0 < n && matches!(gram_of(&w[r0]), Some(Gram::Article)) {
                r0 += 1;
            }
            let left = content(0..l_end);
            let right = content(r0..n);
            let lw = words(&left);
            // the subject ends the left side ("a big red robot that…"): its adjectives go to the alt mode
            let sub = (1..=lw.len().min(3)).rev().find_map(|k| whole_subject(d, &lw[lw.len() - k..]).map(|t| (lw.len() - k, t)));
            if let Some((s, t)) = sub {
                if right.is_empty() || whole_subject(d, &words(&right)).is_some() {
                    return None;
                }
                let mods = &left[..s];
                let mut alt: Vec<usize> = if mods.iter().all(|&k| gram_of(&w[k]).is_none()) { mods.to_vec() } else { Vec::new() };
                alt.extend(&right);
                return Some(Parse { alt, franchise: t.franchise, weak: false, plural: t.plural, bare: false, how: "turns into" });
            }
            if let Some(t) = whole_subject(d, &words(&right)) {
                if left.is_empty() || whole_subject(d, &lw).is_some() {
                    return None;
                }
                return Some(Parse { alt: left, franchise: t.franchise, weak: false, plural: false, bare: false, how: "transforms into a robot" });
            }
            // a clause that isn't about a robot ("a car that turns into a boat"): the ordinary grammar
            return None;
        }
        // 2. "X that transforms" / "X which can transform" (a link word before the verb, at the end)
        for i in 1..n {
            let Some(vl) = longest(w, i, &d.bare_verbs) else { continue };
            if i + vl != n {
                continue;
            }
            let mut l_end = i;
            while l_end > 0 && matches!(gram_of(&w[l_end - 1]), Some(Gram::Clause)) {
                l_end -= 1;
            }
            if l_end == i {
                break;
            }
            let left = content(0..l_end);
            if left.is_empty() || whole_subject(d, &words(&left)).is_some() {
                return None;
            }
            return Some(Parse { alt: left, franchise: None, weak: false, plural: false, bare: false, how: "that transforms" });
        }
        // 3. adjacency: "helicopter robot", "robot car", "jet bot", "transforming wolf", "t rex
        //    transformer", "dinobot", "an autobot car" (+ an optional "with …" tail for the alt mode)
        let head_end = w.iter().position(|x| matches!(gram_of(x), Some(Gram::With | Gram::MadeOf))).unwrap_or(n);
        if head_end == 0 || w[..head_end].iter().any(|x| gram_of(x).is_some() || d.blockers.iter().any(|b| b == x)) {
            return None;
        }
        let mut is_trig = vec![false; n];
        let (mut any, mut weak, mut plural, mut franchise) = (false, true, false, None);
        let mut i = 0;
        while i < head_end {
            match trigger(d, &w[..head_end], i) {
                Some(t) => {
                    any = true;
                    weak &= t.weak;
                    plural |= t.plural;
                    franchise = franchise.or(t.franchise);
                    for s in is_trig.iter_mut().skip(i).take(t.len) {
                        *s = true;
                    }
                    i += t.len;
                }
                None => i += 1,
            }
        }
        if !any {
            return None;
        }
        let mut alt: Vec<usize> = (0..head_end).filter(|&k| !is_trig[k] && size[k].is_none()).collect();
        alt.extend(head_end..n);
        let named_alt = franchise.is_some_and(|f| f.alt.is_some());
        if alt.is_empty() && !weak && !named_alt && size.iter().any(Option::is_some) {
            let trig: Vec<usize> = (0..head_end).filter(|&k| is_trig[k]).collect();
            return Some(Parse { alt: trig, franchise, weak, plural, bare: true, how: "a transformer, sized" });
        }
        Some(Parse { alt, franchise, weak, plural, bare: false, how: if weak { "robot + vehicle" } else { "transformer word" } })
    }

    /// The alt-mode row for the alt's head words (longest match of its last 1-3 words; a one-word match
    /// yields to a known two-word name of another form: "fish tank" is no tank).
    fn tf_row<'d>(&self, d: &'d TfData, head: &[String]) -> Option<(&'d AltRow, usize)> {
        for m in (1..=head.len().min(3)).rev() {
            let key = head[head.len() - m..].join(" ");
            if let Some(r) = d.alts.iter().find(|r| r.words.contains(&key)) {
                if m == 1
                    && head.len() >= 2
                    && let (Some(f), Some(named)) = (r.form, self.lookup(&head[head.len() - 2..].join(" ")))
                    && named.form != f
                    && named.form != Form::Blob0
                {
                    return None;
                }
                return Some((r, m));
            }
        }
        None
    }

    /// A plural head word's singular ("helicopters" -> "helicopter", "wolves" -> "wolf").
    fn tf_singular(&self, d: &TfData, t: &str) -> Option<String> {
        if t.len() <= 3 || !t.ends_with('s') || t.ends_with("ss") || d.alts.iter().any(|r| r.words.iter().any(|x| x == t)) {
            return None;
        }
        singulars(t).into_iter().find(|s| {
            d.alts.iter().any(|r| r.words.iter().any(|x| x == s)) || self.icons.alias.contains_key(s) || self.entry(s).is_some()
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn tf_build(
        &self,
        d: &TfData,
        w: &[String],
        caps: &[bool],
        size: &[Option<(Option<f32>, Option<f32>)>],
        p: Parse<'_>,
        bangs: usize,
        spoken: bool,
    ) -> Option<Resolution> {
        let mut alt: Vec<String> = p.alt.iter().map(|&k| w[k].clone()).collect();
        let mut alt_caps: Vec<bool> = p.alt.iter().map(|&k| caps[k]).collect();
        let mut how = p.how.to_string();
        if alt.is_empty() {
            // "a dinobot", "a predacon": the franchise's own alt mode
            let def = p.franchise.and_then(|f| f.alt.as_ref())?;
            alt = def.split(' ').map(String::from).collect();
            alt_caps = vec![false; alt.len()];
            how = format!("{how} (a {def})");
        }
        let head_end = alt.iter().position(|x| matches!(gram_of(x), Some(Gram::With | Gram::MadeOf))).unwrap_or(alt.len());
        if head_end == 0 {
            return None;
        }
        let mut plural = p.plural;
        if let Some(s) = self.tf_singular(d, &alt[head_end - 1]) {
            alt[head_end - 1] = s;
            plural = true;
        }
        let mut a = self.resolve_tokens_plain(to_toks(&alt, &alt_caps), bangs, spoken, 0);
        let row = self.tf_row(d, &alt[..head_end]);
        // "train" is also a superhero, "crane" also a bird: re-read the words as the vehicle they mean
        if let Some((r, m)) = row
            && let Some(f) = r.form
            && (a.recipe.form != f || Lexicon::is_fallback(&a))
        {
            let said = alt[head_end - m..head_end].join(" ");
            if let Some((_, to)) = r.alias.iter().find(|(from, _)| *from == said) {
                let mut re: Vec<String> = alt[..head_end - m].to_vec();
                re.extend(to.split(' ').map(String::from));
                re.extend(alt[head_end..].iter().cloned());
                let re_caps = vec![false; re.len()];
                a = self.resolve_tokens_plain(to_toks(&re, &re_caps), bangs, spoken, 0);
                a.explanation.insert(0, format!("{said} \u{2192} {to} (the vehicle)"));
            }
        }
        // a curated two-mode icon already is the transformer ("megatron robot")
        if a.recipe.has(Mechanic::Transforms) && matches!(a.recipe.model, ModelSource::Icon(_)) {
            return Some(a);
        }
        if a.kind != ResolutionKind::Construct || a.recipe.form == Form::Blob0 || a.confidence < 0.35 || (p.weak && !a.ignored.is_empty()) {
            return None;
        }
        let row = row.map(|(r, _)| r).filter(|_| !p.bare);
        let form = row.and_then(|r| r.form).unwrap_or(a.recipe.form);
        if !p.bare && (!d.alt_forms.contains(&form) || (p.weak && form != Form::Vehicle)) {
            return None;
        }
        let alt_name = a.recipe.name.clone();
        let s0 = a.recipe.scale.max(0.05);
        let m0 = a.recipe.mass;
        let rc = &mut a.recipe;
        rc.form = form;
        let mut mechs: Vec<Mechanic> = vec![Mechanic::Transforms];
        if let Some(r) = row {
            for f in &r.add {
                if !rc.features.contains(f) {
                    rc.features.push(*f);
                }
            }
            if let Some(first) = r.any.first()
                && !r.any.iter().any(|f| rc.features.contains(f))
            {
                rc.features.push(*first);
            }
            rc.features.retain(|f| !r.drop.contains(f));
            mechs.extend(&r.mech_add);
            rc.mechanics.retain(|m| !r.mech_drop.contains(m));
            if let Some(e) = r.element
                && rc.element == Element::Light
            {
                rc.element = e;
            }
        }
        rc.mechanics.extend(mechs);
        rc.mechanics.retain(|m| !d.drop_mechanics.contains(m) && !(plural && *m == Mechanic::Piloted));
        // size: the alt mode's typical size, clamped to a transformer's; suit words / big words on top
        let lo = row.and_then(|r| r.min_scale).unwrap_or(d.min_scale);
        let hi = row.and_then(|r| r.max_scale).unwrap_or(d.max_scale).max(lo);
        let base = s0.clamp(lo, hi);
        let suit = size.iter().flatten().filter_map(|(s, _)| *s).reduce(f32::min);
        let mul: f32 = size.iter().flatten().filter_map(|(_, m)| *m).product();
        rc.scale = match suit {
            Some(s) => s,
            None => base * mul,
        };
        rc.mass = m0 * (rc.scale / s0).powi(3);
        if suit.is_none() {
            if let Some(m) = row.and_then(|r| r.min_mass) {
                rc.mass = rc.mass.max(m * mul.powi(3));
            }
            let cap = row.and_then(|r| r.max_mass).unwrap_or(d.max_mass).min(d.max_mass);
            rc.mass = rc.mass.min(cap * mul.powi(3));
        }
        // ONE machine you drive — a squad only when the words were plural ("dinobots", "car robots")
        rc.count = if plural { if rc.count > 1 { rc.count.min(d.max_count) } else { d.plural_count } } else { 1 };
        if let Some(t) = p.franchise.and_then(|f| f.temperament) {
            rc.temperament = t;
            rc.intensity = rc.intensity.max(0.6);
        }
        rc.name = w.join(" ");
        a.recipe = a.recipe.clone().normalized();
        if !a.graft.mechanics.contains(&Mechanic::Transforms) {
            a.graft.mechanics.push(Mechanic::Transforms);
            a.graft.mechanics.sort();
        }
        a.kind = ResolutionKind::Construct;
        a.confidence = a.confidence.max(d.confidence);
        let sized = match suit {
            Some(s) => format!(", suit size {s:.1} m"),
            None if (mul - 1.0).abs() > 1e-3 => format!(", \u{00d7}{mul:.1}"),
            None => String::new(),
        };
        a.explanation.insert(
            0,
            format!("{} \u{2192} a transformer ({how}): alt mode {alt_name}, {:.1} m{sized}{}", w.join(" "), a.recipe.scale, if plural { ", a squad" } else { "" }),
        );
        Some(a)
    }
}

fn to_toks(w: &[String], caps: &[bool]) -> Vec<Tok> {
    w.iter().enumerate().map(|(i, t)| Tok { text: t.clone(), caps: caps.get(i).copied().unwrap_or(false) }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_parses() {
        let d = data().expect("transformer.ron parses");
        assert!(d.min_scale >= 4.0 && d.max_scale >= d.min_scale);
        // every suit size is a suit you wear (bd_sim SUIT_MAX_SCALE = 2.6)
        assert!(d.sizes.iter().filter_map(|s| s.suit).all(|s| s <= 2.6));
        // a word belongs to one alt-mode row
        let mut seen = std::collections::HashSet::new();
        for r in &d.alts {
            for w in &r.words {
                assert!(seen.insert(w.clone()), "alt word {w:?} listed twice");
            }
        }
    }
}
