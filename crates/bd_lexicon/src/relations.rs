//! Relations layer: natural phrasings that relate TWO things ("duck the size of a house", "king kong on
//! a skateboard", "satellite death beam", "meteor shower") or scale one ("nuclear", "planetary").
//!
//! Runs once per phrase before the ordinary grammar (hook in `parse.rs::resolve_tokens`). Every relation
//! resolves its sub-phrases with the ordinary grammar (relations off), then combines them:
//! * size: `Y` keeps its own reading; `X` only lends its typical scale (none of X's mechanics);
//! * mount: `X` stays the construct, `Y` becomes `Recipe::mount` (bd_core v1.6) scaled to X, and X takes
//!   Y's locomotion (Fast/Flies/...) instead of stomping;
//! * emitter/sky: `X` is the construct, the attack noun becomes its mechanics; sky words make it hover
//!   high above the aim point and fire down (`Flies + DropFromSky + Beam/Shoots`);
//! * volley: many `X`, falling from the sky for rain/shower/hail.
//!
//! Anything unknown falls back to today's behaviour (the ring never says no). All patterns and word
//! tables live in `data/relations.ron`.

use std::sync::OnceLock;

use bd_core::mechanics::{Element, Feature, Form, FormCategory, Mechanic, Recipe, Temperament};
use serde::Deserialize;

use crate::norm::Tok;
use crate::parse::{Gram, gram_of};
use crate::{Lexicon, Resolution, ResolutionKind};

#[derive(Deserialize)]
struct SizeOf {
    words: String,
    mul: f32,
}

#[derive(Deserialize)]
struct Magnitude {
    words: String,
    #[serde(default = "one")]
    scale: f32,
    #[serde(default)]
    abs: Option<f32>,
    #[serde(default)]
    mech: Vec<Mechanic>,
    #[serde(default)]
    element: Option<Element>,
}

#[derive(Deserialize)]
struct ModPhrase {
    words: String,
    mech: Vec<Mechanic>,
}

#[derive(Deserialize)]
struct Attack {
    words: String,
    mech: Vec<Mechanic>,
    #[serde(default)]
    element: Option<Element>,
    #[serde(default)]
    host: bool,
    #[serde(default)]
    count: Option<u8>,
    #[serde(default)]
    strike: bool,
}

/// A named move ("charge", "breath", "smash"): see `moves` in `data/relations.ron`.
#[derive(Deserialize)]
struct MoveWords {
    words: Vec<String>,
    #[serde(default)]
    mech: Vec<Mechanic>,
    #[serde(default)]
    temperament: Option<Temperament>,
}

#[derive(Deserialize)]
struct Volley {
    word: String,
    count: u8,
    drop: bool,
}

fn one() -> f32 {
    1.0
}

#[derive(Deserialize)]
struct RelData {
    guard_phrases: Vec<String>,
    size_markers: Vec<String>,
    size_of: Vec<SizeOf>,
    magnitude: Vec<Magnitude>,
    scale_mul_min: f32,
    scale_mul_max: f32,
    modifiers: Vec<ModPhrase>,
    ride_words: Vec<String>,
    vehicle_words: Vec<String>,
    gerunds: Vec<(String, String)>,
    board_ratio: f32,
    vehicle_ratio: f32,
    creature_ratio: f32,
    creature_min_scale: f32,
    creature_min_rel: f32,
    mount_locomotion: Vec<Mechanic>,
    mount_drops: Vec<Mechanic>,
    attacks: Vec<Attack>,
    sky_words: Vec<String>,
    sky_emitters: Vec<String>,
    default_sky_emitter: String,
    strike_count: u8,
    #[serde(default)]
    moves: Vec<MoveWords>,
    #[serde(default)]
    move_links: Vec<String>,
    volleys: Vec<Volley>,
    leftover_min_conf: f32,
    leftover_max_mechs: usize,
    leftover_suffixes: Vec<(String, String)>,
    leftovers: Vec<Leftover>,
}

#[derive(Deserialize)]
struct Leftover {
    words: Vec<String>,
    #[serde(default)]
    mech: Vec<Mechanic>,
    #[serde(default)]
    element: Option<Element>,
    #[serde(default)]
    temperament: Option<Temperament>,
}

fn data() -> Option<&'static RelData> {
    static D: OnceLock<Option<RelData>> = OnceLock::new();
    D.get_or_init(|| ron::from_str::<RelData>(include_str!("../data/relations.ron")).ok()).as_ref()
}

/// Effects of stripped magnitude / modifier words, applied to the final recipe.
#[derive(Default)]
struct Effects {
    scale_mul: f32,
    abs: Option<f32>,
    mech: Vec<Mechanic>,
    element: Option<Element>,
    notes: Vec<String>,
}

fn words(toks: &[Tok]) -> Vec<&str> {
    toks.iter().map(|t| t.text.as_str()).collect()
}

fn join(toks: &[Tok]) -> String {
    words(toks).join(" ")
}

