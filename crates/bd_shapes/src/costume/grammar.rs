//! Mix-and-match costume phrases: a base character followed by modifier clauses, over a look made
//! of **slots** (hair, headwear, face, top, bottom, back, accessory, palette).
//!
//! * `gojo`, `the batman`, `goku super saiyan` — a character (+ variant words), as before.
//! * `with / wearing / in / and X` — add or replace the slot `X` belongs to. `X` is a generic part
//!   (assets/costumes/parts.ron: "straw hat", "cape", "sunglasses"…) or another character's
//!   signature part: `with luffy's hat`, `with naruto's headband`, `wearing vader's cape`.
//! * `but / without / no / minus X` — remove it: `gojo but bald`, `naruto without the headband`,
//!   `batman no cape`, `vader no helmet`. `bald`, `shirtless`, `barefoot`, `unmasked` work anywhere.
//! * Colour words recolour: `red goku` (the look's accent zone), `gojo with pink hair`,
//!   `batman with a gold cape`, `spider-man in black`.
//! * Mashups: `goku x naruto` / `goku naruto` / `goku and naruto` — the first character's head
//!   (hair, face, headwear, skin) on the second character's outfit (top, bottom, back, accessories).
//! * Never says no: unknown words are ignored when a character is present; a phrase made only of
//!   parts ("straw hat", "red cape and sunglasses") dresses a default body. Pure and deterministic in
//!   the text, so every peer derives the same look (no network data beyond the phrase).

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

use super::look::*;

/// Costume slots.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum Slot {
    Hair,
    Headwear,
    Face,
    Top,
    Bottom,
    Back,
    Accessory,
    Palette,
}

/// A generic part (assets/costumes/parts.ron).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct PartDef {
    pub names: Vec<String>,
    pub slot: Slot,
    #[serde(default)]
    pub look: Look,
    /// Hair-style parts keep the wearer's hair colour.
    #[serde(default)]
    pub hair: Option<HairStyle>,
}

pub const PARTS_RON: &str = include_str!("../../../../assets/costumes/parts.ron");

pub struct PartTable {
    pub parts: Vec<PartDef>,
    pub index: HashMap<String, usize>,
    pub error: Option<String>,
}

pub fn parse_parts(src: &str) -> PartTable {
    let opts = ron::Options::default().with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME);
    let (parts, error) = match opts.from_str::<Vec<PartDef>>(src) {
        Ok(p) => (p, None),
        Err(e) => (Vec::new(), Some(e.to_string())),
    };
    let mut index = HashMap::new();
    for (i, p) in parts.iter().enumerate() {
        for n in &p.names {
            index.entry(tokens(n).concat()).or_insert(i);
        }
    }
    PartTable { parts, index, error }
}

pub fn parts() -> &'static PartTable {
    static T: OnceLock<PartTable> = OnceLock::new();
    T.get_or_init(|| parse_parts(PARTS_RON))
}

// ------------------------------------------------------------------------------------ words

fn colour_word(w: &str) -> Option<Rgb3> {
    Some(match w {
        "red" | "crimson" | "scarlet" => [210, 30, 36],
        "blue" => [40, 80, 210],
        "navy" => [24, 30, 70],
        "cyan" | "aqua" | "turquoise" => [40, 200, 220],
        "teal" => [30, 140, 140],
        "green" => [40, 160, 60],
        "lime" => [140, 230, 60],
        "yellow" => [250, 214, 40],
        "gold" | "golden" => [232, 184, 52],
        "orange" => [245, 122, 24],
        "purple" | "violet" => [124, 52, 180],
        "magenta" => [220, 40, 180],
        "pink" => [245, 140, 180],
        "black" | "dark" => [22, 22, 26],
        "white" => [242, 242, 246],
        "grey" | "gray" => [124, 128, 136],
        "silver" | "chrome" => [196, 202, 212],
        "brown" => [112, 72, 40],
        "maroon" => [120, 20, 30],
        "beige" | "tan" => [214, 190, 150],
        _ => return None,
    })
}
type Rgb3 = [u8; 3];

