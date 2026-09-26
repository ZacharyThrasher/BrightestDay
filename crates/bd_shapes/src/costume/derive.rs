//! Tier 2: a costume derived from a curated humanoid icon (no hand-authored look).
//!
//! Icons only carry palette slots (+ one recipe accent colour), so the derivation works on the
//! slot *structure*: the dominant slot of each body region (torso, legs, feet, hands) becomes that
//! region's colour, the icon's own `Head` part is worn as the head (keeping its silhouette — cowl
//! ears, helmets, hair spikes), and `Cloth` / `Wing*` / `Tail` parts become a cape, wings, a tail.
//! Slot → colour: Accent = the recipe accent, Dark = a near-black tint of it, Primary / Core stay
//! unpainted (corps colour / hard-light core), exactly like the icon when cast as a construct.

use bd_core::mechanics::{Form, Recipe};
use bd_core::shape::{PartId, PartRole, ShapeSpec, Tint};
use glam::Vec3;

use super::look::*;
use super::paint::{Paint, Rgb};
use super::{Cx, HEAD};

/// Sentinel colours for "leave this unpainted" (never authored: the low bits differ from any look).
pub(super) const SENT_P: Rgb = [1, 2, 3];
pub(super) const SENT_C: Rgb = [4, 5, 6];

/// A humanoid character icon: a biped with a head and arms.
pub fn is_character_icon(recipe: &Recipe, icon: &ShapeSpec) -> bool {
    recipe.form == Form::Biped
        && icon.parts.iter().any(|p| p.role == PartRole::Head)
        && icon.parts.iter().filter(|p| matches!(p.role, PartRole::ArmLeft | PartRole::ArmRight)).count() >= 1
        && icon.prims.len() >= 6
}

fn slot_rgb(t: Tint, accent: Rgb, dark: Rgb) -> Rgb {
    match t {
        Tint::Primary => SENT_P,
        Tint::Accent => accent,
        Tint::Dark => dark,
        Tint::Core => SENT_C,
    }
}

/// Volume-weighted dominant slot of the prims `keep` selects.
fn dominant(spec: &ShapeSpec, keep: impl Fn(&bd_core::shape::Prim, f32) -> bool, lo: f32, hi: f32) -> Option<Tint> {
    let mut w = [0.0f32; 4];
    let h = (hi - lo).max(1e-3);
    for p in spec.prims.iter().filter(|p| !p.subtract && p.size.is_finite() && p.center.is_finite()) {
        let f = (p.center.y - lo) / h;
        if !keep(p, f) {
            continue;
        }
        let v = (p.size.x * p.size.y * p.size.z).abs().max(0.01);
        w[p.tint as usize] += v;
    }
    let (i, best) = w.iter().enumerate().fold((0, 0.0f32), |a, (i, x)| if *x > a.1 { (i, *x) } else { a });
    (best > 0.0).then_some([Tint::Primary, Tint::Accent, Tint::Dark, Tint::Core][i])
}