/// Position of the phrase `p` (space-separated) in `w` starting at or after `from`.
fn find(w: &[&str], p: &str, from: usize) -> Option<usize> {
    let pw: Vec<&str> = p.split(' ').collect();
    if pw.is_empty() || pw.len() > w.len() {
        return None;
    }
    (from..=w.len() - pw.len()).find(|&i| w[i..i + pw.len()] == pw[..])
}

/// Drop leading articles / fillers ("a", "the", "his", "top of"...).
fn trim_lead(t: &[Tok]) -> &[Tok] {
    let mut i = 0;
    while i < t.len() && matches!(gram_of(&t[i].text), Some(Gram::Article | Gram::Filler)) {
        i += 1;
    }
    &t[i..]
}

fn trim_trail(t: &[Tok]) -> &[Tok] {
    let mut n = t.len();
    while n > 0 && matches!(gram_of(&t[n - 1].text), Some(Gram::Article | Gram::Filler)) {
        n -= 1;
    }
    &t[..n]
}

impl Lexicon {
    /// Ordinary resolution of a sub-phrase (relations off).
    fn sub(&self, toks: &[Tok], bangs: usize, spoken: bool) -> Resolution {
        self.resolve_tokens_inner(trim_trail(trim_lead(toks)).to_vec(), bangs, spoken, 0, false)
    }

    /// Is this a real, physical thing (not an unknown blob, a spirit or a bare element)?
    fn physical(&self, text: &str, r: &Resolution) -> bool {
        if r.kind != ResolutionKind::Construct || r.recipe.form == Form::Blob0 || r.confidence < 0.35 {
            return false;
        }
        match self.entry(text) {
            Some((_, rec)) => rec.flags & crate::data::F_SPIRIT == 0,
            None => true,
        }
    }

    fn elemental(&self, text: &str) -> bool {
        self.t.pure.get(text).map(|(e, _)| *e != crate::tables::element_index(Element::Light)).unwrap_or(false)
    }

    /// Exact names and idioms never go through relations.
    fn relation_guarded(&self, d: &RelData, whole: &str) -> bool {
        d.guard_phrases.iter().any(|g| g == whole)
            || self.icons.alias.contains_key(whole)
            || self
                .entry(whole)
                .map(|(_, r)| r.flags & (crate::data::F_ANCHOR | crate::data::F_ENTITY | crate::data::F_POKEMON) != 0)
                .unwrap_or(false)
    }

    /// Guarded only as a Wikidata-style entity title (no idiom, curated alias, hand anchor or Pokémon).
    fn entity_only(&self, d: &RelData, whole: &str) -> bool {
        use crate::data::{F_ANCHOR, F_ENTITY, F_POKEMON};
        !d.guard_phrases.iter().any(|g| g == whole)
            && !self.icons.alias.contains_key(whole)
            && self.entry(whole).is_some_and(|(_, r)| r.flags & F_ENTITY != 0 && r.flags & (F_ANCHOR | F_POKEMON) == 0)
    }