/// Slot nouns (after a possessive, a colour, or `no`).
fn slot_noun(w: &str) -> Option<Slot> {
    Some(match w {
        "hair" | "hairstyle" | "haircut" | "hairdo" => Slot::Hair,
        "hat" | "headwear" | "helmet" | "headgear" | "crown" => Slot::Headwear,
        "face" | "mask" | "eyes" | "eyewear" | "beard" => Slot::Face,
        "outfit" | "clothes" | "costume" | "uniform" | "top" | "shirt" | "gi" | "suit" | "armor" | "armour" | "jacket" | "clothing" => {
            Slot::Top
        }
        "pants" | "trousers" | "legs" | "bottoms" | "shorts" | "boots" | "shoes" | "skirt" => Slot::Bottom,
        "cape" | "cloak" | "back" | "wings" | "weapon" | "sword" => Slot::Back,
        "accessories" | "accessory" | "gear" | "stuff" => Slot::Accessory,
        "colors" | "colours" | "palette" | "color" | "colour" => Slot::Palette,
        _ => return None,
    })
}

const ADD: &[&str] = &["with", "wearing", "in", "and", "plus", "x", "feat", "ft", "has", "having", "wears", "dressed", "as"];
const REMOVE: &[&str] = &["but", "without", "no", "minus", "sans", "except", "lacking", "not", "remove", "removed", "take", "off"];
const FILLER: &[&str] = &[
    "a", "an", "the", "his", "her", "their", "its", "some", "of", "on", "like", "style", "version", "cosplay", "look", "form", "mode",
    "own", "pair", "fancy", "cool",
];
/// Size / count words a summon may carry ("giant goku") — never part of the look.
const SCALE: &[&str] = &[
    "giant", "huge", "big", "tiny", "small", "mini", "little", "massive", "colossal", "gigantic", "enormous", "kaiju", "titan", "titanic",
    "mega", "baby", "chibi", "two", "three", "2", "3", "army", "clone", "clones",
];

/// Words that say nothing about the look or the construct: filler and size / count words
/// ("the giant satoru gojo" is plain Gojo, scaled).
pub(crate) fn plain_word(w: &str) -> bool {
    FILLER.contains(&w) || SCALE.contains(&w) || super::look::FILLER.contains(&w)
}

/// What the phrase asked for.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Parsed {
    /// Base character + variant picked by words.
    pub base: Option<(usize, usize)>,
    pub ops: Vec<Op>,
    /// Words nothing understood.
    pub unknown: Vec<String>,
    /// Scale / count words (summons keep them in the recipe).
    pub scale_words: usize,
    /// Those scale / count words, in order ("giant", "army").
    pub scale: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    /// Apply a generic part (index into [`parts`]).
    Part(usize),
    /// Recoloured part.
    PartColoured(usize, Rgb3),
    /// Take a slot from another character.
    Take(usize, Slot),
    /// Remove a slot.
    Clear(Slot),
    /// Recolour a slot (`Palette` = the look's accent zone).
    Recolour(Slot, Rgb3),
    /// Second character of a mashup (outfit from it).
    Mash(usize),
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Char(usize, usize),
    Poss,
    Part(usize),
    Slot(Slot),
    Colour(Rgb3),
    Add,
    Remove,
    Special(Op),
    Filler,
    Scale(String),
    Unknown(String),
}

/// Lowercase tokens with a possessive marker (`luffy's hat` → `luffy`, `'s`, `hat`).
fn toks(phrase: &str) -> Vec<String> {
    let p = phrase.to_lowercase().replace('’', "'");
    let mut out = Vec::new();
    for raw in p.split(|c: char| c.is_whitespace() || c == ',' || c == '/' || c == '&') {
        if raw == "+" || raw == "x" || raw == "×" {
            out.push("x".to_string());
            continue;
        }
        let (word, poss) = match raw.strip_suffix("'s").or_else(|| raw.strip_suffix("s'")) {
            Some(w) if !w.is_empty() => (w, true),
            _ => (raw, false),
        };
        for t in tokens(word) {
            out.push(t);
        }
        if poss {
            out.push("'s".to_string());
        }
    }
    out
}