/// Build the derived look (sentinels for unpainted slots) + the extra parts to copy.
pub(super) fn derive_look(recipe: &Recipe, icon: &ShapeSpec) -> Look {
    let accent: Rgb = recipe.accent.map_or([200, 200, 210], |a| a.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8));
    let dark: Rgb = [0, 1, 2].map(|i| (accent[i] as f32 * 0.18 + 22.0) as u8);
    let role_of = |id: PartId| icon.parts.iter().find(|p| p.id == id).map(|p| p.role);
    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
    for p in icon.prims.iter().filter(|p| !p.subtract && p.center.is_finite()) {
        lo = lo.min(p.center.y - p.size.y.abs() * 0.5);
        hi = hi.max(p.center.y + p.size.y.abs() * 0.5);
    }
    let is_leg = |id: PartId| matches!(role_of(id), Some(PartRole::LegBackLeft | PartRole::LegBackRight | PartRole::LegFrontLeft | PartRole::LegFrontRight));
    let is_arm = |id: PartId| matches!(role_of(id), Some(PartRole::ArmLeft | PartRole::ArmRight));
    let torso = dominant(icon, |p, f| p.part == 0 && (0.42..0.8).contains(&f), lo, hi).unwrap_or(Tint::Primary);
    let legs = dominant(icon, |p, f| (is_leg(p.part) || p.part == 0) && (0.1..0.45).contains(&f), lo, hi).unwrap_or(torso);
    let feet = dominant(icon, |_, f| f < 0.1, lo, hi).unwrap_or(legs);
    let arms = dominant(icon, |p, _| is_arm(p.part), lo, hi).unwrap_or(torso);
    let belt = dominant(icon, |p, f| p.part == 0 && (0.4..0.5).contains(&f), lo, hi).filter(|t| *t != torso && *t != legs);
    let c = |t: Tint| Col(slot_rgb(t, accent, dark));
    let mut acc = Vec::new();
    for part in &icon.parts {
        match part.role {
            PartRole::Cloth => {
                let t = dominant(icon, |p, _| p.part == part.id, lo, hi).unwrap_or(Tint::Primary);
                if !acc.iter().any(|a| matches!(a, Acc::Cape(..))) {
                    acc.push(Acc::Cape(c(t), None, false));
                }
            }
            PartRole::WingLeft => {
                let t = dominant(icon, |p, _| p.part == part.id, lo, hi).unwrap_or(Tint::Primary);
                acc.push(Acc::Wings(WingKind::Mech, c(t)));
            }
            PartRole::Tail => {
                if !acc.iter().any(|a| matches!(a, Acc::Tail(..))) {
                    let t = dominant(icon, |p, _| p.part == part.id, lo, hi).unwrap_or(Tint::Primary);
                    acc.push(Acc::Tail(TailKind::Demon, c(t)));
                }
            }
            _ => {}
        }
    }
    Look {
        top: Some(Top { cut: Cut::Bodysuit, color: c(torso), alt: None, trim: None, sides: None, stripes: None }),
        sleeves: Some(Sleeves::Long),
        hands: Some(Hands::Gloves(c(arms))),
        legs: Some(Legs { cut: LegCut::Tights, color: c(legs), alt: None }),
        feet: Some(Feet { kind: FootKind::Boots, color: c(feet), alt: None }),
        belt: belt.map(|t| Belt::Belt(c(t), None)),
        acc: Some(acc),
        mouth: Some(false),
        ..Default::default()
    }
}

/// Wear the icon's head, painted by slot (Primary/Core stay palette).
pub(super) fn icon_head_painted(cx: &mut Cx, recipe: &Recipe, icon: &ShapeSpec) {
    let accent: Rgb = recipe.accent.map_or([200, 200, 210], |a| a.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8));
    let dark: Rgb = [0, 1, 2].map(|i| (accent[i] as f32 * 0.18 + 22.0) as u8);
    let p0 = cx.b.spec.prims.len();
    super::heads::icon_head(cx, icon);
    let n = cx.b.spec.prims.len();
    cx.paint.resize(p0, Paint::default());
    for i in p0..n {
        let t = cx.b.spec.prims[i].tint;
        let rgb = match t {
            Tint::Accent => Some(accent),
            Tint::Dark => Some(dark),
            _ => None,
        };
        cx.paint.push(Paint { rgb, decal: false });
    }
    let _ = HEAD;
    let _ = Vec3::ZERO;
}

/// Replace sentinel paints with palette slots.
pub(super) fn resolve_sentinels(spec: &mut ShapeSpec, paint: &mut [Paint]) {
    for (i, p) in paint.iter_mut().enumerate() {
        match p.rgb {
            Some(SENT_P) => {
                p.rgb = None;
                if let Some(pr) = spec.prims.get_mut(i) {
                    pr.tint = Tint::Primary;
                }
            }
            Some(SENT_C) => {
                p.rgb = None;
                if let Some(pr) = spec.prims.get_mut(i) {
                    pr.tint = Tint::Core;
                }
            }
            _ => {}
        }
    }
}