    /// Strip magnitude / modifier phrases (unless they start a name: "mega man", "atomic bomb").
    fn strip_effects(&self, d: &RelData, toks: &[Tok]) -> (Vec<Tok>, Effects) {
        let mut fx = Effects { scale_mul: 1.0, ..Default::default() };
        let mut out = Vec::with_capacity(toks.len());
        let w = words(toks);
        let mut i = 0;
        'tok: while i < toks.len() {
            for m in &d.magnitude {
                let n = m.words.split(' ').count();
                if i + n <= w.len() && w[i..i + n].join(" ") == m.words && !self.starts_name(&w, i, n) {
                    fx.scale_mul *= m.scale;
                    if let Some(a) = m.abs {
                        fx.abs = Some(fx.abs.map_or(a, |b: f32| b.max(a)));
                    }
                    fx.mech.extend(m.mech.iter().copied());
                    if m.element.is_some() {
                        fx.element = m.element;
                    }
                    fx.notes.push(format!("{} \u{2192} {}", m.words, magnitude_note(m)));
                    i += n;
                    continue 'tok;
                }
            }
            for m in &d.modifiers {
                let n = m.words.split(' ').count();
                if i + n <= w.len() && w[i..i + n].join(" ") == m.words {
                    fx.mech.extend(m.mech.iter().copied());
                    fx.notes.push(format!("{} \u{2192} {:?}", m.words, m.mech));
                    i += n;
                    continue 'tok;
                }
            }
            out.push(toks[i].clone());
            i += 1;
        }
        fx.scale_mul = fx.scale_mul.clamp(d.scale_mul_min, d.scale_mul_max);
        (out, fx)
    }

    /// Does w[i..i+n] followed by more words form an icon alias or a hand-anchored name?
    fn starts_name(&self, w: &[&str], i: usize, n: usize) -> bool {
        (i + n + 1..=w.len().min(i + n + 4)).any(|end| {
            let s = w[i..end].join(" ");
            self.icons.alias.contains_key(&s) || self.entry(&s).map(|(_, r)| r.flags & crate::data::F_ANCHOR != 0).unwrap_or(false)
        }) || (i > 0 && {
            let s = w[i - 1..i + n].join(" ");
            self.icons.alias.contains_key(&s)
        })
    }

    /// The relations entry point. `None` = no relation: resolve as usual.
    pub(crate) fn relations(&self, toks: &[Tok], bangs: usize, spoken: bool) -> Option<Resolution> {
        let d = data()?;
        if toks.len() < 2 {
            return None;
        }
        let whole = join(toks);
        if self.relation_guarded(d, &whole) {
            // "dragon breath", "lion roar": a loose encyclopedia title (a shotgun round, an instrument)
            // doesn't beat a creature doing its move; a curated / hand-anchored name always does
            return self.entity_only(d, &whole).then(|| self.move_rel(d, toks, bangs, spoken)).flatten().filter(|r| r.recipe.form.category() == FormCategory::Creature);
        }
        let (rest, fx) = self.strip_effects(d, toks);
        let has_fx = !fx.notes.is_empty();
        if rest.is_empty() {
            return None; // only magnitude words: leave to the ordinary grammar
        }
        let rel = if rest.len() >= 2 && !(has_fx && self.relation_guarded(d, &join(&rest))) {
            self.mount_rel(d, &rest, bangs, spoken)
                .or_else(|| self.size_rel(d, &rest, bangs, spoken))
                .or_else(|| self.volley_rel(d, &rest, bangs, spoken))
                .or_else(|| self.move_rel(d, &rest, bangs, spoken))
                .or_else(|| self.emitter_rel(d, &rest, bangs, spoken))
        } else {
            None
        };
        let mut res = match rel {
            Some(r) => r,
            None if has_fx => self.sub(&rest, bangs, spoken),
            None => return None,
        };
        if has_fx {
            apply_effects(&mut res, fx);
        }
        Some(res)
    }

    // ------------------------------------------------------------------------------ mounts
    fn rideable(&self, d: &RelData, m: &Recipe, rider_scale: f32, vehicle_only: bool) -> Option<MountKind> {
        let has_f = |f: Feature| m.features.contains(&f);
        let vehicleish =
            matches!(m.form, Form::Vehicle | Form::Rocket) || has_f(Feature::Cockpit) || has_f(Feature::Cab) || has_f(Feature::Treads);
        if vehicleish {
            let board = m.form == Form::Vehicle && m.scale < 1.5 && !has_f(Feature::Cockpit) && !has_f(Feature::Cab);
            return Some(if board { MountKind::Board } else { MountKind::Vehicle });
        }
        if vehicle_only {
            return None;
        }
        let boardish = (m.form == Form::Platform && (m.has(Mechanic::Piloted) || m.has(Mechanic::Fast) || m.has(Mechanic::Flies)))
            || (m.has(Mechanic::Piloted) && m.form.category() != FormCategory::Creature)
            || has_f(Feature::Wheels)
            || has_f(Feature::Sails);
        if boardish {
            return Some(MountKind::Board);
        }
        let creature_mount =
            matches!(m.form, Form::Quadruped | Form::Bird | Form::Fish | Form::Serpent | Form::Kaiju | Form::Insect | Form::Arachnid);
        if creature_mount && m.scale >= d.creature_min_scale.max(rider_scale * d.creature_min_rel) {
            return Some(MountKind::Creature);
        }
        if creature_mount && m.has(Mechanic::Piloted) {
            return Some(MountKind::Creature);
        }
        None
    }

    fn mount_rel(&self, d: &RelData, toks: &[Tok], bangs: usize, spoken: bool) -> Option<Resolution> {
        let w = words(toks);
        // "skateboarding gorilla": a gerund anywhere before the rider
        // (two-word gerunds first: "jet skiing", which repair may have split from "jetskiing")
        for (i, n) in (0..w.len()).flat_map(|i| [(i, 2usize), (i, 1)]) {
            if i + n > w.len() {
                continue;
            }
            let t = w[i..i + n].join(" ");
            if let Some((_, noun)) = d.gerunds.iter().find(|(g, _)| *g == t) {
                let rider: Vec<Tok> = toks.iter().enumerate().filter(|(j, _)| *j < i || *j >= i + n).map(|(_, t)| t.clone()).collect();
                if rider.is_empty() {
                    return None;
                }
                let mount_toks: Vec<Tok> = noun.split(' ').map(|w| Tok { text: w.to_string(), caps: false }).collect();
                return self.make_mount(d, &rider, &mount_toks, false, bangs, spoken, "on");
            }
        }
        // "X riding Y" / "Y riding X" / "X on a Y"
        for rw in &d.ride_words {
            if let Some(i) = find(&w, rw, 1) {
                let n = rw.split(' ').count();
                let (a, b) = (&toks[..i], trim_lead(&toks[i + n..]));
                if a.is_empty() || b.is_empty() {
                    continue;
                }
                if let Some(r) = self.make_mount(d, a, b, false, bangs, spoken, rw) {
                    return Some(r);
                }
                // hyphenated "skateboard-riding gorilla": the rideable side comes first
                if rw.starts_with("rid")
                    && self.sub(a, bangs, spoken).recipe.form.category() != FormCategory::Creature
                    && let Some(r) = self.make_mount(d, b, a, false, bangs, spoken, rw)
                {
                    return Some(r);
                }
            }
        }
        // "X in a Y" / "X driving Y" / "X with a Y": a vehicle you sit in
        for vw in &d.vehicle_words {
            if let Some(i) = find(&w, vw, 1) {
                let (a, b) = (&toks[..i], trim_lead(&toks[i + 1..]));
                if a.is_empty() || b.is_empty() {
                    continue;
                }
                // "with" only for a creature rider and a real vehicle ("gorilla with a car")
                if let Some(r) = self.make_mount(d, a, b, true, bangs, spoken, vw)
                    && (vw != "with" || r.recipe.form.category() == FormCategory::Creature)
                {
                    return Some(r);
                }
            }
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    fn make_mount(
        &self,
        d: &RelData,
        rider_toks: &[Tok],
        mount_toks: &[Tok],
        vehicle_only: bool,
        bangs: usize,
        spoken: bool,
        how: &str,
    ) -> Option<Resolution> {
        let mount_text = join(trim_trail(trim_lead(mount_toks)));
        let rider_text = join(trim_trail(trim_lead(rider_toks)));
        if self.elemental(&mount_text) {
            return None;
        }
        let m = self.sub(mount_toks, bangs, spoken);
        if !self.physical(&mount_text, &m) {
            return None;
        }
        let mut r = self.sub(rider_toks, bangs, spoken);
        if !self.physical(&rider_text, &r) || r.recipe.form.category() == FormCategory::Structure {
            return None;
        }
        let kind = self.rideable(d, &m.recipe, r.recipe.scale, vehicle_only)?;
        let mut mount = m.recipe.clone();
        let old = mount.scale;
        mount.scale = match kind {
            MountKind::Board => r.recipe.scale * d.board_ratio,
            MountKind::Vehicle => mount.scale.max(r.recipe.scale * d.vehicle_ratio),
            MountKind::Creature => mount.scale.max(r.recipe.scale * d.creature_ratio),
        };
        let k = (mount.scale / old.max(0.01)).clamp(0.01, 100.0);
        mount.mass = (mount.mass * k * k * k).clamp(0.01, 1e9);
        mount.mount = None;
        mount.payload = None;
        mount.count = 1;
        let rc = &mut r.recipe;
        rc.mechanics.retain(|x| !d.mount_drops.contains(x));
        r.graft.mechanics.retain(|x| !d.mount_drops.contains(x));
        for x in &d.mount_locomotion {
            if mount.has(*x) {
                rc.mechanics.push(*x);
            }
        }
        let note = format!("{} {how} {} \u{2192} mount ({:?}, {:.1} m)", rider_text, mount_text, kind, mount.scale);
        rc.mount = Some(Box::new(mount.normalized()));
        let prep = if matches!(kind, MountKind::Vehicle) && d.vehicle_words.iter().any(|v| v == how) { "in" } else { "on" };
        rc.name = format!("{} {prep} {}", rc.name, m.recipe.name);
        r.recipe = r.recipe.clone().normalized();
        r.explanation.insert(0, note);
        r.confidence = r.confidence.min(m.confidence).max(0.5);
        r.kind = ResolutionKind::Construct;
        Some(r)
    }

    // ------------------------------------------------------------------------------ size
    fn size_rel(&self, d: &RelData, toks: &[Tok], bangs: usize, spoken: bool) -> Option<Resolution> {
        let w = words(toks);
        // "Y the size of X", "Y bigger than X"
        for so in &d.size_of {
            if let Some(i) = find(&w, &so.words, 1) {
                let n = so.words.split(' ').count();
                let (y, x) = (&toks[..i], &toks[i + n..]);
                if !x.is_empty()
                    && let Some(r) = self.sized(y, x, so.mul, &so.words, bangs, spoken)
                {
                    return Some(r);
                }
            }
        }
        // "X sized Y" / "X-size Y"
        for mk in &d.size_markers {
            if let Some(i) = find(&w, mk, 1) {
                let (x, y) = (&toks[..i], &toks[i + 1..]);
                if y.is_empty() {
                    continue;
                }
                // "king sized bed", "planet sized", "life size": an ordinary size adjective
                if self.t.adj.contains_key(&w[..=i].join(" ")) {
                    continue;
                }
                if let Some(r) = self.sized(y, x, 1.0, mk, bangs, spoken) {
                    return Some(r);
                }
            }
        }
        None
    }

    fn sized(&self, y: &[Tok], x: &[Tok], mul: f32, how: &str, bangs: usize, spoken: bool) -> Option<Resolution> {
        let x_text = join(trim_trail(trim_lead(x)));
        let reference = self.sub(x, bangs, spoken);
        if !self.physical(&x_text, &reference) {
            return None;
        }
        let mut r = self.sub(y, bangs, spoken);
        if r.recipe.form == Form::Blob0 {
            return None;
        }
        let old = r.recipe.scale.max(0.01);
        let target = (reference.recipe.scale * mul).clamp(0.02, 60.0);
        let k = target / old;
        r.recipe.scale = target;
        r.recipe.mass = (r.recipe.mass * k * k * k).clamp(0.01, 1e10);
        r.graft.scale_mul = (r.graft.scale_mul * k).clamp(0.05, 60.0);
        r.explanation.insert(
            0,
            if how.contains(' ') {
                format!("{how} {x_text} \u{2192} {target:.1} m ({} only lends its size)", reference.recipe.name)
            } else {
                format!("{x_text}-{how} \u{2192} {target:.1} m ({} only lends its size)", reference.recipe.name)
            },
        );
        Some(r)
    }

    // ------------------------------------------------------------------------------ volleys
    fn volley_rel(&self, d: &RelData, toks: &[Tok], bangs: usize, spoken: bool) -> Option<Resolution> {
        let w = words(toks);
        for v in &d.volleys {
            // "rain of arrows" (explicit: any physical X)
            if let Some(i) = w.iter().position(|t| *t == v.word) {
                if w.get(i + 1) == Some(&"of") && i + 2 < w.len() {
                    let mut x: Vec<Tok> = toks[..i].to_vec();
                    x.extend_from_slice(&toks[i + 2..]);
                    if let Some(r) = self.volley(v, &x, true, bangs, spoken) {
                        return Some(r);
                    }
                }
                // "meteor shower" (X V, last word): X must be a projectile-like thing
                if i + 1 == w.len()
                    && i > 0
                    && let Some(r) = self.volley(v, &toks[..i], false, bangs, spoken)
                {
                    return Some(r);
                }
            }
        }
        None
    }

    fn volley(&self, v: &Volley, x: &[Tok], explicit: bool, bangs: usize, spoken: bool) -> Option<Resolution> {
        let x_text = join(trim_trail(trim_lead(x)));
        if self.elemental(&x_text) {
            // "fire storm": the element grammar handles those; "rain of fire" is fire falling from the sky
            if !(explicit && v.drop) {
                return None;
            }
            let mut r = self.sub(x, bangs, spoken);
            if let Some((e, _)) = self.t.pure.get(&x_text)
                && let Some(el) = crate::tables::ALL_ELEMENT.get(*e as usize).copied()
            {
                r.recipe.element = el;
                r.graft.element = Some(el);
            }
            r.recipe.count = r.recipe.count.max(v.count);
            r.recipe.mechanics.extend([Mechanic::Swarm, Mechanic::DropFromSky]);
            r.recipe = r.recipe.clone().normalized();
            r.kind = ResolutionKind::Construct;
            r.explanation.insert(0, format!("{} of {x_text} \u{2192} {} falling from the sky", v.word, v.count));
            return Some(r);
        }
        let mut r = self.sub(x, bangs, spoken);
        if !self.physical(&x_text, &r) {
            return None;
        }
        let f = r.recipe.form;
        let projectile = f.category() == FormCategory::Object
            || matches!(f, Form::Spear | Form::Disc | Form::Sword | Form::Axe | Form::Hammer | Form::Insect | Form::Bird);
        if !explicit && !projectile {
            return None; // "baby shower" is a party, not falling babies
        }
        let rc = &mut r.recipe;
        rc.count = rc.count.max(v.count);
        rc.mechanics.push(Mechanic::Swarm);
        if v.drop {
            rc.mechanics.retain(|m| *m != Mechanic::Shoots);
            rc.mechanics.push(Mechanic::DropFromSky);
        } else {
            rc.mechanics.push(Mechanic::Fast);
        }
        r.recipe = r.recipe.clone().normalized();
        r.explanation.insert(
            0,
            format!("{} of {x_text} \u{2192} {} {}", v.word, v.count, if v.drop { "falling from the sky" } else { "fired at once" }),
        );
        Some(r)
    }

    // ------------------------------------------------------------------------------ named moves
    /// "<name> <move>" said on its own (an incantation like "hulk smash" with nothing after it): the
    /// named thing doing its move, or `None`. Used by the incantation stage (parse.rs).
    pub(crate) fn name_move(&self, toks: &[Tok], bangs: usize, spoken: bool) -> Option<Resolution> {
        let d = data()?;
        if toks.len() < 2 || self.relation_guarded(d, &join(toks)) {
            return None;
        }
        self.move_rel(d, toks, bangs, spoken)
    }

    /// "<creature / character / icon> [’s] <move>" → the named thing with the move's mechanics
    /// emphasised ("reinhardt charge", "godzilla's breath", "t rex stomp"). The name wins over the move
    /// word's own dictionary sense; a non-creature icon only takes a move it already has.
    fn move_rel(&self, d: &RelData, toks: &[Tok], bangs: usize, spoken: bool) -> Option<Resolution> {
        let w = words(toks);
        // the longest move phrase at the end of the phrase
        let mut best: Option<(&MoveWords, &str, usize)> = None;
        for m in &d.moves {
            for p in &m.words {
                let n = p.split(' ').count();
                if n < w.len() && w[w.len() - n..].join(" ") == *p && best.is_none_or(|b| b.2 < n) {
                    best = Some((m, p.as_str(), n));
                }
            }
        }
        let (mv, said, n) = best?;
        let mut end = toks.len() - n;
        while end > 1 && d.move_links.iter().any(|l| *l == toks[end - 1].text) {
            end -= 1;
        }
        let x = trim_trail(trim_lead(&toks[..end]));
        if x.is_empty() {
            return None;
        }
        let x_text = join(x);
        // "fire breath", "ice punch": an element + move is the element grammar's
        if self.elemental(&x_text) || gram_of(&x_text).is_some() {
            return None;
        }
        let mut r = self.sub(x, bangs, spoken);
        if !self.physical(&x_text, &r) || !r.ignored.is_empty() || r.recipe.mount.is_some() {
            return None;
        }
        let creature = r.recipe.form.category() == FormCategory::Creature;
        let owns = !mv.mech.is_empty() && mv.mech.iter().all(|m| r.recipe.has(*m));
        if !(creature || (r.shape.is_some() && owns)) {
            return None; // "phone charge", "grand slam": not a named thing doing a move
        }
        let rc = &mut r.recipe;
        let mut added: Vec<Mechanic> = Vec::new();
        for m in &mv.mech {
            if !rc.mechanics.contains(m) {
                rc.mechanics.push(*m);
                added.push(*m);
            }
            if !r.graft.mechanics.contains(m) {
                r.graft.mechanics.push(*m);
            }
        }
        r.graft.mechanics.sort();
        if let Some(t) = mv.temperament {
            rc.temperament = t;
            rc.intensity = rc.intensity.max(0.8);
            r.graft.temperament = Some(t);
        }
        // OPENING MOVE: the named move is performed once right after it forms (bd_sim creature AI).
        rc.opening = mv.mech.first().copied();
        let name = rc.name.clone();
        r.recipe = r.recipe.clone().normalized();
        r.kind = ResolutionKind::Construct;
        r.confidence = r.confidence.max(0.6);
        let what = match (mv.mech.is_empty(), added.is_empty()) {
            (true, _) => format!("{:?}", mv.temperament.unwrap_or_default()),
            (false, true) => format!("its own {:?}", mv.mech),
            (false, false) => format!("{:?}", mv.mech),
        };
        r.explanation.insert(0, format!("{x_text} {said} \u{2192} {name} doing its move: {what} emphasised (the name wins over \"{said}\")"));
        Some(r)
    }

    // ------------------------------------------------------------------------------ emitters
    fn emitter_rel(&self, d: &RelData, toks: &[Tok], bangs: usize, spoken: bool) -> Option<Resolution> {
        let w = words(toks);
        // longest attack phrase at the end of the phrase
        let mut best: Option<(&Attack, usize)> = None;
        for a in &d.attacks {
            let n = a.words.split(' ').count();
            if n < w.len() && w[w.len() - n..].join(" ") == a.words && best.is_none_or(|(b, _)| b.words.split(' ').count() < n) {
                best = Some((a, n));
            }
        }
        let (atk, n) = best?;
        let before = &toks[..toks.len() - n];
        // sky adjectives come off X; a sky emitter noun stays X and still means "in the sky"
        let bw = words(before);
        let mut sky = false;
        let mut x: Vec<Tok> = Vec::new();
        let mut i = 0;
        'b: while i < before.len() {
            for s in &d.sky_words {
                let k = s.split(' ').count();
                if i + k <= bw.len() && bw[i..i + k].join(" ") == *s {
                    sky = true;
                    i += k;
                    continue 'b;
                }
            }
            x.push(before[i].clone());
            i += 1;
        }
        let x_text = join(trim_trail(trim_lead(&x)));
        if d.sky_emitters.contains(&x_text) {
            sky = true;
        }
        if x_text.is_empty() {
            if !sky {
                return None;
            }
            x = d.default_sky_emitter.split(' ').map(|w| Tok { text: w.to_string(), caps: false }).collect();
        }
        let x_text = join(trim_trail(trim_lead(&x)));
        if self.elemental(&x_text) {
            return None; // "fire breath", "ice beam": the element grammar handles those
        }
        let mut r = self.sub(&x, bangs, spoken);
        if !self.physical(&x_text, &r) || (r.recipe.form.category() == FormCategory::Held && !sky) {
            return None; // "death ray", "sword beam", "laser gun": keep today's handheld readings
        }
        let creature = r.recipe.form.category() == FormCategory::Creature;
        if atk.host && creature && !sky {
            return None; // "shark cannon": a cannon that shoots sharks (payload grammar)
        }
        let projectile = r.recipe.form.category() == FormCategory::Object
            && !d.sky_emitters.contains(&x_text)
            && !matches!(r.recipe.form, Form::Vehicle);
        let rc = &mut r.recipe;
        let what;
        if atk.strike && projectile {
            // "missile strike": a volley of projectiles dropping on the target
            rc.mechanics.extend([Mechanic::DropFromSky, Mechanic::Explodes, Mechanic::Homing, Mechanic::Swarm]);
            rc.mechanics.retain(|m| *m != Mechanic::Shoots && *m != Mechanic::Thrust);
            rc.count = rc.count.max(d.strike_count);
            what = format!("{} \u{2192} {} {x_text} dropping on the target", atk.words, d.strike_count);
        } else if sky && projectile {
            rc.mechanics.push(Mechanic::DropFromSky);
            rc.mechanics.extend(atk.mech.iter().copied());
            what = format!("{} {x_text} \u{2192} dropped from the sky", atk.words);
        } else {
            rc.mechanics.extend(atk.mech.iter().copied());
            if atk.strike {
                rc.mechanics.push(Mechanic::Shoots);
            }
            if let Some(c) = atk.count {
                rc.count = rc.count.max(c);
            }
            if sky {
                // an emitter hovering high above the aim point, firing down (bd_sim places it)
                rc.mechanics.extend([Mechanic::Flies, Mechanic::DropFromSky]);
                rc.mechanics.retain(|m| *m != Mechanic::Returning); // a thrown disc's return, not an emitter's
                r.graft.mechanics.retain(|m| *m != Mechanic::Returning);
                what = format!("{x_text} {} \u{2192} hovers high above the target and fires down", atk.words);
            } else {
                what = format!("{x_text} {} \u{2192} {x_text} fires it", atk.words);
            }
        }
        if let Some(e) = atk.element {
            rc.element = e;
        }
        r.recipe = r.recipe.clone().normalized();
        r.kind = ResolutionKind::Construct;
        r.explanation.insert(0, what);
        Some(r)
    }
}