fn lex(phrase: &str) -> Vec<Tok> {
    let ct = table();
    let pt = parts();
    let w = toks(phrase);
    let mut out = Vec::new();
    let mut i = 0;
    while i < w.len() {
        if w[i] == "'s" {
            out.push(Tok::Poss);
            i += 1;
            continue;
        }
        // Longest window first: characters, then parts.
        let mut hit = None;
        for len in (1..=5.min(w.len() - i)).rev() {
            let win = &w[i..i + len];
            if win.iter().any(|t| t == "'s") {
                continue;
            }
            let k: String = win.concat();
            if let Some(&c) = ct.index.get(&k) {
                hit = Some((len, Tok::Char(c, 0)));
                break;
            }
            if let Some(&p) = pt.index.get(&k) {
                hit = Some((len, Tok::Part(p)));
                break;
            }
        }
        if let Some((len, t)) = hit {
            out.push(t);
            i += len;
            continue;
        }
        let t = w[i].as_str();
        out.push(if let Some(c) = colour_word(t) {
            Tok::Colour(c)
        } else if let Some(s) = slot_noun(t) {
            Tok::Slot(s)
        } else if ADD.contains(&t) {
            Tok::Add
        } else if REMOVE.contains(&t) {
            Tok::Remove
        } else if matches!(t, "bald" | "hairless" | "shaved") {
            Tok::Special(Op::Clear(Slot::Hair))
        } else if matches!(t, "shirtless" | "topless") {
            Tok::Special(Op::Clear(Slot::Top))
        } else if matches!(t, "unmasked" | "maskless" | "barefaced") {
            Tok::Special(Op::Clear(Slot::Face))
        } else if matches!(t, "hatless" | "helmetless") {
            Tok::Special(Op::Clear(Slot::Headwear))
        } else if matches!(t, "capeless") {
            Tok::Special(Op::Clear(Slot::Back))
        } else if t == "barefoot" {
            Tok::Special(Op::Clear(Slot::Bottom))
        } else if FILLER.contains(&t) {
            Tok::Filler
        } else if SCALE.contains(&t) {
            Tok::Scale(t.to_string())
        } else {
            Tok::Unknown(t.to_string())
        });
        i += 1;
    }
    out
}