#[derive(Clone, Copy, Debug)]
enum MountKind {
    Board,
    Vehicle,
    Creature,
}

fn magnitude_note(m: &Magnitude) -> String {
    let mut parts = Vec::new();
    if let Some(a) = m.abs {
        parts.push(format!("{a:.0} m tier"));
    }
    if (m.scale - 1.0).abs() > 1e-3 {
        parts.push(format!("\u{00d7}{:.1} size", m.scale));
    }
    if let Some(e) = m.element {
        parts.push(format!("{e:?}"));
    }
    for x in &m.mech {
        parts.push(format!("{x:?}"));
    }
    parts.join(", ")
}

fn apply_effects(res: &mut Resolution, fx: Effects) {
    let rc = &mut res.recipe;
    let old = rc.scale.max(0.01);
    let mut target = (rc.scale * fx.scale_mul).clamp(0.02, 60.0);
    if let Some(a) = fx.abs {
        target = target.max(a);
    }
    let k = target / old;
    rc.scale = target;
    rc.mass = (rc.mass * k * k * k).clamp(0.01, 1e10);
    rc.mechanics.extend(fx.mech.iter().copied());
    if let Some(e) = fx.element {
        rc.element = e;
    }
    res.graft.scale_mul = (res.graft.scale_mul * k).clamp(0.05, 60.0);
    for m in &fx.mech {
        if !res.graft.mechanics.contains(m) {
            res.graft.mechanics.push(*m);
        }
    }
    res.graft.mechanics.sort();
    if fx.element.is_some() {
        res.graft.element = fx.element;
    }
    res.recipe = res.recipe.clone().normalized();
    for n in fx.notes.into_iter().rev() {
        res.explanation.insert(0, n);
    }
}

// ------------------------------------------------------------------------------------------ leftovers
/// What one leftover word adds (a light graft).
#[derive(Default)]
struct LightGraft {
    mech: Vec<Mechanic>,
    element: Option<Element>,
    temperament: Option<Temperament>,
    features: Vec<Feature>,
    via: String,
}

impl LightGraft {
    fn is_empty(&self) -> bool {
        self.mech.is_empty() && self.element.is_none() && self.temperament.is_none() && self.features.is_empty()
    }
    fn describe(&self, max_mechs: usize) -> String {
        let mut p: Vec<String> = self.mech.iter().take(max_mechs).map(|m| format!("{m:?}")).collect();
        if let Some(e) = self.element {
            p.push(format!("{e:?} tint"));
        }
        if let Some(t) = self.temperament {
            p.push(format!("{t:?}"));
        }
        p.extend(self.features.iter().map(|f| format!("{f:?}")));
        p.join(", ")
    }
}

/// Stems of a word by the data file's suffix rules ("destroying" -> destroy, destroye; "running" -> run).
fn stems(d: &RelData, w: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (suf, rep) in &d.leftover_suffixes {
        if let Some(stem) = w.strip_suffix(suf.as_str())
            && stem.len() >= 3
        {
            let s = format!("{stem}{rep}");
            let b = s.as_bytes();
            if rep.is_empty() && b.len() >= 4 && b[b.len() - 1] == b[b.len() - 2] && !b"aeiouls".contains(&b[b.len() - 1]) {
                let undoubled = s[..s.len() - 1].to_string();
                if !out.contains(&undoubled) {
                    out.push(undoubled);
                }
            }
            if !out.contains(&s) {
                out.push(s);
            }
        }
    }
    out
}