/// Parse a costume phrase (see the module docs). `None` = no character and not purely parts.
pub fn parse(phrase: &str) -> Option<Parsed> {
    // Exact alias (+ variant words) first: `gojo sunglasses` is Gojo's own sunglasses variant.
    if let Some(f) = find(phrase) {
        return Some(Parsed { base: Some((f.index, f.base_variant)), ..Default::default() });
    }
    let t = lex(phrase);
    let mut p = Parsed::default();
    let mut remove = false;
    let mut colour: Option<Rgb3> = None;
    let mut i = 0;
    while i < t.len() {
        match &t[i] {
            Tok::Char(c, _) => {
                let c = *c;
                // `luffy's hat` / `luffy hat`: a slot taken from that character.
                let poss = matches!(t.get(i + 1), Some(Tok::Poss));
                let j = if poss { i + 2 } else { i + 1 };
                // Without an apostrophe only a second character reads as possessive ("goku sword"
                // is Goku with a sword; "goku luffy hat" is Goku with Luffy's hat).
                let target = match t.get(j) {
                    _ if !poss && p.base.is_none() => None,
                    Some(Tok::Part(pi)) => Some(parts().parts[*pi].slot),
                    Some(Tok::Slot(s)) => Some(*s),
                    _ => None,
                };
                match target {
                    Some(slot) if !remove => {
                        let outfit = matches!(t.get(j), Some(Tok::Slot(Slot::Top))) && slot == Slot::Top;
                        p.ops.push(Op::Take(c, slot));
                        if outfit {
                            p.ops.push(Op::Take(c, Slot::Bottom));
                        }
                        i = j + 1;
                        continue;
                    }
                    _ => {}
                }
                if poss {
                    i += 2;
                    continue;
                }
                if p.base.is_none() {
                    p.base = Some((c, 0));
                    if let Some(col) = colour.take() {
                        p.ops.push(Op::Recolour(Slot::Palette, col));
                    }
                } else if !remove {
                    p.ops.push(Op::Mash(c));
                }
            }
            Tok::Part(pi) => {
                if remove {
                    p.ops.push(Op::Clear(parts().parts[*pi].slot));
                } else {
                    match colour.take() {
                        Some(col) => p.ops.push(Op::PartColoured(*pi, col)),
                        None => p.ops.push(Op::Part(*pi)),
                    }
                }
            }
            Tok::Slot(s) => {
                if remove {
                    p.ops.push(Op::Clear(*s));
                } else if let Some(col) = colour.take() {
                    p.ops.push(Op::Recolour(*s, col));
                }
            }
            Tok::Colour(c) => {
                // A colour applies to the next part / slot / character, else to the palette.
                match t.get(i + 1) {
                    Some(Tok::Part(_) | Tok::Slot(_)) => colour = Some(*c),
                    Some(Tok::Char(..)) if p.base.is_none() => colour = Some(*c),
                    _ => p.ops.push(Op::Recolour(Slot::Palette, *c)),
                }
            }
            Tok::Add => remove = false,
            Tok::Remove => remove = true,
            Tok::Special(op) => p.ops.push(op.clone()),
            Tok::Poss | Tok::Filler => {}
            Tok::Scale(w) => {
                p.scale_words += 1;
                p.scale.push(w.clone());
            }
            Tok::Unknown(w) => p.unknown.push(w.clone()),
        }
        i += 1;
    }
    if p.base.is_some() || (!p.ops.is_empty() && p.unknown.is_empty()) { Some(p) } else { None }
}

// ------------------------------------------------------------------------------------ compose

/// A plain body for part-only phrases ("straw hat").
pub fn default_look() -> Look {
    Look {
        skin: Some(Col([236, 196, 160])),
        hair: Some(Hair { style: HairStyle::Short, color: Col([80, 52, 34]), tips: None, fringe: None }),
        eyes: Some(Eyes { color: Col([70, 50, 40]), style: EyeStyle::Normal }),
        top: Some(Top { cut: Cut::Tee, color: Col([236, 236, 236]), alt: None, trim: None, sides: None, stripes: None }),
        legs: Some(Legs { cut: LegCut::Pants, color: Col([58, 84, 130]), alt: None }),
        feet: Some(Feet { kind: FootKind::Sneakers, color: Col([60, 60, 66]), alt: None }),
        ..Default::default()
    }
}

fn is_back(a: &Acc) -> bool {
    matches!(
        a,
        Acc::Cape(..)
            | Acc::Cloak(..)
            | Acc::Wings(..)
            | Acc::BackSword(..)
            | Acc::BigSword(..)
            | Acc::TwinSwords(..)
            | Acc::Staff(..)
            | Acc::RoundShield(..)
            | Acc::KiteShield(..)
            | Acc::Backpack(..)
            | Acc::Jetpack(..)
            | Acc::Gourd(..)
            | Acc::Quiver(..)
            | Acc::Bow(..)
            | Acc::Polearm(..)
            | Acc::Axe(..)
            | Acc::Shell(..)
    )
}

/// Copy `slot` of `from` into `to`.
fn take(to: &mut Look, from: &Look, slot: Slot) {
    let mut acc = to.acc.take().unwrap_or_default();
    let src = from.acc.clone().unwrap_or_default();
    match slot {
        Slot::Hair => {
            to.hair = from.hair.or(Some(Hair { style: HairStyle::Bald, color: Col([0, 0, 0]), tips: None, fringe: None }));
            to.brows = from.brows.or(to.brows);
        }
        Slot::Headwear => {
            to.hat = Some(from.hat.unwrap_or(Hat::No));
            to.helmet = Some(from.helmet.unwrap_or(Helmet::No));
        }
        Slot::Face => {
            to.mask = Some(from.mask.unwrap_or(Mask::No));
            to.eyewear = Some(from.eyewear.unwrap_or(Eyewear::No));
            to.facial = from.facial.clone();
            to.face_paint = from.face_paint.clone();
            to.eyes = from.eyes.or(to.eyes);
        }
        Slot::Top => {
            to.top = from.top;
            to.sleeves = from.sleeves;
            to.hands = from.hands;
            to.emblem = from.emblem.clone();
            to.back_emblem = from.back_emblem.clone();
            to.belt = from.belt;
            to.build = from.build;
        }
        Slot::Bottom => {
            to.legs = from.legs;
            to.feet = from.feet;
        }
        Slot::Back => {
            acc.retain(|a| !is_back(a));
            acc.extend(src.iter().filter(|a| is_back(a)).cloned());
        }
        Slot::Accessory => {
            acc.retain(is_back);
            acc.extend(src.iter().filter(|a| !is_back(a)).cloned());
        }
        Slot::Palette => {
            if let (Some(t), Some(f)) = (to.top.as_mut(), from.top) {
                t.color = f.color;
                t.alt = f.alt.or(t.alt);
            }
            if let (Some(l), Some(f)) = (to.legs.as_mut(), from.legs) {
                l.color = f.color;
            }
        }
    }
    to.acc = Some(acc);
}

fn clear(l: &mut Look, slot: Slot) {
    let mut acc = l.acc.take().unwrap_or_default();
    match slot {
        Slot::Hair => {
            let c = l.hair.map_or(Col([40, 30, 24]), |h| h.color);
            l.hair = Some(Hair { style: HairStyle::Bald, color: c, tips: None, fringe: None });
        }
        Slot::Headwear => {
            l.hat = Some(Hat::No);
            l.helmet = Some(Helmet::No);
        }
        Slot::Face => {
            l.mask = Some(Mask::No);
            l.eyewear = Some(Eyewear::No);
            l.facial = Some(Vec::new());
            l.face_paint = None;
        }
        Slot::Top => {
            if let Some(t) = l.top.as_mut() {
                t.cut = Cut::Bare;
            }
            l.emblem = None;
            l.back_emblem = None;
        }
        Slot::Bottom => {
            l.feet = Some(Feet { kind: FootKind::Bare, color: Col([0, 0, 0]), alt: None });
        }
        Slot::Back => acc.retain(|a| !is_back(a)),
        Slot::Accessory => {
            acc.retain(is_back);
            l.belt = Some(Belt::No);
        }
        Slot::Palette => {}
    }
    l.acc = Some(acc);
}