impl Lexicon {
    /// Leftover content words become light grafts on the result instead of vanishing silently
    /// ("destroying robot" -> robot + Explodes/Heavy). Unmapped words stay in `ignored`.
    pub(crate) fn absorb_leftovers(&self, res: &mut Resolution, bangs: usize, spoken: bool) {
        let Some(d) = data() else {
            return;
        };
        if res.ignored.is_empty() || res.stand_rush.is_some() {
            return;
        }
        let mut kept = Vec::new();
        let mut absorbed = 0;
        for w in std::mem::take(&mut res.ignored) {
            if w.len() < 3 || gram_of(&w).is_some() || !w.chars().all(|c| c.is_ascii_alphabetic()) {
                kept.push(w);
                continue;
            }
            match self.leftover_graft(d, &w, bangs, spoken) {
                Some(l) => {
                    apply_light(res, &l, d.leftover_max_mechs);
                    res.explanation.push(format!("{w} \u{2192} {} (leftover word{})", l.describe(d.leftover_max_mechs), l.via));
                    absorbed += 1;
                }
                None => kept.push(w),
            }
        }
        res.ignored = kept;
        if absorbed > 0 {
            if res.confidence <= 0.0 {
                // the whole phrase was one unknown-but-mappable word: a graft, not a glowing blob
                res.kind = ResolutionKind::Graft;
                res.confidence = d.leftover_min_conf;
                res.explanation.retain(|e| !e.starts_with("unknown word"));
            } else {
                res.confidence = (res.confidence / 0.85f32.powi(absorbed)).min(1.0);
            }
            res.recipe = res.recipe.clone().normalized();
        }
    }