/// Replace the first colour of a part patch (its "main" colour).
fn recolour_patch(l: &mut Look, c: Rgb3) {
    let c = Col(c);
    if let Some(h) = l.hat.as_mut() {
        match h {
            Hat::Cap(x, _)
            | Hat::Pointy(x)
            | Hat::Straw(x, _)
            | Hat::Wizard(x, _)
            | Hat::Cowboy(x)
            | Hat::Fedora(x, _)
            | Hat::TopHat(x, _)
            | Hat::Crown(x)
            | Hat::Beanie(x) => *x = c,
            Hat::Headband(x, _)
            | Hat::Bandana(x)
            | Hat::Horns(x)
            | Hat::Halo(x)
            | Hat::CatEars(x)
            | Hat::Hood(x)
            | Hat::Tiara(x)
            | Hat::Winged(x, _)
            | Hat::LokiHorns(x) => *x = c,
            Hat::Beret(x) | Hat::Bowler(x) | Hat::Bucket(x) | Hat::Headphones(x) | Hat::Tied(x) | Hat::Tricorn(x, _) | Hat::Ears(x) => {
                *x = c
            }
            Hat::No => {}
        }
    }
    if let Some(t) = l.top.as_mut() {
        t.color = c;
    }
    if let Some(g) = l.legs.as_mut()
        && l.top.is_none()
    {
        g.color = c;
    }
    match l.eyewear.as_mut() {
        Some(
            Eyewear::Blindfold(x)
            | Eyewear::RoundShades(x)
            | Eyewear::Shades(x)
            | Eyewear::Glasses(x)
            | Eyewear::Visor(x)
            | Eyewear::Eyepatch(x, _)
            | Eyewear::Bandage(x),
        ) => *x = c,
        Some(Eyewear::Goggles(_, x)) => *x = c,
        _ => {}
    }
    for a in l.add.iter_mut().flatten() {
        match a {
            Acc::Cape(x, _, _)
            | Acc::Cloak(x)
            | Acc::Scarf(x)
            | Acc::Wings(_, x)
            | Acc::Tail(_, x)
            | Acc::Backpack(x)
            | Acc::Jetpack(x, _)
            | Acc::Staff(x) => *x = c,
            Acc::Pauldrons(x)
            | Acc::Necklace(x)
            | Acc::Saber(x)
            | Acc::Holsters(x)
            | Acc::Gourd(x)
            | Acc::Claws(x)
            | Acc::Spikes(x)
            | Acc::Bandolier(x) => *x = c,
            Acc::BackSword(_, x) | Acc::BigSword(_, x) | Acc::TwinSwords(_, x) => *x = c,
            _ => {}
        }
    }
}

fn recolour(l: &mut Look, slot: Slot, c: Rgb3) {
    let col = Col(c);
    match slot {
        Slot::Hair => {
            if let Some(h) = l.hair.as_mut() {
                h.color = col;
                h.tips = None;
            } else {
                l.hair = Some(Hair { style: HairStyle::Short, color: col, tips: None, fringe: None });
            }
        }
        Slot::Headwear | Slot::Face => {
            let mut patch = Look { hat: l.hat, eyewear: l.eyewear, ..Default::default() };
            recolour_patch(&mut patch, c);
            if slot == Slot::Headwear {
                l.hat = patch.hat;
                if let Some(
                    Helmet::Knight(x, _)
                    | Helmet::Vader(x)
                    | Helmet::Space(x, _)
                    | Helmet::Robot(x, _)
                    | Helmet::Smooth(x, _)
                    | Helmet::Mando(x, _)
                    | Helmet::Chief(x, _),
                ) = l.helmet.as_mut()
                {
                    *x = col;
                }
            } else {
                l.eyewear = patch.eyewear;
                if let Some(
                    Mask::Ninja(x) | Mask::Bandana(x) | Mask::Domino(x) | Mask::Cowl(x, _) | Mask::Spider(x, _) | Mask::Full(x, _, _),
                ) = l.mask.as_mut()
                {
                    *x = col;
                }
            }
        }
        Slot::Top => {
            if let Some(t) = l.top.as_mut() {
                t.color = col;
            }
        }
        Slot::Bottom => {
            if let Some(g) = l.legs.as_mut() {
                g.color = col;
            }
        }
        Slot::Back => {
            for a in l.acc.iter_mut().flatten() {
                if let Acc::Cape(x, _, _) | Acc::Cloak(x) | Acc::Wings(_, x) = a {
                    *x = col;
                }
            }
        }
        Slot::Accessory => {}
        Slot::Palette => {
            let zone = l.accent.unwrap_or(Zone::Top);
            match zone {
                Zone::Top => recolour(l, Slot::Top, c),
                Zone::Alt => {
                    if let Some(t) = l.top.as_mut() {
                        t.alt = Some(col);
                    }
                }
                Zone::Legs => recolour(l, Slot::Bottom, c),
                Zone::Hair => recolour(l, Slot::Hair, c),
                Zone::Cape => recolour(l, Slot::Back, c),
                Zone::Trim => {
                    if let Some(t) = l.top.as_mut() {
                        t.trim = Some(col);
                    }
                }
            }
            // Bodysuits read as one colour: legs follow the top when they matched it.
            if let (Some(t), Some(g)) = (l.top, l.legs.as_mut())
                && zone == Zone::Top
                && matches!(t.cut, Cut::Bodysuit | Cut::Jumpsuit | Cut::Gi | Cut::Robe | Cut::Dress | Cut::Kimono)
            {
                g.color = col;
            }
        }
    }
}

fn apply_part(l: &mut Look, pi: usize, colour: Option<Rgb3>) {
    let p = &parts().parts[pi];
    if let Some(style) = p.hair {
        let c = l.hair.map_or(Col([80, 52, 34]), |h| h.color);
        let tips = l.hair.and_then(|h| h.tips);
        l.hair = Some(Hair { style, color: colour.map_or(c, Col), tips, fringe: None });
        return;
    }
    let mut patch = p.look.clone();
    if let Some(c) = colour {
        recolour_patch(&mut patch, c);
    }
    // One thing per slot: a hat replaces a helmet, a new cape replaces the old one.
    match p.slot {
        Slot::Headwear => {
            if patch.hat.is_some() {
                l.helmet = Some(Helmet::No);
            }
            if patch.helmet.is_some() {
                l.hat = Some(Hat::No);
            }
        }
        Slot::Face if patch.mask.is_some() => l.eyewear = Some(Eyewear::No),
        _ => {}
    }
    let mut acc = l.acc.take().unwrap_or_default();
    for new in patch.add.iter().flatten() {
        acc.retain(|a| {
            std::mem::discriminant(a) != std::mem::discriminant(new)
                && !(is_back(a)
                    && is_back(new)
                    && matches!(new, Acc::Cape(..) | Acc::Cloak(..))
                    && matches!(a, Acc::Cape(..) | Acc::Cloak(..)))
        });
    }
    acc.extend(patch.add.take().unwrap_or_default());
    let merged = l.patched(&Look { acc: Some(acc), ..patch });
    *l = merged;
}

/// The look a parsed phrase asks for (`variant` = the base character's variant).
pub fn compose(p: &Parsed, variant: usize) -> Look {
    let t = table();
    let mut l = match p.base {
        Some((c, v)) => t.chars[c].look_of(v + variant),
        None => default_look(),
    };
    for op in &p.ops {
        match op {
            Op::Part(pi) => apply_part(&mut l, *pi, None),
            Op::PartColoured(pi, c) => apply_part(&mut l, *pi, Some(*c)),
            Op::Take(c, slot) => {
                let from = t.chars[*c].look_of(0);
                take(&mut l, &from, *slot);
            }
            Op::Mash(c) => {
                let from = t.chars[*c].look_of(0);
                for s in [Slot::Top, Slot::Bottom, Slot::Back, Slot::Accessory] {
                    take(&mut l, &from, s);
                }
            }
            Op::Clear(s) => clear(&mut l, *s),
            Op::Recolour(s, c) => recolour(&mut l, *s, *c),
        }
    }
    l
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_parse() {
        assert!(parts().error.is_none(), "{:?}", parts().error);
        assert!(parts().parts.len() >= 60);
    }

    #[test]
    fn tokenises_possessives() {
        assert_eq!(toks("Luffy's hat"), vec!["luffy", "'s", "hat"]);
        assert_eq!(toks("goku + naruto"), vec!["goku", "x", "naruto"]);
    }
}