    fn leftover_graft(&self, d: &RelData, w: &str, bangs: usize, spoken: bool) -> Option<LightGraft> {
        let mut cands = vec![w.to_string()];
        if let Some(l) = self.t.infl.get(w) {
            cands.push(l.clone());
        }
        cands.extend(stems(d, w));
        // 1. the data table, by word or stem
        for c in &cands {
            if let Some(t) = d.leftovers.iter().find(|t| t.words.iter().any(|x| x == c)) {
                let l = LightGraft { mech: t.mech.clone(), element: t.element, temperament: t.temperament, ..Default::default() };
                if !l.is_empty() {
                    return Some(l);
                }
            }
        }
        // 2. the ordinary single-word graft reading of a stem ("glimmering" -> glimmer)
        for c in cands.iter().skip(1).take(8) {
            if c.len() < 3 || gram_of(c).is_some() {
                continue;
            }
            let r = self.resolve_tokens_inner(vec![Tok { text: c.clone(), caps: false }], bangs, spoken, 0, false);
            // a graft word ("glimmer") gives its graft; a thing ("hammering" -> hammer) its noun graft
            let thing = r.kind == ResolutionKind::Construct && r.recipe.form != Form::Blob0;
            if !(r.kind == ResolutionKind::Graft || thing)
                || r.confidence < if thing { d.leftover_min_conf.max(0.7) } else { d.leftover_min_conf }
                || !r.ignored.is_empty()
                || r.explanation.iter().any(|e| e.contains("typo") || e.contains("sounds like"))
            {
                continue;
            }
            let l = LightGraft {
                mech: r.graft.mechanics.clone(),
                element: r.graft.element,
                temperament: if thing { None } else { r.graft.temperament },
                features: if thing { Vec::new() } else { r.graft.features.clone() },
                via: format!(" via {c}"),
            };
            if !l.is_empty() {
                return Some(l);
            }
        }
        None
    }
}

fn apply_light(res: &mut Resolution, l: &LightGraft, max_mechs: usize) {
    let rc = &mut res.recipe;
    for m in l.mech.iter().take(max_mechs) {
        if !rc.mechanics.contains(m) {
            rc.mechanics.push(*m);
        }
        if !res.graft.mechanics.contains(m) {
            res.graft.mechanics.push(*m);
        }
    }
    res.graft.mechanics.sort();
    if let Some(e) = l.element {
        if rc.element == Element::Light || rc.form == Form::Blob0 {
            rc.element = e;
        }
        res.graft.element.get_or_insert(e);
    }
    if let Some(t) = l.temperament {
        if rc.temperament == Temperament::Calm {
            rc.temperament = t;
            rc.intensity = rc.intensity.max(0.5);
        }
        res.graft.temperament.get_or_insert(t);
    }
    for f in &l.features {
        if !rc.features.contains(f) {
            rc.features.push(*f);
        }
        if !res.graft.features.contains(f) {
            res.graft.features.push(*f);
        }
    }
    res.graft.features.sort();
}

impl Lexicon {
    /// An unknown word whose stem is a reasonably common known word ("glimmering" -> glimmer): repair
    /// keeps it as typed instead of splitting or typo-correcting it, and the leftover pass maps it.
    pub(crate) fn known_stem(&self, s: &str) -> bool {
        let Some(d) = data() else {
            return false;
        };
        s.len() >= 6 && stems(d, s).iter().any(|x| x.len() >= 4 && self.known_token(x) && self.token_rank(x) >= 60)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn data_file_parses() {
        let d = super::data().expect("relations.ron parses");
        assert!(!d.attacks.is_empty() && !d.size_of.is_empty() && !d.gerunds.is_empty());
    }
}
