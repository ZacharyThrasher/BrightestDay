//! Style library: reusable voxel building blocks at costume resolution (1/16 m), parametric by colour,
//! that dress the fixed humanoid rig from a [`Look`] — hair styles, faces, eyewear, masks, helmets,
//! hats, outfit cuts per region, belts, emblems (pixel mosaics) and accessories.
//!
//! Everything is authored on the voxel lattice (`Pn::vb` covers exact voxel index ranges) so faces
//! and emblems land on whole voxels. The rig (voxel indices, +Y up, facing −Z):
//!
//! | region | x | y | z |
//! |---|---|---|---|
//! | head (skull) | −4..4 | 21..27 (jaw 20) | −4..3 (face = −4) |
//! | neck | −1..1 | 19 | −2..0 |
//! | torso (root) | −3..3 | 10..18 (pelvis 10..11) | −2..1 (chest = −2) |
//! | arms | ±4..±6 | 9..18 (hand 9..11, forearm 12..14, upper 15..18) | −2..0 |
//! | legs | ±1..±3 | 0..9 (feet 0..1, toe z −3) | −2..1 |
//!
//! Part ids / roles / pivots match the vocabulary costume, so every animation, the 3P hand lookup
//! and the first-person hands work unchanged. Hitboxes never depend on the costume.

use bd_core::shape::{PartRole, Tint};
use glam::{IVec3, Quat, Vec3};

use super::look::*;
use super::paint::{Pn, Rgb};
use super::{ARM_L, ARM_R, Cx, HEAD, LEG_L, LEG_R, acc, rig};
use crate::build::v3;

const FACE: i32 = -4;
const DEFAULT_SKIN: Rgb = [244, 204, 170];

fn mul(c: Rgb, k: f32) -> Rgb {
    c.map(|x| (x as f32 * k).round().clamp(0.0, 255.0) as u8)
}
fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    [0, 1, 2].map(|i| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t).round().clamp(0.0, 255.0) as u8)
}
fn lum(c: Rgb) -> f32 {
    (0.2126 * c[0] as f32 + 0.7152 * c[1] as f32 + 0.0722 * c[2] as f32) / 255.0
}
/// A visible outline/detail colour against `c` (darker, or lighter for very dark colours).
fn shade(c: Rgb) -> Rgb {
    if lum(c) < 0.18 { mix(c, [255, 255, 255], 0.22) } else { mul(c, 0.62) }
}

const WHITE: Rgb = [242, 242, 246];
const BLACK: Rgb = [20, 20, 24];

/// Resolved look (defaults filled, accent applied).
struct Kit {
    skin: Rgb,
    broad: bool,
    hair: Option<Hair>,
    eyes: Eyes,
    brows: Option<Rgb>,
    mouth: bool,
    facial: Vec<Facial>,
    eyewear: Eyewear,
    mask: Mask,
    helmet: Helmet,
    hat: Hat,
    top: Top,
    sleeves: Sleeves,
    hands: Hands,
    legs: Legs,
    feet: Feet,
    belt: Belt,
    emblem: Option<Mosaic>,
    back_emblem: Option<Mosaic>,
    face_paint: Option<Mosaic>,
    acc: Vec<Acc>,
}

fn resolve(l: &Look, accent: Option<Rgb>) -> Kit {
    let top = l.top.unwrap_or(Top { cut: Cut::Tee, color: Col([235, 235, 235]), alt: None, trim: None, sides: None, stripes: None });
    let default_sleeves = match top.cut {
        Cut::Tee | Cut::Dress => Sleeves::Short,
        Cut::Bare | Cut::Tank | Cut::Gi | Cut::Vest => Sleeves::Bare,
        _ => Sleeves::Long,
    };
    let legs = l.legs.unwrap_or(Legs {
        cut: match top.cut {
            Cut::Gi => LegCut::Baggy,
            Cut::Robe => LegCut::LongSkirt,
            Cut::Dress => LegCut::LongSkirt,
            Cut::Jumpsuit | Cut::Bodysuit => LegCut::Tights,
            Cut::Power | Cut::Plate => LegCut::Armored,
            _ => LegCut::Pants,
        },
        color: match top.cut {
            Cut::Gi | Cut::Robe | Cut::Dress | Cut::Jumpsuit | Cut::Bodysuit | Cut::Suit | Cut::Power | Cut::Plate | Cut::HighCollar | Cut::Kimono => top.color,
            Cut::Overalls => top.alt.unwrap_or(Col([40, 70, 170])),
            _ => Col([50, 60, 90]),
        },
        alt: None,
    });
    let mut k = Kit {
        skin: l.skin.map_or(DEFAULT_SKIN, Col::rgb),
        broad: l.build == Some(Build::Broad),
        hair: l.hair,
        eyes: l.eyes.unwrap_or(Eyes { color: Col([70, 50, 40]), style: EyeStyle::Normal }),
        brows: l.brows.map(Col::rgb),
        mouth: l.mouth.unwrap_or(true),
        facial: l.facial.clone().unwrap_or_default(),
        eyewear: l.eyewear.unwrap_or(Eyewear::No),
        mask: l.mask.unwrap_or(Mask::No),
        helmet: l.helmet.unwrap_or(Helmet::No),
        hat: l.hat.unwrap_or(Hat::No),
        top,
        sleeves: l.sleeves.unwrap_or(default_sleeves),
        hands: l.hands.unwrap_or(Hands::Bare),
        legs,
        feet: l.feet.unwrap_or(Feet { kind: FootKind::Shoes, color: Col([60, 45, 35]), alt: None }),
        belt: l.belt.unwrap_or(Belt::No),
        emblem: l.emblem.clone(),
        back_emblem: l.back_emblem.clone(),
        face_paint: l.face_paint.clone(),
        acc: l.acc.clone().unwrap_or_default(),
    };
    if let Some(a) = accent {
        let a = Col(a);
        match l.accent.unwrap_or(Zone::Top) {
            Zone::Top => k.top.color = a,
            Zone::Alt => k.top.alt = Some(a),
            Zone::Legs => k.legs.color = a,
            Zone::Hair => {
                if let Some(h) = k.hair.as_mut() {
                    h.color = a;
                }
            }
            Zone::Trim => k.top.trim = Some(a),
            Zone::Cape => {
                for x in &mut k.acc {
                    if let Acc::Cape(c, _, _) | Acc::Cloak(c) = x {
                        *c = a;
                    }
                }
            }
        }
    }
    k
}

// ------------------------------------------------------------------------------------ entry

/// Dress the rig from a look. `accent` = the lobby ACCENT tweak (replaces the look's accent zone).
/// `own_head = false` leaves the head to the caller (derived looks wear the icon's head).
pub(super) fn dress(cx: &mut Cx, look: &Look, accent: Option<Rgb>, own_head: bool) {
    let k = resolve(look, accent);
    // Parts first so the ids match HEAD / ARM_* / LEG_*.
    {
        let b = &mut cx.b;
        let h = b.part(0, v3(0.0, rig::NECK_Y, 0.0), PartRole::Head);
        debug_assert_eq!(h, HEAD);
        for (id, s) in [(ARM_L, -1.0f32), (ARM_R, 1.0)] {
            let got = b.part(0, v3(s * rig::SHOULDER.x, rig::SHOULDER.y, 0.0), if s < 0.0 { PartRole::ArmLeft } else { PartRole::ArmRight });
            debug_assert_eq!(got, id);
        }
        for (id, s) in [(LEG_L, -1.0f32), (LEG_R, 1.0)] {
            let got = b.part(0, v3(s * rig::HIP_X, rig::HIP_Y, 0.0), if s < 0.0 { PartRole::LegBackLeft } else { PartRole::LegBackRight });
            debug_assert_eq!(got, id);
        }
        b.on(0);
    }
    let hide = cx.hide;
    let mut k = k;
    if !own_head {
        k.hair = None;
        k.helmet = Helmet::No;
        k.mask = Mask::No;
        k.eyewear = Eyewear::No;
        k.hat = Hat::No;
    }
    let helmet_on = !matches!(k.helmet, Helmet::No) && hide & acc::HEAD == 0;
    let mask_on = !matches!(k.mask, Mask::No) && hide & acc::FACE == 0;
    let full_mask = mask_on && matches!(k.mask, Mask::Spider(..) | Mask::Full(..) | Mask::Cowl(..) | Mask::Flared(..));
    {
        let mut p = cx.pn();
        if own_head {
            head_base(&mut p, &k);
        }
        if own_head && !helmet_on && !full_mask {
            face(&mut p, &k, hide);
        }
        body(&mut p, &k, hide);
        // Lantern ring on the ring hand: corps-coloured hard light (unpainted Core).
        p.on(ARM_R).palette(Tint::Core).px(4, 10, if matches!(k.hands, Hands::Gauntlets(_)) && hide & acc::ARMS == 0 { -4 } else { -3 });
        p.on(0);
    }
    // Hair (under hats; hidden by helmets and full masks).
    if let Some(h) = k.hair
        && !helmet_on
        && !full_mask
    {
        let hat_on = !matches!(k.hat, Hat::No) && hide & acc::HEAD == 0;
        cx.pgroup(acc::HEAD, v3(0.0, 27.0, 0.0), |p| {
            p.on(HEAD);
            hair(p, &h, hat_on);
        });
    }
    if mask_on {
        let m = k.mask;
        let skin = k.skin;
        let eyes = k.eyes;
        cx.pgroup(acc::FACE, v3(0.0, 24.0, -4.0), |p| {
            p.on(HEAD);
            mask(p, m, skin, eyes);
        });
    }
    if helmet_on {
        let hm = k.helmet;
        cx.pgroup(acc::HEAD, v3(0.0, 24.0, 0.0), |p| {
            p.on(HEAD);
            helmet(p, hm);
        });
    }
    if !matches!(k.eyewear, Eyewear::No) && !helmet_on {
        let e = k.eyewear;
        cx.pgroup(acc::FACE, v3(0.0, 24.0, -4.0), |p| {
            p.on(HEAD);
            eyewear(p, e);
        });
    }
    if !matches!(k.hat, Hat::No) && !helmet_on {
        let h = k.hat;
        let tall_hair = k.hair.is_some_and(|h| matches!(h.style, HairStyle::SpikyUp | HairStyle::Saiyan | HairStyle::Flame | HairStyle::Afro));
        cx.pgroup(acc::HEAD, v3(0.0, 28.0, 0.0), |p| {
            p.on(HEAD);
            hat(p, h, tall_hair);
        });
    }
    for a in k.acc.clone() {
        accessory(cx, &a, &k);
    }
}

// ------------------------------------------------------------------------------------ head

fn head_base(p: &mut Pn, k: &Kit) {
    p.on(HEAD).c(k.skin);
    p.vb(-4, 4, 21, 27, FACE, 3);
    p.vb(-3, 3, 20, 20, -3, 2);
    p.vb(-1, 1, 19, 19, -2, 0);
    // Ears (hair usually covers them).
    let ear = mul(k.skin, 0.93);
    p.c(ear).vbs(5, 5, 23, 24, -1, 0);
}

fn face(p: &mut Pn, k: &Kit, hide: u32) {
    p.on(HEAD);
    let skin = k.skin;
    if let Some(m) = &k.face_paint {
        let top = m.y.unwrap_or(27);
        mosaic(p, m, IVec3::new(-4, top, FACE), IVec3::X, IVec3::NEG_Y, IVec3::Z, 1);
    }
    // Eyes: [white, iris] per side, iris toward the nose.
    let iris = k.eyes.color.rgb();
    let pupil = mul(iris, 0.55);
    let white = WHITE;
    let brow = k.brows.or(k.hair.map(|h| h.color.rgb())).unwrap_or(mul(skin, 0.5));
    let covered = hide & acc::FACE == 0 && matches!(k.eyewear, Eyewear::Blindfold(_) | Eyewear::Bandage(_));
    match if covered { EyeStyle::Closed } else { k.eyes.style } {
        EyeStyle::Normal => {
            for s in [-1, 1] {
                p.decal(white).vb(s * 3, s * 3, 23, 24, FACE, FACE);
                p.decal(pupil).px(s * 2, 24, FACE);
                p.decal(iris).px(s * 2, 23, FACE);
            }
        }
        EyeStyle::Big => {
            for s in [-1, 1] {
                p.decal(pupil).vb(s * 2, s * 3, 24, 24, FACE, FACE);
                p.decal(iris).vb(s * 2, s * 3, 23, 23, FACE, FACE);
                p.decal(white).px(s * 3, 24, FACE);
                p.decal(mul(iris, 0.8)).vb(s * 2, s * 3, 22, 22, FACE, FACE);
            }
        }
        EyeStyle::Narrow | EyeStyle::Angry => {
            for s in [-1, 1] {
                p.decal(white).px(s * 3, 23, FACE);
                p.decal(iris).px(s * 2, 23, FACE);
            }
        }
        EyeStyle::Glow => {
            for s in [-1, 1] {
                p.decal(iris).vb(s * 2, s * 3, 23, 23, FACE, FACE);
                p.decal(mix(iris, WHITE, 0.6)).px(s * 2, 24, FACE);
                p.decal(iris).px(s * 3, 24, FACE);
            }
        }
        EyeStyle::Closed => {
            for s in [-1, 1] {
                p.decal(mul(skin, 0.45)).vb(s * 2, s * 3, 23, 23, FACE, FACE);
            }
        }
        EyeStyle::Dots => {
            for s in [-1, 1] {
                p.decal(BLACK).px(s * 2, 23, FACE);
            }
        }
    }
    // Brows.
    for s in [-1, 1] {
        if k.eyes.style == EyeStyle::Angry {
            p.decal(brow).px(s * 2, 24, FACE).px(s * 3, 25, FACE);
        } else {
            p.decal(brow).vb(s * 2, s * 3, 25, 25, FACE, FACE);
        }
    }
    if k.mouth {
        p.decal(mix(skin, [120, 40, 40], 0.45)).vb(-1, 1, 21, 21, FACE, FACE);
    }
    if hide & acc::FACE != 0 {
        return;
    }
    for f in &k.facial {
        match *f {
            Facial::Beard(c) => {
                let c = c.rgb();
                p.c(c).vb(-4, 4, 20, 22, -5, -3).vb(-3, 3, 19, 19, -5, -2).vbs(4, 4, 20, 24, -4, 1);
                p.decal(mul(c, 0.5)).vb(-1, 1, 21, 21, -5, -5);
            }
            Facial::LongBeard(c) => {
                let c = c.rgb();
                p.c(c).vb(-4, 4, 20, 22, -5, -3).vbs(4, 4, 20, 24, -4, 1).vb(-3, 3, 16, 19, -5, -3).vb(-2, 2, 13, 15, -5, -3);
                p.decal(mul(c, 0.5)).vb(-1, 1, 21, 21, -5, -5);
            }
            Facial::Goatee(c) => {
                p.c(c.rgb()).vb(-1, 1, 19, 20, -4, -3).vbs(1, 2, 22, 22, -5, -5);
            }
            Facial::Moustache(c) => {
                p.c(c.rgb()).vb(-2, 2, 22, 22, -5, -5);
            }
            Facial::BigMoustache(c) => {
                let c = c.rgb();
                p.c(c).vb(-3, 3, 21, 22, -5, -5).vbs(4, 4, 21, 21, -5, -4);
                // Mario's nose sits above it.
                p.c(mul(skin, 1.02)).vb(-1, 1, 23, 23, -5, -5).px(0, 22, -6);
            }
            Facial::Stubble(c) => {
                p.decal(mix(skin, c.rgb(), 0.45)).vb(-4, 4, 20, 22, FACE, FACE).vb(-1, 1, 21, 21, FACE, FACE);
                p.decal(mix(skin, [120, 40, 40], 0.45)).vb(-1, 1, 21, 21, FACE, FACE);
            }
            Facial::Scar(c, side) => {
                let s = if side == Side::Left { -1 } else { 1 };
                p.decal(c.rgb()).px(s * 3, 22, FACE).px(s * 2, 22, FACE);
            }
            Facial::EyeScar(c, side) => {
                let s = if side == Side::Left { -1 } else { 1 };
                p.decal(c.rgb()).vb(s * 3, s * 3, 22, 26, FACE, FACE);
            }
            Facial::Whiskers(c) => {
                for s in [-1, 1] {
                    p.decal(c.rgb()).vb(s * 3, s * 4, 22, 22, FACE, FACE).px(s * 4, 21, FACE).px(s * 3, 21, FACE);
                }
            }
            Facial::CheekMarks(c) => {
                for s in [-1, 1] {
                    p.decal(c.rgb()).px(s * 4, 22, FACE).px(s * 3, 22, FACE);
                }
            }
            Facial::ForeheadMark(c) => {
                p.decal(c.rgb()).px(0, 26, FACE);
            }
            Facial::Stripe(c, side) => {
                let s = if side == Side::Left { -1 } else { 1 };
                p.decal(c.rgb()).vb(s * 3, s * 3, 20, 27, FACE, FACE).vb(s * 4, s * 4, 21, 27, FACE, 2);
            }
            Facial::Blush(c) => {
                for s in [-1, 1] {
                    p.decal(c.rgb()).px(s * 4, 22, FACE);
                }
            }
            Facial::Fangs => {
                p.decal(WHITE).px(-1, 21, FACE).px(1, 21, FACE);
            }
            Facial::Lipstick(c) => {
                p.decal(c.rgb()).vb(-1, 1, 21, 21, FACE, FACE);
            }
        }
    }
}

// ------------------------------------------------------------------------------------ hair

/// Hair shell: top layer, painted skull top/back/sides, optional outer volume.
fn cap(p: &mut Pn, c: Rgb, side_low: i32, back_low: i32, volume: bool) {
    p.c(c);
    p.vb(-4, 4, 28, 28, -4, 3);
    p.vb(-4, 4, 27, 27, FACE, 3);
    p.vb(-4, 4, back_low, 27, 3, 3);
    p.vbs(4, 4, side_low, 27, -3, 3);
    if volume {
        p.vb(-4, 4, back_low.max(21), 28, 4, 4);
        p.vbs(5, 5, (side_low + 1).max(22), 27, -3, 3);
        p.vb(-3, 3, 29, 29, -3, 2);
    }
}

fn fringe(p: &mut Pn, c: Rgb, f: Fringe, volume: bool) {
    p.c(c);
    match f {
        Fringe::Full => {
            p.vb(-4, 4, 26, 27, FACE, FACE);
            if volume {
                p.vb(-4, 4, 26, 27, -5, -5);
            }
        }
        Fringe::Parted => {
            p.vb(-4, 4, 27, 27, FACE, FACE).vbs(2, 4, 26, 26, FACE, FACE);
            if volume {
                p.vbs(1, 4, 27, 27, -5, -5).vbs(3, 4, 26, 26, -5, -5);
            }
        }
        Fringe::Spiky => {
            p.vb(-4, 4, 26, 27, FACE, FACE);
            for x in [-3, 0, 3] {
                p.vb(x, x, 25, 25, FACE, FACE);
            }
            if volume {
                p.vb(-4, 4, 27, 27, -5, -5);
                for x in [-4, -1, 2] {
                    p.vb(x, x + 1, 25, 26, -5, -5);
                }
            }
        }
        Fringe::Side => {
            p.vb(-4, 4, 26, 27, FACE, FACE).vb(-4, -1, 25, 25, FACE, FACE).px(-4, 24, FACE);
            if volume {
                p.vb(-4, 3, 27, 27, -5, -5).vb(-4, -1, 25, 26, -5, -5);
            }
        }
        Fringe::Open => {
            p.vb(-4, 4, 27, 27, FACE, FACE);
        }
    }
}

/// Chunky hair clump: a thick block for the first half, then a cone to the tip (reads as an
/// anime spike at 1/16 m, where a plain cone thins to single voxels too early).
fn spike(p: &mut Pn, base: Vec3, tip: Vec3, d: f32) {
    let dir = (tip - base).normalize_or(Vec3::Y);
    let mid = base + (tip - base) * 0.5;
    p.seg(base, mid, d * 0.85, d * 0.7);
    p.cone_to(mid - dir * 0.5, tip, d * 0.9);
}

fn hair(p: &mut Pn, h: &Hair, hat_on: bool) {
    let c = h.color.rgb();
    let tip = h.tips.map_or(c, Col::rgb);
    // Under a hat the hair stays close to the head (the hat sits on it).
    let tall = !hat_on;
    use HairStyle::*;
    match h.style {
        Bald => {}
        Buzz => {
            let c2 = c;
            p.c(c2).vb(-4, 4, 27, 27, FACE, 3).vb(-4, 4, 23, 27, 3, 3).vbs(4, 4, 24, 27, -3, 3);
        }
        Short | Tufty | Swept => {
            cap(p, c, 24, 22, false);
            fringe(p, c, h.fringe.unwrap_or(if h.style == Swept { Fringe::Side } else { Fringe::Parted }), false);
            if h.style == Tufty && tall {
                p.c(c).vb(0, 0, 29, 30, -1, -1).px(1, 31, -1);
            }
        }
        Messy | Shaggy => {
            cap(p, c, if h.style == Shaggy { 21 } else { 23 }, 21, true);
            fringe(p, c, h.fringe.unwrap_or(Fringe::Spiky), true);
            if tall {
                p.c(tip);
                for (x, z) in [(-3, -2), (0, 1), (2, -2), (-1, 2), (3, 1)] {
                    p.px(x, 30, z);
                }
            }
        }
        SpikyUp => {
            cap(p, c, 24, 22, true);
            fringe(p, c, h.fringe.unwrap_or(Fringe::Spiky), true);
            if tall {
                p.c(c);
                // A tall upward mop: a core mass plus spikes leaning up/back.
                p.vb(-4, 4, 29, 30, -3, 3).vb(-3, 3, 31, 31, -2, 3);
                p.c(tip);
                let spikes = [
                    (v3(-3.0, 30.0, -2.0), v3(-4.5, 35.0, -1.0), 3.4),
                    (v3(0.0, 30.5, -2.5), v3(0.0, 36.0, -1.5), 3.6),
                    (v3(3.0, 30.0, -2.0), v3(4.5, 35.0, -1.0), 3.4),
                    (v3(-2.0, 30.5, 1.0), v3(-3.0, 35.5, 3.0), 3.6),
                    (v3(2.0, 30.5, 1.0), v3(3.0, 35.5, 3.0), 3.6),
                    (v3(0.0, 30.0, 2.5), v3(0.0, 34.0, 5.0), 3.6),
                    (v3(-4.0, 28.5, 0.0), v3(-6.5, 32.0, 0.5), 3.0),
                    (v3(4.0, 28.5, 0.0), v3(6.5, 32.0, 0.5), 3.0),
                ];
                for (a, b, d) in spikes {
                    spike(p, a, b, d);
                }
            }
        }
        SpikyWild => {
            cap(p, c, 23, 21, true);
            fringe(p, c, h.fringe.unwrap_or(Fringe::Spiky), true);
            p.c(tip);
            let mut spikes = vec![
                (v3(-4.5, 26.0, 0.0), v3(-7.5, 27.0, 0.5), 3.0),
                (v3(4.5, 26.0, 0.0), v3(7.5, 27.0, 0.5), 3.0),
                (v3(-4.5, 23.5, 1.0), v3(-6.5, 22.5, 2.0), 2.6),
                (v3(4.5, 23.5, 1.0), v3(6.5, 22.5, 2.0), 2.6),
                (v3(-2.5, 24.0, 4.0), v3(-3.5, 23.0, 7.0), 3.0),
                (v3(2.5, 24.0, 4.0), v3(3.5, 23.0, 7.0), 3.0),
                (v3(0.0, 27.0, 4.0), v3(0.0, 27.5, 7.5), 3.2),
            ];
            if tall {
                spikes.extend([
                    (v3(-3.0, 28.5, -2.0), v3(-4.5, 32.0, -2.5), 3.2),
                    (v3(0.0, 29.0, -2.0), v3(0.0, 32.5, -2.5), 3.2),
                    (v3(3.0, 28.5, -2.0), v3(4.5, 32.0, -2.5), 3.2),
                    (v3(-2.0, 29.0, 1.5), v3(-3.0, 32.0, 3.5), 3.4),
                    (v3(2.0, 29.0, 1.5), v3(3.0, 32.0, 3.5), 3.4),
                ]);
            }
            for (a, b, d) in spikes {
                spike(p, a, b, d);
            }
        }
        Saiyan => {
            cap(p, c, 22, 21, true);
            p.c(c).vb(-4, 4, 26, 27, FACE, FACE);
            // Forehead bangs pointing down.
            p.c(tip);
            for (x, l) in [(-3.0f32, 23.5f32), (0.0, 24.0), (2.5, 23.0)] {
                spike(p, v3(x, 27.5, -4.8), v3(x + 0.3, l, -5.6), 2.6);
            }
            if tall {
                let spikes = [
                    (v3(-3.0, 28.5, -1.0), v3(-8.0, 33.0, 0.0), 4.6),
                    (v3(-1.0, 29.0, 0.0), v3(-2.5, 35.5, 0.5), 4.6),
                    (v3(1.5, 29.0, 0.0), v3(3.0, 35.0, 1.0), 4.6),
                    (v3(3.0, 28.5, -1.0), v3(8.0, 32.5, 0.0), 4.4),
                    (v3(-2.5, 27.0, 3.0), v3(-6.0, 29.0, 6.5), 4.0),
                    (v3(2.5, 27.0, 3.0), v3(6.0, 29.0, 6.5), 4.0),
                    (v3(0.0, 25.0, 3.5), v3(0.0, 23.0, 7.0), 3.6),
                ];
                for (a, b, d) in spikes {
                    spike(p, a, b, d);
                }
            } else {
                p.c(c).vb(-5, 5, 27, 29, -3, 3);
            }
        }
        Flame => {
            cap(p, c, 24, 22, false);
            // Widow's peak.
            p.c(c).vb(-4, 4, 27, 27, FACE, FACE).vbs(2, 4, 26, 26, FACE, FACE).px(0, 26, FACE);
            if tall {
                p.c(c).vb(-4, 4, 28, 30, -3, 3);
                p.c(tip);
                spike(p, v3(0.0, 29.0, 0.0), v3(0.0, 38.0, 1.5), 7.5);
                spike(p, v3(-2.5, 29.0, 0.5), v3(-4.5, 35.0, 1.5), 4.2);
                spike(p, v3(2.5, 29.0, 0.5), v3(4.5, 35.0, 1.5), 4.2);
                spike(p, v3(0.0, 29.0, 2.5), v3(0.0, 35.0, 4.5), 4.2);
            }
        }
        SpikyBack => {
            cap(p, c, 22, 21, true);
            fringe(p, c, h.fringe.unwrap_or(Fringe::Parted), true);
            // Long side bangs framing the face.
            p.c(c).vbs(4, 4, 20, 26, -5, -5);
            p.c(tip);
            for (a, b, d) in [
                (v3(-2.5, 27.0, 3.0), v3(-4.0, 29.0, 7.5), 3.6),
                (v3(0.0, 28.0, 3.0), v3(0.0, 30.5, 8.0), 3.8),
                (v3(2.5, 27.0, 3.0), v3(4.0, 29.0, 7.5), 3.6),
                (v3(0.0, 25.0, 4.0), v3(0.0, 24.0, 7.5), 3.2),
            ] {
                spike(p, a, b, d);
            }
        }
        Long => {
            cap(p, c, 20, 19, true);
            fringe(p, c, h.fringe.unwrap_or(Fringe::Parted), true);
            // Hair down the back, behind the shoulders.
            p.c(c).vb(-4, 4, 12, 20, 4, 4).vb(-3, 3, 11, 11, 4, 4);
            p.c(c).vbs(4, 5, 19, 21, -3, 3);
        }
        Bob => {
            cap(p, c, 20, 20, true);
            fringe(p, c, h.fringe.unwrap_or(Fringe::Full), true);
        }
        Ponytail => {
            cap(p, c, 23, 22, false);
            fringe(p, c, h.fringe.unwrap_or(Fringe::Parted), false);
            p.c(mul(c, 0.8)).vb(-1, 1, 25, 26, 4, 4);
            p.c(tip).seg(v3(0.0, 25.0, 5.0), v3(0.0, 15.0, 6.0), 3.0, 2.4);
        }
        TwinTails => {
            cap(p, c, 23, 22, false);
            fringe(p, c, h.fringe.unwrap_or(Fringe::Full), false);
            p.c(tip);
            for s in [-1.0f32, 1.0] {
                p.seg(v3(s * 5.0, 26.5, 1.0), v3(s * 7.0, 14.0, 2.0), 3.0, 3.0);
            }
        }
        Bun => {
            cap(p, c, 23, 22, false);
            fringe(p, c, h.fringe.unwrap_or(Fringe::Parted), false);
            if tall {
                p.c(tip).el(v3(0.0, 30.0, 1.5), v3(5.0, 4.0, 5.0));
            }
        }
        SideBuns => {
            cap(p, c, 23, 22, false);
            fringe(p, c, h.fringe.unwrap_or(Fringe::Parted), false);
            p.c(tip);
            for s in [-1.0f32, 1.0] {
                p.el(v3(s * 5.5, 25.0, 0.5), v3(3.0, 4.0, 4.0));
            }
        }
        Afro => {
            p.c(c).el(v3(0.0, 27.0, 1.5), v3(15.0, 13.0, 13.0));
            fringe(p, c, Fringe::Open, false);
        }
        Mohawk => {
            p.c(mix(c, [60, 50, 45], 0.5)).vb(-4, 4, 27, 27, FACE, 3).vbs(4, 4, 24, 27, -3, 3).vb(-4, 4, 23, 27, 3, 3);
            p.c(tip).vb(-1, 1, 28, 31, -4, 4).vb(-1, 1, 25, 30, 4, 4);
        }
        Slicked => {
            cap(p, c, 24, 21, false);
            fringe(p, c, Fringe::Open, false);
            p.c(c).vb(-3, 3, 26, 28, 4, 4).vb(-4, 4, 26, 26, FACE, FACE);
        }
        Topknot => {
            cap(p, c, 24, 22, false);
            fringe(p, c, Fringe::Open, false);
            p.c(tip).el(v3(0.0, 30.0, 0.5), v3(3.0, 3.0, 3.0)).seg(v3(0.0, 30.0, 1.0), v3(0.0, 27.0, 5.0), 1.6, 1.6);
        }
    }
}

// ------------------------------------------------------------------------------------ eyewear

fn eyewear(p: &mut Pn, e: Eyewear) {
    match e {
        Eyewear::No => {}
        Eyewear::Blindfold(c) => {
            let c = c.rgb();
            p.c(c).vb(-4, 4, 23, 25, -5, -5).vbs(5, 5, 23, 25, -4, 3).vb(-4, 4, 23, 25, 4, 4);
            p.decal(shade(c)).vb(-4, 4, 24, 24, -5, -5);
        }
        Eyewear::RoundShades(c) => {
            let c = c.rgb();
            p.c(c).vb(-3, -2, 23, 24, -5, -5).vb(2, 3, 23, 24, -5, -5).vb(-1, 1, 24, 24, -5, -5);
            p.c(mul(c, 0.8)).vbs(4, 5, 24, 24, -5, -2);
            p.decal(mix(c, WHITE, 0.45)).px(-3, 24, -5).px(2, 24, -5);
        }
        Eyewear::Shades(c) => {
            let c = c.rgb();
            p.c(c).vb(-4, 4, 23, 24, -5, -5).vbs(5, 5, 24, 24, -4, 0);
            p.decal(mix(c, WHITE, 0.35)).px(-3, 24, -5).px(2, 24, -5);
        }
        Eyewear::Glasses(c) => {
            let c = c.rgb();
            for s in [-1, 1] {
                p.c(c).vb(s * 2, s * 3, 25, 25, -5, -5).vb(s * 2, s * 3, 22, 22, -5, -5).vb(s * 4, s * 4, 23, 24, -5, -5).vb(s, s, 23, 24, -5, -5);
            }
            p.c(c).px(0, 24, -5).vbs(5, 5, 24, 24, -4, 0);
        }
        Eyewear::Visor(c) => {
            let c = c.rgb();
            p.c(c).vb(-4, 4, 23, 24, -5, -5).vbs(5, 5, 23, 24, -4, 1);
            p.decal(mix(c, WHITE, 0.5)).vb(-3, 3, 24, 24, -5, -5);
        }
        Eyewear::Goggles(f, l) => {
            let (f, l) = (f.rgb(), l.rgb());
            p.c(f).vb(-4, 4, 22, 25, -5, -5).vbs(5, 5, 23, 24, -4, 3).vb(-4, 4, 23, 24, 4, 4);
            p.c(l).vb(-3, -2, 23, 24, -6, -6).vb(2, 3, 23, 24, -6, -6);
        }
        Eyewear::Eyepatch(c, side) => {
            let s = if side == Side::Left { -1 } else { 1 };
            let c = c.rgb();
            p.c(c).vb(s * 2, s * 3, 22, 24, -5, -5);
            p.decal(c).vb(-4, 4, 25, 25, FACE, FACE).vbs(4, 4, 25, 25, -3, 3);
        }
        Eyewear::Bandage(c) => {
            let c = c.rgb();
            p.c(c).vb(-4, 4, 25, 26, -5, -5).vbs(5, 5, 25, 26, -4, 3).vb(-4, 4, 25, 26, 4, 4);
        }
    }
}

// ------------------------------------------------------------------------------------ masks

fn whole_head(p: &mut Pn, c: Rgb) {
    p.c(c).vb(-4, 4, 21, 28, FACE, 3).vb(-3, 3, 20, 20, -3, 2).vb(-3, 3, 29, 29, -3, 2).vb(-1, 1, 19, 19, -2, 0);
}

fn mask(p: &mut Pn, m: Mask, skin: Rgb, eyes: Eyes) {
    let pal = |map: &[(char, Rgb)]| {
        let map = map.to_vec();
        move |ch: char| map.iter().find(|(k, _)| *k == ch).map(|(_, c)| *c)
    };
    match m {
        Mask::No => {}
        Mask::Spider(base, lines) => {
            let (b, l) = (base.rgb(), lines.rgb());
            whole_head(p, b);
            let f = pal(&[('L', l), ('K', BLACK), ('W', WHITE)]);
            let rows = ["....L....", "L...L...L", "KKK.L.KKK", "KWWKLKWWK", ".KWWLWWK.", "L.KK.KK.L", "...LLL...", "....L...."];
            p.mosaic(&rows, &f, IVec3::new(-4, 27, FACE), IVec3::X, IVec3::NEG_Y, IVec3::Z, 1);
            // Web lines around the head.
            p.decal(l).vb(-4, 4, 25, 25, -3, 3).vb(0, 0, 21, 28, 3, 3).vbs(4, 4, 21, 28, 0, 0).vb(-4, 4, 28, 28, 0, 0).vb(0, 0, 28, 28, -4, 3);
        }
        Mask::Full(base, patch, lens) => {
            let (b, pc, lc) = (base.rgb(), patch.rgb(), lens.rgb());
            whole_head(p, b);
            let f = pal(&[('P', pc), ('W', lc)]);
            let rows = [".........", ".........", ".PPP.PPP.", "PPWP.PWPP", ".PPP.PPP.", ".........", ".........", "........."];
            p.mosaic(&rows, &f, IVec3::new(-4, 27, FACE), IVec3::X, IVec3::NEG_Y, IVec3::Z, 1);
        }
        Mask::Cowl(c, ears) => {
            let c = c.rgb();
            p.c(c).vb(-4, 4, 22, 28, FACE, 3).vb(-4, 4, 19, 21, 1, 3).vbs(4, 4, 19, 21, -2, 3).vb(-3, 3, 29, 29, -3, 2);
            p.c(c).vb(-4, 4, 18, 19, -3, 2);
            // Lower face stays skin (the mouth shows).
            p.c(skin).vb(-3, 3, 20, 21, -4, -4);
            p.decal(mix(skin, [120, 40, 40], 0.45)).vb(-1, 1, 21, 21, FACE, FACE);
            p.decal(WHITE).vb(-3, -2, 24, 24, FACE, FACE).vb(2, 3, 24, 24, FACE, FACE);
            p.decal(shade(c)).px(0, 23, FACE).px(0, 24, FACE);
            if ears {
                p.c(c);
                for s in [-1.0f32, 1.0] {
                    p.cone_to(v3(s * 3.0, 28.5, -0.5), v3(s * 3.4, 33.0, -0.5), 2.6);
                }
            }
        }
        Mask::Flared(c, flare) => {
            let (c, fl) = (c.rgb(), flare.rgb());
            p.c(c).vb(-4, 4, 22, 28, FACE, 3).vb(-3, 3, 29, 29, -3, 2);
            p.c(skin).vb(-3, 3, 20, 21, -4, -4);
            p.decal(mix(skin, [120, 40, 40], 0.45)).vb(-1, 1, 21, 21, FACE, FACE);
            p.decal(fl).vb(-4, -1, 23, 25, FACE, FACE).vb(1, 4, 23, 25, FACE, FACE);
            p.decal(WHITE).vb(-3, -2, 24, 24, FACE, FACE).vb(2, 3, 24, 24, FACE, FACE);
            p.c(fl);
            for s in [-1.0f32, 1.0] {
                p.cone_to(v3(s * 4.0, 25.0, -2.0), v3(s * 6.0, 31.0, 1.0), 2.6);
            }
        }
        Mask::Ninja(c) => {
            let c = c.rgb();
            p.c(c).vb(-4, 4, 20, 22, -5, -5).vbs(4, 5, 20, 22, -4, 3).vb(-4, 4, 20, 22, 4, 4).vb(-3, 3, 19, 19, -4, 2);
            p.decal(shade(c)).px(0, 22, -5);
        }
        Mask::Bandana(c) => {
            let c = c.rgb();
            p.c(c).vb(-4, 4, 23, 24, -5, -5).vbs(5, 5, 23, 24, -4, 3).vb(-4, 4, 23, 24, 4, 4);
            p.decal(WHITE).vb(-3, -2, 23, 23, -5, -5).vb(2, 3, 23, 23, -5, -5);
            p.c(c).seg(v3(-0.5, 23.5, 5.0), v3(-2.5, 18.0, 7.0), 1.4, 1.0).seg(v3(0.5, 23.5, 5.0), v3(2.5, 18.5, 6.5), 1.4, 1.0);
        }
        Mask::Oni(c) => {
            let c = c.rgb();
            p.c(c).vb(-4, 4, 20, 26, -5, -5).vb(-3, 3, 27, 27, -5, -5);
            p.decal(BLACK).vb(-3, -2, 24, 24, -5, -5).vb(2, 3, 24, 24, -5, -5).vb(-2, 2, 21, 21, -5, -5);
            p.decal(WHITE).px(-2, 21, -5).px(2, 21, -5);
            p.c([240, 230, 200]);
            for s in [-1.0f32, 1.0] {
                p.cone_to(v3(s * 3.0, 27.0, -4.5), v3(s * 4.5, 31.0, -4.0), 2.2);
            }
        }
        Mask::Hockey(c) => {
            let c = c.rgb();
            p.c(c).vb(-3, 3, 20, 27, -5, -5).vbs(4, 4, 21, 26, -5, -5);
            p.decal(BLACK).vb(-3, -2, 24, 24, -5, -5).vb(2, 3, 24, 24, -5, -5).px(-1, 21, -5).px(1, 21, -5).px(0, 22, -5);
            p.decal([200, 30, 30]).px(-2, 26, -5).px(-3, 25, -5).px(2, 26, -5).px(3, 25, -5);
        }
        Mask::Skull(c) => {
            let c = c.rgb();
            let f = pal(&[('W', c), ('K', BLACK)]);
            let rows = [".WWWWWWW.", "WWWWWWWWW", "WKKWWWKKW", "WKKWWWKKW", "WWWWKWWWW", "WWWWWWWWW", ".WKWKWKW.", "..WWWWW.."];
            p.mosaic(&rows, &f, IVec3::new(-4, 27, FACE), IVec3::X, IVec3::NEG_Y, IVec3::Z, 1);
        }
        Mask::Domino(c) => {
            let c = c.rgb();
            p.c(c).vb(-4, 4, 23, 25, -5, -5);
            p.decal(WHITE).vb(-3, -2, 24, 24, -5, -5).vb(2, 3, 24, 24, -5, -5);
            let _ = eyes;
        }
        Mask::Respirator(c) => {
            let c = c.rgb();
            p.c(c).vb(-2, 2, 20, 22, -5, -5).vb(-1, 1, 20, 21, -6, -6).vbs(3, 4, 21, 21, -5, -4);
            p.decal(shade(c)).px(0, 21, -6);
        }
    }
}

// ------------------------------------------------------------------------------------ helmets

/// Closed helmet shell: 11 × 10 × 10 around the skull with a rounded top.
fn shell(p: &mut Pn, c: Rgb) {
    p.c(c).vb(-5, 5, 20, 28, -5, 4).vb(-4, 4, 29, 29, -4, 3).vb(-4, 4, 19, 19, -4, 3);
}

fn helmet(p: &mut Pn, h: Helmet) {
    match h {
        Helmet::No => {}
        Helmet::Vader(c) => {
            let c = c.rgb();
            let hi = mix(c, [150, 160, 185], 0.4);
            // Dome + flared skirt down to the shoulders (the triangle silhouette).
            p.c(c).vb(-5, 5, 21, 28, -5, 4).vb(-4, 4, 29, 29, -4, 3).vb(-3, 3, 30, 30, -3, 2);
            p.c(c).vb(-6, 6, 20, 23, -3, 5).vb(-7, 7, 19, 20, -2, 6);
            p.decal(hi).vb(-7, 7, 19, 19, -2, -2).vbs(6, 6, 21, 23, -3, -3);
            // Face mask: brow ridge, big dark lenses, nose ridge, triangular grille.
            p.c(c).vb(-5, 5, 25, 25, -6, -6).vb(-5, 5, 19, 24, -5, -5);
            p.decal([58, 64, 76]).vb(-4, -1, 23, 24, -5, -5).vb(1, 4, 23, 24, -5, -5);
            p.decal([150, 158, 172]).px(-3, 24, -5).px(2, 24, -5);
            p.c(c).vb(0, 0, 21, 24, -6, -6);
            let g = [150, 154, 162];
            p.c(g).vb(-1, 1, 21, 22, -6, -6).vb(-2, 2, 19, 20, -6, -6);
            p.decal([60, 62, 70]).px(-1, 19, -6).px(1, 19, -6).px(0, 20, -6);
            p.decal(hi).vb(-3, 3, 28, 28, -4, -4).px(-4, 27, -5);
        }
        Helmet::Trooper(c, d) => {
            let (c, d) = (c.rgb(), d.rgb());
            shell(p, c);
            p.c(c).vb(-5, 5, 25, 25, -6, -6);
            let f = |ch: char| match ch {
                'K' => Some(d),
                'B' => Some([60, 90, 170]),
                'G' => Some([110, 116, 124]),
                _ => None,
            };
            let rows = ["...........", "...........", "...........", ".KKK...KKK.", "..KK...KK..", "B....K....B", "...GKKKG...", "..G.G.G.G..", "..........."];
            p.mosaic(&rows, &f, IVec3::new(-5, 28, -5), IVec3::X, IVec3::NEG_Y, IVec3::Z, 1);
            p.c(c).vb(-2, 2, 20, 21, -6, -6);
            p.decal(d).px(-1, 20, -6).px(1, 20, -6);
        }
        Helmet::Mando(c, v) | Helmet::Boba(c, v, _) => {
            let (c, v) = (c.rgb(), v.rgb());
            shell(p, c);
            p.decal(mix(c, WHITE, 0.25)).vb(-3, 3, 28, 28, -5, -5).vb(-4, 4, 29, 29, -3, -3);
            // The T visor.
            p.decal(v).vb(-4, 4, 24, 24, -5, -5).vb(-1, 1, 20, 23, -5, -5);
            p.decal(shade(c)).px(-2, 21, -5).px(2, 21, -5);
            p.c(mul(c, 0.8)).vbs(6, 6, 22, 24, -2, 0);
            if let Helmet::Boba(_, _, t) = h {
                let t = t.rgb();
                p.decal(t).vb(-3, 3, 27, 27, -5, -5).vbs(3, 4, 25, 26, -5, -5);
                p.c([150, 150, 150]).rod(v3(6.0, 24.0, -1.0), v3(6.0, 31.0, -1.0), 1.0).vb(5, 6, 30, 31, -3, -1);
                p.decal(mul(c, 0.6)).px(-3, 27, -5).px(3, 23, -5);
            }
        }
        Helmet::Chief(c, v) | Helmet::Doom(c, v) => {
            let (c, v) = (c.rgb(), v.rgb());
            shell(p, c);
            p.c(c).vb(-2, 2, 30, 30, -3, 1);
            // Big wraparound visor.
            let (y0, y1) = if matches!(h, Helmet::Doom(..)) { (22, 24) } else { (22, 25) };
            p.c(v).vb(-4, 4, y0, y1, -6, -6).vbs(5, 5, y0, y1, -5, -3);
            p.decal(mix(v, WHITE, 0.45)).vb(-3, 0, y1, y1, -6, -6).px(-3, y1 - 1, -6);
            // Chin / mouth vent.
            p.c(mul(c, 0.8)).vb(-2, 2, 19, 21, -6, -6);
            p.decal([60, 62, 66]).vb(-1, 1, 20, 20, -6, -6);
            p.c([80, 82, 88]).vbs(6, 6, 22, 24, -2, 1);
            if matches!(h, Helmet::Doom(..)) {
                p.c(c).vb(-1, 1, 26, 30, -6, -6);
            }
        }
        Helmet::IronMan(c, face, eye) => {
            let (c, fc, e) = (c.rgb(), face.rgb(), eye.rgb());
            shell(p, c);
            let f = |ch: char| match ch {
                'F' => Some(fc),
                'E' => Some(e),
                'D' => Some(mul(fc, 0.55)),
                _ => None,
            };
            let rows = ["...........", "...........", "..FFFFFFF..", ".FFFFFFFFF.", ".FEEFFFEEF.", ".FFFFFFFFF.", "..FFFFFFF..", "..FFDDDFF..", "...FFFFF...", "....FFF...."];
            p.mosaic(&rows, &f, IVec3::new(-5, 28, -5), IVec3::X, IVec3::NEG_Y, IVec3::Z, 1);
            p.c(fc).vb(-2, 2, 19, 20, -6, -6);
        }
        Helmet::Knight(c, plume) => {
            let c = c.rgb();
            shell(p, c);
            p.c(c).vb(0, 0, 19, 27, -6, -6);
            p.decal([30, 30, 34]).vb(-4, -1, 24, 24, -5, -5).vb(1, 4, 24, 24, -5, -5);
            p.decal([60, 60, 66]).px(-2, 21, -5).px(2, 21, -5).px(-2, 22, -5).px(2, 22, -5);
            if let Some(pl) = plume {
                p.c(pl.rgb());
                for i in 0..4 {
                    let f = i as f32;
                    p.el(v3(0.0, 30.5 + f * 0.5, -2.0 + f * 1.8), v3(2.0, 3.0, 3.0));
                }
            }
        }
        Helmet::Space(c, v) => {
            let (c, v) = (c.rgb(), v.rgb());
            p.c(c).vb(-5, 5, 19, 28, -5, 5).vb(-4, 4, 29, 29, -4, 4).vb(-6, 6, 18, 19, -4, 4);
            p.c(v).vb(-4, 4, 21, 26, -6, -6).vb(-3, 3, 27, 27, -6, -6);
            p.decal(mix(v, WHITE, 0.55)).vb(-3, -2, 25, 26, -6, -6);
        }
        Helmet::Robot(c, e) => {
            let (c, e) = (c.rgb(), e.rgb());
            p.c(c).vb(-5, 5, 20, 29, -5, 4);
            p.c(e).vb(-4, 4, 24, 24, -6, -6);
            p.decal(mul(c, 0.7)).vb(-3, 3, 21, 21, -5, -5);
            p.c(mul(c, 0.8)).rod(v3(0.0, 29.5, 0.0), v3(0.0, 33.0, 0.0), 1.0).px(0, 34, 0);
        }
        Helmet::Samus(c, v) => {
            let (c, v) = (c.rgb(), v.rgb());
            shell(p, c);
            p.c(v).vb(-3, 3, 22, 25, -6, -6).vb(-4, 4, 23, 24, -6, -6);
            p.decal(mix(v, WHITE, 0.5)).vb(-3, -1, 25, 25, -6, -6);
            p.c(mix(c, [230, 200, 60], 0.6)).vbs(6, 6, 22, 24, -2, 0).vb(-1, 1, 28, 29, -6, -5);
        }
        Helmet::Horned(c, horn) => {
            let (c, hc) = (c.rgb(), horn.rgb());
            p.c(c).vb(-5, 5, 26, 28, -5, 4).vb(-4, 4, 29, 29, -4, 3).vbs(5, 5, 20, 25, -3, 4).vb(-4, 4, 20, 25, 4, 4);
            p.c(c).vb(0, 0, 21, 25, -5, -5);
            p.c(hc);
            for s in [-1.0f32, 1.0] {
                p.cone_to(v3(s * 5.0, 27.0, -1.0), v3(s * 7.5, 33.0, 0.5), 2.8);
            }
        }
        Helmet::Crested(c, crest) => {
            let (c, cr) = (c.rgb(), crest.rgb());
            p.c(c).vb(-5, 5, 20, 28, -5, 4).vb(-4, 4, 29, 29, -4, 3);
            // T-shaped face opening (eyes + mouth) shows the face under it.
            p.decal([40, 30, 26]).vb(-4, 4, 23, 24, -5, -5).vb(-1, 1, 19, 22, -5, -5);
            p.c(cr).vb(0, 0, 29, 33, -5, 5).vb(0, 0, 30, 32, 6, 6);
        }
        Helmet::Slit(c, s) => {
            let (c, s) = (c.rgb(), s.rgb());
            shell(p, c);
            p.decal(s).vb(-3, 3, 24, 24, -5, -5).vb(0, 0, 20, 23, -5, -5);
            p.decal(mix(c, WHITE, 0.3)).vb(-4, 4, 25, 25, -5, -5);
        }
        Helmet::Chainsaw(c, blade) => {
            let (c, bl) = (c.rgb(), blade.rgb());
            p.c(c).vb(-5, 5, 20, 28, -5, 4);
            p.decal(WHITE).vb(-3, -2, 23, 23, -5, -5).vb(2, 3, 23, 23, -5, -5);
            p.decal(BLACK).vb(-3, 3, 21, 21, -5, -5);
            p.decal(WHITE).px(-2, 21, -5).px(0, 21, -5).px(2, 21, -5);
            p.c(bl).vb(-1, 1, 25, 29, -11, -5).vb(0, 0, 30, 30, -10, -6);
            p.decal(shade(bl)).vb(-1, 1, 25, 25, -11, -5);
        }
        Helmet::Prime(c, face, eye) => {
            let (c, fc, e) = (c.rgb(), face.rgb(), eye.rgb());
            shell(p, c);
            p.c(c).vbs(6, 6, 23, 30, -1, 1);
            p.c(c).vb(0, 0, 28, 29, -6, -6);
            p.c(fc).vb(-3, 3, 19, 22, -6, -6);
            p.decal(shade(fc)).vb(-2, 2, 20, 20, -6, -6).vb(-2, 2, 21, 21, -6, -6);
            p.decal(e).vb(-3, -2, 24, 24, -5, -5).vb(2, 3, 24, 24, -5, -5);
            p.decal(fc).vb(-4, 4, 23, 23, -5, -5).vb(-1, 1, 24, 26, -5, -5);
        }
        Helmet::Mega(c, st) => {
            let (c, st) = (c.rgb(), st.rgb());
            p.c(c).vb(-5, 5, 26, 28, -5, 4).vb(-4, 4, 29, 29, -4, 3).vbs(5, 5, 20, 25, -3, 4).vb(-4, 4, 20, 25, 4, 4);
            p.c(st).vb(-1, 1, 26, 29, -6, -5);
            p.c(st).vbs(6, 6, 22, 24, -2, 0);
        }
        Helmet::Smooth(c, v) => {
            let (c, v) = (c.rgb(), v.rgb());
            p.c(c).vb(-5, 5, 20, 28, -5, 4).vb(-4, 4, 29, 29, -4, 3);
            p.c(v).vb(-4, 4, 22, 25, -6, -6);
        }
    }
}

// ------------------------------------------------------------------------------------ hats

fn ring(p: &mut Pn, y0: i32, y1: i32) {
    p.vb(-4, 4, y0, y1, -5, -5).vbs(5, 5, y0, y1, -4, 3).vb(-4, 4, y0, y1, 4, 4);
}

fn hat(p: &mut Pn, h: Hat, tall_hair: bool) {
    let lift = if tall_hair { 1 } else { 0 };
    match h {
        Hat::No => {}
        Hat::Cap(c, badge) => {
            let c = c.rgb();
            p.c(c).vb(-5, 5, 26, 29, -5, 4).vb(-4, 4, 30, 30, -4, 3);
            p.c(c).vb(-4, 4, 26, 26, -8, -6).vb(-3, 3, 26, 26, -9, -9);
            if let Some(b) = badge {
                p.c(WHITE).vb(-1, 1, 27, 29, -6, -6).vbs(2, 2, 28, 28, -6, -6);
                p.decal(b.rgb()).vbs(1, 1, 27, 29, -6, -6).px(0, 28, -6);
            }
        }
        Hat::Pointy(c) => {
            let c = c.rgb();
            p.c(c).vb(-5, 5, 26, 28, -4, 4).vb(-4, 4, 29, 29, -4, 3);
            p.c(c).cone_to(v3(0.0, 28.0, 1.5), v3(0.0, 21.0, 9.0), 9.0);
        }
        Hat::Straw(c, band) => {
            let (c, bd) = (c.rgb(), band.rgb());
            p.c(c).cy(v3(0.0, 28.0 + lift as f32, -0.5), 13.0, 1.0);
            p.c(c).vb(-4, 4, 29 + lift, 31 + lift, -4, 3).vb(-3, 3, 32 + lift, 32 + lift, -3, 2);
            p.c(bd).vb(-4, 4, 29 + lift, 29 + lift, -5, 4).vbs(5, 5, 29 + lift, 29 + lift, -4, 3);
        }
        Hat::Wizard(c, band) => {
            let (c, bd) = (c.rgb(), band.rgb());
            p.c(c).cy(v3(0.0, 28.0, -0.5), 13.0, 1.0);
            p.c(bd).vb(-4, 4, 29, 29, -5, 4);
            p.c(c).cone_to(v3(0.0, 29.0, -0.5), v3(0.0, 37.0, 2.5), 9.0);
            p.c(c).cone_to(v3(0.0, 36.0, 2.0), v3(0.0, 38.0, 5.5), 3.2);
        }
        Hat::Cowboy(c) => {
            let c = c.rgb();
            p.c(c).cy(v3(0.0, 28.0, -0.5), 13.0, 1.0).vbs(6, 6, 29, 29, -3, 2);
            p.c(c).vb(-4, 4, 29, 31, -4, 3).vb(-3, 3, 32, 32, -3, 2);
            p.c(mul(c, 0.7)).vb(-4, 4, 29, 29, -5, 4);
            p.decal(mul(c, 0.8)).vb(0, 0, 32, 32, -3, 2);
        }
        Hat::Fedora(c, band) => {
            let (c, bd) = (c.rgb(), band.rgb());
            p.c(c).cy(v3(0.0, 28.0, -0.5), 12.0, 1.0);
            p.c(c).vb(-4, 4, 29, 31, -4, 3);
            p.c(bd).vb(-4, 4, 29, 29, -5, 4).vbs(5, 5, 29, 29, -4, 3);
            p.decal(mul(c, 0.75)).vb(0, 0, 31, 31, -3, 2);
        }
        Hat::TopHat(c, band) => {
            let (c, bd) = (c.rgb(), band.rgb());
            p.c(c).cy(v3(0.0, 28.0, -0.5), 12.0, 1.0);
            p.c(c).vb(-4, 4, 29, 35, -4, 3);
            p.c(bd).vb(-4, 4, 29, 29, -5, 4).vbs(5, 5, 29, 29, -4, 3);
        }
        Hat::Crown(c) => {
            let c = c.rgb();
            p.c(c);
            ring(p, 28 + lift, 28 + lift);
            for (x, z) in [(-4, -5), (0, -5), (4, -5), (-5, 0), (5, 0), (-4, 4), (0, 4), (4, 4)] {
                p.vb(x, x, 29 + lift, 30 + lift, z, z);
            }
            p.decal([200, 30, 50]).px(0, 28 + lift, -5);
        }
        Hat::Beanie(c) => {
            let c = c.rgb();
            p.c(c).vb(-5, 5, 25, 29, -5, 4).vb(-4, 4, 30, 30, -4, 3);
            p.c(mul(c, 0.8)).vb(-5, 5, 25, 25, -5, 4);
        }
        Hat::Headband(c, plate) => {
            let c = c.rgb();
            p.c(c);
            ring(p, 25, 26);
            p.c(c).seg(v3(-0.5, 25.5, 5.0), v3(-2.0, 21.0, 7.0), 1.6, 1.0).seg(v3(0.5, 25.5, 5.0), v3(2.0, 21.5, 6.5), 1.6, 1.0);
            if let Some(pl) = plate {
                let pl = pl.rgb();
                p.c(pl).vb(-2, 2, 25, 26, -6, -6);
                p.decal(mul(pl, 0.55)).px(0, 26, -6).px(-1, 25, -6).px(1, 25, -6);
            }
        }
        Hat::Bandana(c) => {
            let c = c.rgb();
            p.c(c).vb(-5, 5, 26, 29, -5, 4).vb(-4, 4, 30, 30, -4, 3);
            p.c(c).seg(v3(0.0, 27.0, 5.0), v3(-1.5, 22.0, 7.0), 2.0, 1.0);
        }
        Hat::Horns(c) => {
            let c = c.rgb();
            p.c(c);
            for s in [-1.0f32, 1.0] {
                p.cone_to(v3(s * 3.0, 27.5, -1.0), v3(s * 5.0, 32.0, 1.0), 2.6);
            }
        }
        Hat::Halo(c) => {
            let c = c.rgb();
            p.c(c).vb(-3, 3, 32 + lift * 3, 32 + lift * 3, -4, -4).vb(-3, 3, 32 + lift * 3, 32 + lift * 3, 3, 3).vbs(4, 4, 32 + lift * 3, 32 + lift * 3, -3, 2);
        }
        Hat::CatEars(c) => {
            let c = c.rgb();
            p.c(c);
            for s in [-1.0f32, 1.0] {
                p.cone_to(v3(s * 3.0, 28.0, 0.0), v3(s * 3.8, 31.5, 0.0), 3.0);
            }
        }
        Hat::Hood(c) => {
            let c = c.rgb();
            p.c(c).vb(-5, 5, 28, 29, -5, 4).vbs(5, 5, 19, 27, -5, 4).vb(-5, 5, 19, 27, 4, 5).vb(-4, 4, 27, 27, -5, -5);
            p.c(mul(c, 0.85)).vb(-3, 3, 30, 30, -3, 3);
            p.decal(mul([244, 204, 170], 0.7)).vb(-4, 4, 26, 26, FACE, FACE);
        }
        Hat::Tiara(c) => {
            let c = c.rgb();
            p.c(c).vb(-4, 4, 26, 26, -5, -5).vbs(5, 5, 26, 26, -4, 0);
            p.c(c).vb(-1, 1, 27, 27, -5, -5).px(0, 28, -5);
            p.decal([220, 30, 40]).px(0, 27, -5);
        }
        Hat::Winged(c, w) => {
            let (c, w) = (c.rgb(), w.rgb());
            p.c(c).vb(-5, 5, 26, 29, -5, 4).vb(-4, 4, 30, 30, -4, 3);
            p.c(w);
            for s in [-1.0f32, 1.0] {
                p.seg(v3(s * 5.5, 27.0, 0.0), v3(s * 7.0, 32.0, 2.0), 1.0, 3.0);
            }
        }
        Hat::LokiHorns(c) => {
            let c = c.rgb();
            p.c(c);
            ring(p, 25, 26);
            p.c(c).vb(-5, 5, 27, 28, -5, -5).vb(-4, 4, 29, 29, -4, 3);
            for s in [-1.0f32, 1.0] {
                p.seg(v3(s * 2.0, 27.0, -5.5), v3(s * 3.0, 32.0, -5.0), 1.4, 1.4);
                p.cone_to(v3(s * 3.0, 32.0, -5.0), v3(s * 6.0, 37.0, -1.0), 2.4);
            }
        }
        Hat::Beret(c) => {
            let c = c.rgb();
            p.c(c).el(v3(1.0, 28.8, -0.5), v3(12.0, 3.0, 11.0));
        }
        Hat::Bowler(c) => {
            let c = c.rgb();
            p.c(c).cy(v3(0.0, 28.0, -0.5), 11.0, 1.0).el(v3(0.0, 30.0, -0.5), v3(9.0, 6.0, 9.0));
        }
        Hat::Bucket(c) => {
            let c = c.rgb();
            p.c(c).cy(v3(0.0, 27.5, -0.5), 12.0, 1.0).vb(-4, 4, 28, 30, -4, 3);
        }
        Hat::Headphones(c) => {
            let c = c.rgb();
            p.c(c).vb(-5, 5, 29, 29, -1, 0).vbs(5, 5, 25, 28, -1, 0).vbs(6, 6, 22, 25, -2, 1);
        }
        Hat::Tricorn(c, trim) => {
            let (c, tr) = (c.rgb(), trim.rgb());
            p.c(c).vb(-4, 4, 28, 30, -4, 3);
            p.c(c).vb(-6, 6, 28, 29, -2, 2).vb(-3, 3, 28, 29, -7, -5).vb(-4, 4, 28, 29, 4, 5);
            p.c(tr).vb(-6, 6, 30, 30, -2, -2).vb(-3, 3, 30, 30, -7, -7);
        }
        Hat::Ears(c) => {
            let c = c.rgb();
            p.c(c);
            for s in [-1.0f32, 1.0] {
                p.seg(v3(s * 5.0, 24.0, 0.0), v3(s * 7.5, 26.5, 0.5), 2.0, 1.2);
            }
        }
        Hat::Tied(c) => {
            let c = c.rgb();
            p.c(c);
            ring(p, 26, 26);
            p.c(c).seg(v3(-0.5, 26.0, 5.0), v3(-2.5, 20.0, 7.5), 1.6, 1.0).seg(v3(0.5, 26.0, 5.0), v3(2.5, 20.5, 7.0), 1.6, 1.0);
        }
    }
}

// ------------------------------------------------------------------------------------ body

fn arm(p: &mut Pn, s: i32, y0: i32, y1: i32) {
    p.vb(s * 4, s * 6, y0, y1, -2, 0);
}
fn leg(p: &mut Pn, s: i32, y0: i32, y1: i32) {
    p.vb(s, s * 3, y0, y1, -2, 1);
}
fn torso(p: &mut Pn, y0: i32, y1: i32) {
    p.vb(-3, 3, y0, y1, -2, 1);
}

fn body(p: &mut Pn, k: &Kit, hide: u32) {
    let t = k.top.color.rgb();
    let alt = k.top.alt.map(Col::rgb);
    let trim = k.top.trim.map(Col::rgb);
    let skin = k.skin;
    let bulky = matches!(k.top.cut, Cut::Plate | Cut::Power) || k.broad;
    let chest_z = if bulky { -3 } else { -2 };
    let back_z = if bulky { 2 } else { 1 };

    // --- torso
    p.on(0).c(t);
    torso(p, 10, 18);
    if bulky {
        p.c(t).vb(-3, 3, 11, 18, -3, 2);
    }
    // --- legs (under anything the top does to them)
    legs(p, k, hide);
    p.on(0);
    match k.top.cut {
        Cut::Waistcoat => {}
        Cut::Bare => {
            p.c(skin);
            torso(p, 12, 18);
            p.decal(mul(skin, 0.86)).vb(-2, -1, 16, 16, -2, -2).vb(1, 2, 16, 16, -2, -2).px(0, 14, -2).px(0, 12, -2);
        }
        Cut::Tee => {
            p.decal(skin).vb(-1, 1, 18, 18, -2, -2);
            if let Some(tr) = trim {
                p.decal(tr).vb(-2, 2, 18, 18, -2, -2).vb(-1, 1, 18, 18, -2, -2);
                p.decal(skin).px(0, 18, -2);
            }
        }
        Cut::Tank => {
            p.decal(skin).vb(-2, 2, 18, 18, -2, -2).vb(-1, 1, 17, 17, -2, -2).vb(-2, 2, 18, 18, 1, 1);
        }
        Cut::LongSleeve | Cut::Jumpsuit => {
            if let Some(tr) = trim {
                p.decal(tr).vb(0, 0, 11, 18, -2, -2);
            }
        }
        Cut::HighCollar => {
            p.c(t).vb(-3, 3, 19, 20, -4, -4).vb(-3, 3, 19, 20, 3, 3).vbs(4, 4, 19, 20, -4, 3).vb(-3, 3, 19, 19, -3, 2);
            if let Some(tr) = trim {
                p.c(tr).px(-1, 20, -5);
                p.decal(shade(t)).vb(0, 0, 11, 17, -2, -2);
            }
        }
        Cut::Gi | Cut::Kimono => {
            let under = alt.unwrap_or(skin);
            p.decal(under).vb(-2, 2, 18, 18, -2, -2).vb(-1, 1, 17, 17, -2, -2).px(0, 16, -2);
            if let Some(tr) = trim {
                p.decal(tr).px(-2, 17, -2).px(-1, 16, -2).px(0, 15, -2).px(2, 17, -2).px(1, 16, -2);
            }
        }
        Cut::Robe | Cut::Dress => {
            if let Some(tr) = trim {
                p.decal(tr).vb(0, 0, 10, 18, -2, -2).vb(-2, 2, 18, 18, -2, -2);
            }
        }
        Cut::Plate => {
            let a = alt.unwrap_or(shade(t));
            p.decal(a).vb(-3, 3, 14, 14, -3, -3).vb(-3, 3, 11, 11, -3, 2);
            p.decal(mix(t, WHITE, 0.3)).vb(-2, -1, 17, 17, -3, -3).vb(1, 2, 17, 17, -3, -3);
        }
        Cut::Power => {
            let a = alt.unwrap_or([60, 62, 68]);
            p.decal(a).vb(-3, 3, 11, 12, -3, 2).vb(-3, 3, 18, 18, -3, -3);
            p.decal(shade(t)).vb(0, 0, 13, 17, -3, -3).vb(-3, 3, 15, 15, -3, -3);
            p.c(t).vb(-2, 2, 16, 17, -4, -4);
        }
        Cut::Suit => {
            let shirt = alt.unwrap_or(WHITE);
            let tie = trim.unwrap_or([30, 30, 36]);
            p.decal(shirt).vb(-1, 1, 16, 18, -2, -2).vb(-2, 2, 18, 18, -2, -2);
            p.decal(tie).vb(0, 0, 13, 17, -2, -2);
            p.decal(shade(t)).px(-2, 17, -2).px(2, 17, -2).px(-1, 15, -2).px(1, 15, -2);
        }
        Cut::Hoodie => {
            let a = alt.unwrap_or(shade(t));
            p.decal(a).vb(-2, 2, 11, 13, -2, -2);
            p.decal(trim.unwrap_or(WHITE)).vb(-1, -1, 15, 17, -2, -2).vb(1, 1, 15, 17, -2, -2);
            p.on(HEAD).c(t).vb(-4, 4, 19, 22, 4, 4).vb(-3, 3, 23, 23, 4, 4).vbs(5, 5, 19, 20, 0, 4);
            p.on(0);
        }
        Cut::Coat | Cut::Haori => {
            let inner = alt.unwrap_or(shade(t));
            if k.top.cut == Cut::Coat {
                p.decal(inner).vb(-1, 1, 10, 18, -2, -2);
                p.c(t).vb(-3, 3, 19, 19, -3, 2).vb(-3, 3, 20, 20, 2, 2).vbs(3, 3, 20, 20, -3, 2);
            }
            let long = if k.top.cut == Cut::Coat { 3 } else { 5 };
            for (id, s) in [(LEG_L, -1), (LEG_R, 1)] {
                p.on(id).c(t).vb(s, s * 3, long, 9, -3, 2);
                if k.top.cut == Cut::Coat {
                    p.decal(inner).vb(s, s, long, 9, -3, -3);
                }
            }
            p.on(0);
            if k.top.cut == Cut::Haori
                && let Some(a) = alt
            {
                checker(p, t, a, k);
            }
            if let Some(tr) = trim {
                p.decal(tr).vb(-3, 3, 18, 18, -2, -2);
            }
        }
        Cut::Tunic => {
            for (id, s) in [(LEG_L, -1), (LEG_R, 1)] {
                p.on(id).c(t).vb(s, s * 3, 5, 9, -3, 2);
            }
            p.on(0);
            if let Some(tr) = trim {
                p.decal(tr).vb(-1, 1, 18, 18, -2, -2);
            }
        }
        Cut::Bodysuit => {
            if let Some(sd) = k.top.sides.map(Col::rgb) {
                p.decal(sd).vb(-3, -3, 11, 17, -2, 1).vb(3, 3, 11, 17, -2, 1).vb(-3, 3, 10, 11, -2, 1);
            }
            if let Some(tr) = trim {
                p.decal(tr).vb(-3, 3, 11, 11, -2, 1);
            }
        }
        Cut::Vest => {
            p.c(skin).vb(-1, 1, 11, 18, -2, -2);
            p.decal(mul(skin, 0.86)).px(-1, 16, -2).px(1, 16, -2).px(0, 13, -2);
            if let Some(tr) = trim {
                p.decal(tr).vb(-2, -2, 11, 18, -2, -2).vb(2, 2, 11, 18, -2, -2);
            }
        }
        Cut::Overalls => {
            let o = alt.unwrap_or([40, 70, 170]);
            p.decal(o).vb(-2, 2, 12, 15, -2, -2).vb(-2, -2, 16, 18, -2, -2).vb(2, 2, 16, 18, -2, -2).vb(-3, 3, 10, 12, -2, 1).vb(-2, -2, 13, 18, 1, 1).vb(2, 2, 13, 18, 1, 1);
            let btn = trim.unwrap_or([250, 210, 40]);
            p.c(btn).px(-2, 15, -3).px(2, 15, -3);
        }
        Cut::Jacket => {
            let inner = alt.unwrap_or(WHITE);
            p.decal(inner).vb(-1, 1, 11, 18, -2, -2);
            if let Some(tr) = trim {
                p.decal(tr).vb(-2, -2, 11, 18, -2, -2).vb(2, 2, 11, 18, -2, -2);
            }
            p.c(t).vb(-3, 3, 19, 19, 0, 2).vbs(3, 3, 19, 19, -2, 2);
        }
    }
    if let Some(st) = k.top.stripes.map(Col::rgb) {
        for y in (11..=17).step_by(2) {
            p.decal(st).vb(-3, 3, y, y, chest_z, back_z);
        }
    }
    if k.top.cut == Cut::Waistcoat {
        let shirt = alt.unwrap_or(WHITE);
        p.decal(shirt).vb(-1, 1, 16, 18, -2, -2);
        p.decal(trim.unwrap_or(shade(t))).px(0, 15, -2).px(0, 13, -2).px(0, 11, -2);
        if k.top.trim.is_none() {
            p.decal(shade(t)).vb(0, 0, 11, 15, -2, -2);
        }
    }
    // --- sleeves / arms / hands
    for (id, s) in [(ARM_L, -1), (ARM_R, 1)] {
        p.on(id);
        let sleeve = match k.top.cut {
            Cut::Overalls | Cut::Gi | Cut::Vest => alt.filter(|_| k.top.cut != Cut::Overalls).unwrap_or(t),
            _ => t,
        };
        let sleeve = if k.top.cut == Cut::Overalls { t } else { sleeve };
        let sleeve = if k.top.cut == Cut::Waistcoat { alt.unwrap_or(WHITE) } else { sleeve };
        let (upper, fore) = match k.sleeves {
            Sleeves::Long => (Some(sleeve), Some(if k.top.cut == Cut::Tunic { alt.unwrap_or(sleeve) } else { sleeve })),
            Sleeves::Short => (Some(sleeve), None),
            Sleeves::Bare => (None, None),
        };
        p.c(upper.unwrap_or(skin));
        arm(p, s, 15, 18);
        p.c(fore.unwrap_or(skin));
        arm(p, s, 12, 14);
        if k.sleeves == Sleeves::Short {
            p.c(sleeve).vb(s * 4, s * 6, 15, 15, -2, 0);
        }
        if let Some(st) = k.top.stripes.map(Col::rgb)
            && k.top.cut != Cut::Waistcoat
        {
            let lo = if k.sleeves == Sleeves::Long { 12 } else { 16 };
            if k.sleeves != Sleeves::Bare {
                for y in (lo..=17).step_by(2) {
                    p.decal(st).vb(s * 4, s * 6, y, y, -2, 0);
                }
            }
        }
        if k.top.cut == Cut::Bodysuit
            && let Some(sd) = k.top.sides.map(Col::rgb)
        {
            p.decal(sd).vb(s * 4, s * 6, 13, 17, -2, 0);
            p.decal(t).vb(s * 4, s * 6, 18, 18, -2, 0);
        }
        if matches!(k.top.cut, Cut::Kimono | Cut::Robe | Cut::Haori) && k.sleeves == Sleeves::Long {
            p.c(sleeve).vb(s * 4, s * 6, 12, 15, -3, 1);
        }
        if k.top.cut == Cut::Plate || k.top.cut == Cut::Power {
            p.c(t).vb(s * 4, s * 7, 16, 19, -3, 1);
            p.decal(shade(t)).vb(s * 4, s * 7, 16, 16, -3, 1);
            p.c(t).vb(s * 4, s * 6, 11, 14, -3, 1);
        }
        if let Some(tr) = trim
            && matches!(k.top.cut, Cut::Tee | Cut::Gi | Cut::Kimono)
            && k.sleeves != Sleeves::Bare
        {
            p.decal(tr).vb(s * 4, s * 6, if k.sleeves == Sleeves::Short { 15 } else { 12 }, if k.sleeves == Sleeves::Short { 15 } else { 12 }, -2, 0);
        }
        // Hands.
        let hands = if hide & acc::ARMS != 0 { Hands::Bare } else { k.hands };
        match hands {
            Hands::Bare => {
                p.c(skin);
                arm(p, s, 9, 11);
            }
            Hands::Gloves(c) => {
                p.c(c.rgb());
                arm(p, s, 9, 11);
            }
            Hands::Gauntlets(c) => {
                let c = c.rgb();
                p.c(c);
                arm(p, s, 9, 11);
                p.c(c).vb(s * 4, s * 7, 10, 14, -3, 1);
                p.decal(shade(c)).vb(s * 4, s * 7, 14, 14, -3, 1);
            }
            Hands::Wraps(c) => {
                p.c(skin);
                arm(p, s, 9, 11);
                p.decal(c.rgb()).vb(s * 4, s * 6, 10, 10, -2, 0).vb(s * 4, s * 6, 12, 13, -2, 0);
            }
            Hands::Wristbands(c) => {
                p.c(skin);
                arm(p, s, 9, 11);
                p.c(c.rgb()).vb(s * 4, s * 6, 11, 12, -3, 1);
            }
            Hands::Fingerless(c) => {
                p.c(c.rgb());
                arm(p, s, 9, 11);
                p.decal(skin).vb(s * 4, s * 6, 9, 9, -2, 0);
            }
        }
    }
    p.on(0);
    // --- belt
    match k.belt {
        Belt::No => {}
        Belt::Belt(c, buckle) => {
            let c = c.rgb();
            p.c(c).vb(-3, 3, 11, 11, chest_z, back_z);
            if let Some(b) = buckle {
                p.c(b.rgb()).vb(-1, 1, 11, 11, chest_z - 1, chest_z - 1);
            }
        }
        Belt::Sash(c) => {
            let c = c.rgb();
            p.c(c).vb(-3, 3, 11, 12, chest_z, back_z);
            p.c(c).vb(-3, -2, 8, 10, chest_z - 1, chest_z - 1).px(-3, 11, chest_z - 1);
        }
        Belt::Utility(c) => {
            let c = c.rgb();
            p.c(c).vb(-3, 3, 11, 11, chest_z, back_z);
            p.c(c).vb(-3, 3, 11, 11, chest_z - 1, chest_z - 1);
            p.decal(mul(c, 0.7)).px(-2, 11, chest_z - 1).px(0, 11, chest_z - 1).px(2, 11, chest_z - 1);
        }
    }
    // --- emblems
    if let Some(m) = &k.emblem {
        let w = m.rows.iter().map(|r| r.chars().count()).max().unwrap_or(0) as i32;
        let top = m.y.unwrap_or(17);
        mosaic(p, m, IVec3::new(-(w - 1) / 2, top, chest_z), IVec3::X, IVec3::NEG_Y, IVec3::Z, 1);
    }
    if let Some(m) = &k.back_emblem {
        let w = m.rows.iter().map(|r| r.chars().count()).max().unwrap_or(0) as i32;
        let top = m.y.unwrap_or(17);
        mosaic(p, m, IVec3::new((w - 1) / 2, top, back_z), IVec3::NEG_X, IVec3::NEG_Y, IVec3::NEG_Z, 1);
    }
}

/// Two-colour 2×2 checks on the torso and the haori skirt (Tanjiro).
fn checker(p: &mut Pn, a: Rgb, b: Rgb, _k: &Kit) {
    p.on(0);
    for y in 10..=18 {
        for x in -3..=3 {
            let c = if ((x + 8) / 2 + y / 2) % 2 == 0 { a } else { b };
            p.decal(c).px(x, y, -2).px(x, y, 1);
        }
        for z in -2..=1 {
            let c = if ((z + 8) / 2 + y / 2) % 2 == 0 { a } else { b };
            p.decal(c).px(-3, y, z).px(3, y, z);
        }
    }
    for (id, s) in [(LEG_L, -1), (LEG_R, 1)] {
        p.on(id);
        for y in 5..=9 {
            for x in 1..=3 {
                let c = if ((x * s + 8) / 2 + y / 2) % 2 == 0 { a } else { b };
                p.decal(c).px(x * s, y, -3).px(x * s, y, 2);
            }
        }
    }
    p.on(0);
}

fn legs(p: &mut Pn, k: &Kit, hide: u32) {
    let c = k.legs.color.rgb();
    let alt = k.legs.alt.map(Col::rgb);
    let skin = k.skin;
    // Pelvis.
    p.on(0).c(c).vb(-3, 3, 10, 11, -2, 1);
    for (id, s) in [(LEG_L, -1), (LEG_R, 1)] {
        p.on(id);
        match k.legs.cut {
            LegCut::Pants | LegCut::Tights => {
                p.c(c);
                leg(p, s, 2, 9);
                if let Some(a) = alt {
                    p.decal(a).vb(s * 3, s * 3, 2, 9, -1, 0);
                }
            }
            LegCut::Baggy => {
                p.c(c);
                leg(p, s, 2, 9);
                p.c(c).vb(s, s * 3, 3, 8, -3, 2);
            }
            LegCut::Shorts => {
                p.c(skin);
                leg(p, s, 2, 5);
                p.c(c);
                leg(p, s, 6, 9);
            }
            LegCut::Skirt => {
                p.c(alt.unwrap_or(skin));
                leg(p, s, 2, 6);
                p.c(c).vb(s, s * 3, 6, 9, -3, 2);
            }
            LegCut::LongSkirt => {
                p.c(c).vb(s, s * 3, 1, 9, -3, 2);
            }
            LegCut::Trunks => {
                p.c(c);
                leg(p, s, 2, 9);
                p.c(alt.unwrap_or(shade(c))).vb(s, s * 3, 8, 9, -2, 1);
            }
            LegCut::Armored => {
                p.c(alt.unwrap_or([60, 62, 68]));
                leg(p, s, 2, 9);
                p.c(c).vb(s, s * 3, 6, 9, -3, 2).vb(s, s * 3, 2, 5, -3, 1);
                p.decal(shade(c)).vb(s, s * 3, 5, 5, -3, -3);
            }
            LegCut::Hakama => {
                p.c(c).vb(s, s * 3, 1, 9, -3, 2).vb(s * 3, s * 4, 1, 7, -3, 2);
                p.decal(shade(c)).vb(s * 2, s * 2, 1, 8, -3, -3);
            }
        }
        // Feet.
        let f = k.feet;
        let fc = f.color.rgb();
        let kind = if hide & acc::FEET != 0 && !matches!(f.kind, FootKind::Bare | FootKind::Sandals) { FootKind::Shoes } else { f.kind };
        let under_skirt = matches!(k.legs.cut, LegCut::LongSkirt | LegCut::Hakama);
        match kind {
            FootKind::Shoes => {
                p.c(fc).vb(s, s * 3, 0, 1, -3, 1);
            }
            FootKind::Sneakers => {
                p.c(fc).vb(s, s * 3, 0, 1, -3, 1);
                p.c(f.alt.map_or(WHITE, Col::rgb)).vb(s, s * 3, 0, 0, -3, 1);
            }
            FootKind::Boots => {
                p.c(fc).vb(s, s * 3, 0, 1, -3, 1);
                if !under_skirt {
                    p.c(fc).vb(s, s * 3, 2, 4, -2, 1);
                    p.c(f.alt.map_or(shade(fc), Col::rgb)).vb(s, s * 3, 4, 4, -3, 2);
                }
            }
            FootKind::HighBoots => {
                p.c(fc).vb(s, s * 3, 0, 1, -3, 1);
                if !under_skirt {
                    p.c(fc).vb(s, s * 3, 2, 6, -2, 1);
                    if let Some(a) = f.alt {
                        p.c(a.rgb()).vb(s, s * 3, 6, 6, -3, 2);
                    }
                }
            }
            FootKind::Sandals => {
                p.c(skin).vb(s, s * 3, 0, 1, -3, 1);
                p.c(fc).vb(s, s * 3, 0, 0, -3, 1);
                p.decal(fc).vb(s, s * 3, 1, 1, -3, -3);
            }
            FootKind::Bare => {
                p.c(skin).vb(s, s * 3, 0, 1, -3, 1);
                if !under_skirt {
                    p.c(skin);
                    leg(p, s, 2, if matches!(k.legs.cut, LegCut::Shorts) { 5 } else { 2 });
                }
            }
            FootKind::Armored => {
                p.c(fc).vb(s, s * 3, 0, 3, -4, 2);
                p.decal(shade(fc)).vb(s, s * 3, 3, 3, -4, -4);
            }
        }
    }
    p.on(0);
}

fn mosaic(p: &mut Pn, m: &Mosaic, origin: IVec3, right: IVec3, down: IVec3, n: IVec3, depth: i32) {
    let rows: Vec<&str> = m.rows.iter().map(String::as_str).collect();
    let pal = |ch: char| m.pal.get(&ch).map(|c| c.rgb());
    p.mosaic(&rows, &pal, origin, right, down, n, depth);
}

// ------------------------------------------------------------------------------------ accessories

fn slot_of(a: &Acc) -> u32 {
    match a {
        Acc::Cape(..) | Acc::Cloak(..) | Acc::Wings(..) | Acc::BackSword(..) | Acc::BigSword(..) | Acc::TwinSwords(..) | Acc::Staff(..) | Acc::RoundShield(..) | Acc::KiteShield(..) | Acc::Backpack(..) | Acc::Jetpack(..) | Acc::Gourd(..) | Acc::Quiver(..) | Acc::Bow(..) | Acc::Polearm(..) => acc::BACK,
        Acc::Tail(..) => acc::TAIL,
        Acc::HipSwords(..) | Acc::Pouches(..) | Acc::Saber(..) | Acc::Holsters(..) | Acc::Hammer(..) | Acc::SashTails(..) => acc::HIP,
        Acc::Pauldron(..) | Acc::Pauldrons(..) | Acc::Spikes(..) | Acc::Mantle(..) | Acc::Bandolier(..) | Acc::Necklace(..) => acc::SHOULDERS,
        Acc::Claws(..) => acc::ARMS,
        Acc::Gauntlet(_, _, Side::Left) => acc::LEFT_ARM,
        Acc::Gauntlet(..) => acc::ARMS,
        Acc::Scarf(..) | Acc::Collar(..) | Acc::Earpiece(..) => acc::FACE,
        Acc::ChestLight(..) => acc::AURA,
        Acc::Axe(..) | Acc::Shell(..) => acc::BACK,
        Acc::ArmCannon(_, _, Side::Left) => acc::LEFT_ARM,
        Acc::ArmCannon(..) => acc::ARMS,
    }
}

fn accessory(cx: &mut Cx, a: &Acc, k: &Kit) {
    let slot = slot_of(a);
    let bulky = matches!(k.top.cut, Cut::Plate | Cut::Power) || k.broad;
    let back = if bulky { 3 } else { 2 };
    let chest = if bulky { -3 } else { -2 };
    let anchor = match slot {
        acc::BACK => v3(0.0, 14.0, back as f32),
        acc::TAIL => v3(0.0, 11.0, 2.0),
        acc::HIP => v3(0.0, 10.0, 0.0),
        acc::SHOULDERS => v3(0.0, 18.0, 0.0),
        acc::FACE => v3(0.0, 21.0, -4.0),
        _ => v3(0.0, 12.0, 0.0),
    };
    let a = a.clone();
    cx.pgroup(slot, anchor, |p| {
        p.on(0);
        match a {
            Acc::Cape(c, lining, collar) => {
                let (c, l) = (c.rgb(), lining.map_or(shade(c.rgb()), Col::rgb));
                p.part(0, v3(0.0, 18.0, back as f32), PartRole::Cloth);
                p.c(l).vb(-4, 4, 3, 18, back, back).vb(-5, 5, 3, 8, back, back);
                p.c(c).vb(-4, 4, 3, 18, back + 1, back + 1).vb(-5, 5, 3, 9, back + 1, back + 1).vb(-3, 3, 18, 18, back - 1, back + 1);
                if collar {
                    p.c(c).vb(-5, 5, 19, 22, 4, 4).vbs(5, 5, 19, 22, 1, 4);
                    p.c(l).vb(-4, 4, 19, 21, 3, 3);
                }
                p.on(0).c([220, 190, 80]).px(-3, 18, chest - 1).px(3, 18, chest - 1);
            }
            Acc::Cloak(c) => {
                let c = c.rgb();
                p.part(0, v3(0.0, 18.0, back as f32), PartRole::Cloth);
                p.c(c).vb(-4, 4, 11, 18, back, back + 1).vb(-3, 3, 10, 10, back, back + 1);
                p.on(0).c(c).vb(-3, 3, 19, 19, -2, back).vbs(3, 3, 18, 18, -3, back);
            }
            Acc::Scarf(c) => {
                let c = c.rgb();
                p.c(c).vb(-3, 3, 19, 19, -3, 2).vb(-2, 2, 20, 20, -4, -4).vb(-2, 2, 20, 20, 3, 3);
                p.part(0, v3(-2.0, 18.0, -3.0), PartRole::Cloth);
                p.c(c).vb(-3, -2, 12, 18, -3, -3);
                p.decal(shade(c)).vb(-3, -2, 12, 12, -3, -3);
            }
            Acc::Wings(kind, c) => {
                let c = c.rgb();
                for (role, s) in [(PartRole::WingLeft, -1.0f32), (PartRole::WingRight, 1.0)] {
                    p.part(0, v3(s * 2.0, 16.0, back as f32 + 1.0), role);
                    let bz = back as f32 + 1.5;
                    match kind {
                        WingKind::Angel | WingKind::Butterfly | WingKind::Insect => {
                            p.c(c).seg(v3(s * 2.0, 16.0, bz), v3(s * 7.0, 25.0, bz + 2.0), 3.0, 1.0);
                            p.c(mul(c, 0.94)).seg(v3(s * 2.5, 15.0, bz), v3(s * 7.5, 20.0, bz + 2.5), 3.0, 1.0);
                            p.c(mul(c, 0.88)).seg(v3(s * 2.5, 14.0, bz), v3(s * 6.5, 8.0, bz + 2.0), 2.6, 1.0);
                        }
                        WingKind::Bat => {
                            p.c(mul(c, 0.7)).seg(v3(s * 2.0, 16.0, bz), v3(s * 7.5, 24.0, bz + 2.0), 1.0, 1.0);
                            p.c(c).seg(v3(s * 3.0, 15.0, bz), v3(s * 7.0, 17.0, bz + 2.0), 3.0, 1.0).seg(v3(s * 3.0, 13.0, bz), v3(s * 6.0, 10.0, bz + 1.5), 3.0, 1.0);
                        }
                        WingKind::Mech => {
                            p.c(c).seg(v3(s * 2.0, 17.0, bz), v3(s * 7.0, 24.0, bz + 1.0), 2.0, 1.0).seg(v3(s * 2.0, 15.0, bz), v3(s * 7.5, 18.0, bz + 1.0), 2.0, 1.0);
                            p.c(mix(c, WHITE, 0.4)).seg(v3(s * 2.0, 13.0, bz), v3(s * 6.5, 12.0, bz + 1.0), 2.0, 1.0);
                        }
                    }
                }
            }
            Acc::Tail(kind, c) => {
                let c = c.rgb();
                p.part(0, v3(0.0, 11.0, 2.0), PartRole::Tail);
                match kind {
                    TailKind::Monkey => {
                        p.c(c).seg(v3(0.0, 11.0, 2.0), v3(0.0, 8.0, 6.0), 1.6, 1.6).seg(v3(0.0, 8.0, 6.0), v3(-1.0, 4.0, 8.0), 1.6, 1.6).seg(v3(-1.0, 4.0, 8.0), v3(-1.5, 5.0, 10.0), 1.6, 1.6);
                    }
                    TailKind::Cat | TailKind::Demon => {
                        p.c(c).seg(v3(0.0, 11.0, 2.0), v3(0.0, 7.0, 6.0), 1.4, 1.4).seg(v3(0.0, 7.0, 6.0), v3(0.0, 9.0, 9.0), 1.4, 1.4);
                        if kind == TailKind::Demon {
                            p.c(c).cone_to(v3(0.0, 9.0, 9.0), v3(0.0, 11.0, 10.0), 2.6);
                        }
                    }
                    TailKind::Fox => {
                        p.c(c).eq(v3(0.0, 8.0, 5.5), v3(4.0, 4.0, 8.0), Quat::from_rotation_x(0.6));
                        p.c(WHITE).el(v3(0.0, 5.5, 8.5), v3(2.6, 2.6, 2.6));
                    }
                    TailKind::Lizard => {
                        p.c(c).cone_to(v3(0.0, 10.0, 2.0), v3(0.0, 3.0, 9.0), 3.0);
                    }
                }
            }
            Acc::BackSword(blade, hilt) => {
                let bz = back as f32 + 1.0;
                p.c(mul(hilt.rgb(), 0.8)).seg(v3(-4.0, 7.0, bz), v3(3.0, 20.0, bz), 1.6, 1.2);
                p.c(blade.rgb()).seg(v3(-4.0, 7.0, bz), v3(2.6, 19.2, bz), 0.8, 1.4);
                p.c([200, 170, 60]).seg(v3(2.6, 19.5, bz), v3(3.8, 20.4, bz), 2.4, 1.6);
                p.c(hilt.rgb()).seg(v3(3.2, 20.0, bz), v3(5.2, 24.0, bz), 1.4, 1.4);
            }
            Acc::BigSword(blade, hilt) => {
                let bz = back + 1;
                let bl = blade.rgb();
                p.c(bl).vb(-2, 2, 3, 20, bz, bz);
                p.c(bl).vb(-1, 1, 2, 2, bz, bz);
                p.decal(shade(bl)).px(0, 17, bz).px(0, 15, bz).vb(-2, 2, 20, 20, bz, bz);
                p.c(mul(hilt.rgb(), 0.7)).vb(-3, 3, 21, 21, bz, bz);
                p.c(hilt.rgb()).vb(0, 0, 22, 25, bz, bz);
            }
            Acc::TwinSwords(blade, hilt) => {
                let bz = back as f32 + 1.0;
                for s in [-1.0f32, 1.0] {
                    p.c(blade.rgb()).seg(v3(-s * 4.0, 7.0, bz), v3(s * 3.0, 20.0, bz), 1.0, 1.0);
                    p.c(hilt.rgb()).seg(v3(s * 3.0, 20.0, bz), v3(s * 4.6, 24.0, bz), 1.4, 1.4);
                }
            }
            Acc::HipSwords(n, c) => {
                let c = c.rgb();
                let cols = [c, [230, 230, 230], [40, 40, 44], [200, 40, 40]];
                for i in 0..n.min(3) as usize {
                    let z = -2.5 + i as f32 * 1.5;
                    p.c(cols[i % 4]).seg(v3(-4.5, 12.0, z - 2.0), v3(-4.5, 6.0, z + 5.0), 1.0, 1.0);
                    p.c([180, 160, 70]).seg(v3(-4.5, 12.0, z - 2.0), v3(-4.5, 13.5, z - 4.0), 1.2, 1.2);
                }
            }
            Acc::Staff(c) => {
                let bz = back as f32 + 1.0;
                p.c(c.rgb()).rod(v3(-5.0, 4.0, bz), v3(5.0, 25.0, bz), 1.4);
                p.c([230, 180, 50]).rod(v3(-5.2, 3.6, bz), v3(-4.4, 5.2, bz), 1.8).rod(v3(4.4, 23.8, bz), v3(5.2, 25.4, bz), 1.8);
            }
            Acc::RoundShield(face, rim, star) => {
                let bz = back + 1;
                let (f, r) = (face.rgb(), rim.rgb());
                p.c(r).cz(v3(0.0, 14.0, bz as f32), 11.0, 1.0);
                p.c(f).cz(v3(0.0, 14.0, bz as f32 + 0.6), 8.0, 1.0);
                p.c(r).cz(v3(0.0, 14.0, bz as f32 + 1.2), 5.0, 1.0);
                if let Some(s) = star {
                    let s = s.rgb();
                    p.c([40, 60, 160]).cz(v3(0.0, 14.0, bz as f32 + 1.8), 3.0, 1.0);
                    p.decal(s).px(0, 14, bz + 2).px(0, 15, bz + 2).px(-1, 14, bz + 2).px(1, 14, bz + 2);
                }
            }
            Acc::KiteShield(face, trim) => {
                let bz = back + 1;
                let (f, t) = (face.rgb(), trim.rgb());
                p.c(t).vb(-4, 4, 11, 19, bz, bz).vb(-3, 3, 9, 10, bz, bz).vb(-1, 1, 8, 8, bz, bz);
                p.c(f).vb(-3, 3, 11, 18, bz + 1, bz + 1).vb(-2, 2, 10, 10, bz + 1, bz + 1);
                p.decal([200, 40, 40]).vb(-1, 1, 13, 13, bz + 1, bz + 1).px(0, 14, bz + 1).px(0, 12, bz + 1);
                p.decal(t).vb(-2, 2, 16, 16, bz + 1, bz + 1).px(0, 17, bz + 1);
            }
            Acc::Backpack(c) => {
                let c = c.rgb();
                p.c(c).vb(-3, 3, 11, 17, back, back + 2);
                p.decal(shade(c)).vb(-2, 2, 13, 13, back + 2, back + 2);
                p.c(shade(c)).vb(-2, -2, 16, 18, chest, chest).vb(2, 2, 16, 18, chest, chest);
            }
            Acc::Jetpack(c, flame) => {
                let c = c.rgb();
                for s in [-1.0f32, 1.0] {
                    p.c(c).cy(v3(s * 2.0, 14.5, back as f32 + 2.0), 3.0, 8.0);
                    p.c(mix(c, WHITE, 0.3)).cone_to(v3(s * 2.0, 19.0, back as f32 + 2.0), v3(s * 2.0, 21.0, back as f32 + 2.0), 2.4);
                    p.c(shade(c)).cy(v3(s * 2.0, 10.0, back as f32 + 2.0), 2.0, 1.0);
                    p.palette(Tint::Core).px(if s < 0.0 { -2 } else { 2 }, 9, back + 2);
                }
                let _ = flame;
                p.c(shade(c)).vb(-1, 1, 12, 17, back, back + 1);
            }
            Acc::Pauldron(c, side) => {
                let (id, s) = if side == Side::Left { (ARM_L, -1) } else { (ARM_R, 1) };
                let c = c.rgb();
                p.on(id).c(c).vb(s * 4, s * 7, 16, 19, -3, 1);
                p.decal(shade(c)).vb(s * 4, s * 7, 16, 16, -3, 1);
            }
            Acc::Pauldrons(c) => {
                let c = c.rgb();
                for (id, s) in [(ARM_L, -1), (ARM_R, 1)] {
                    p.on(id).c(c).vb(s * 4, s * 7, 16, 19, -3, 1);
                    p.decal(shade(c)).vb(s * 4, s * 7, 16, 16, -3, 1);
                }
            }
            Acc::Spikes(c) => {
                let c = c.rgb();
                for (id, s) in [(ARM_L, -1.0f32), (ARM_R, 1.0)] {
                    p.on(id).c(c);
                    for z in [-1.5f32, 0.5] {
                        p.cone_to(v3(s * 5.5, 19.0, z), v3(s * 6.5, 22.0, z), 1.8);
                    }
                }
            }
            Acc::Claws(c) => {
                let c = c.rgb();
                for (id, s) in [(ARM_L, -1.0f32), (ARM_R, 1.0)] {
                    p.on(id).c(c);
                    for x in [4.0f32, 5.0, 6.0] {
                        p.seg(v3(s * x, 10.0, -3.0), v3(s * x, 10.5, -8.5), 0.6, 0.6);
                    }
                }
            }
            Acc::Gauntlet(c, gem, side) => {
                let (id, s) = if side == Side::Left { (ARM_L, -1) } else { (ARM_R, 1) };
                let c = c.rgb();
                p.on(id).c(c).vb(s * 4, s * 7, 8, 13, -3, 1);
                let g = gem.rgb();
                p.decal(g).px(s * 5, 11, -3).px(s * 4, 10, -3).px(s * 6, 10, -3);
                p.decal([60, 200, 90]).px(s * 5, 9, -3);
                p.decal([240, 200, 40]).px(s * 7, 11, -1);
            }
            Acc::Pouches(c) => {
                let c = c.rgb();
                for x in [-3, -1, 1, 3] {
                    p.c(c).vb(x, x, 10, 11, chest - 1, chest - 1);
                }
            }
            Acc::Saber(c) => {
                p.on(LEG_L).c(c.rgb()).vb(-4, -4, 5, 8, -1, -1);
                p.c([40, 40, 44]).vb(-4, -4, 6, 6, -1, -1);
            }
            Acc::Holsters(c) => {
                let c = c.rgb();
                for (id, s) in [(LEG_L, -1), (LEG_R, 1)] {
                    p.on(id).c(c).vb(s * 4, s * 4, 4, 8, -1, 0);
                    p.c([40, 40, 44]).vb(s * 4, s * 4, 8, 9, -2, -1);
                }
            }
            Acc::Gourd(c) => {
                let c = c.rgb();
                p.c(c).el(v3(0.0, 11.0, back as f32 + 3.0), v3(8.0, 8.0, 5.0)).el(v3(0.0, 18.0, back as f32 + 2.5), v3(5.0, 5.0, 4.0));
                p.c(shade(c)).vb(-3, 3, 14, 14, back + 1, back + 5);
            }
            Acc::Quiver(c, arrows) => {
                let bz = back as f32 + 1.5;
                p.c(c.rgb()).seg(v3(-3.0, 9.0, bz), v3(2.0, 19.0, bz), 2.4, 2.4);
                p.c(arrows.rgb()).seg(v3(1.5, 19.0, bz), v3(3.0, 22.5, bz), 2.0, 1.0);
            }
            Acc::Bow(c) => {
                let bz = back as f32 + 1.0;
                p.c(c.rgb()).seg(v3(-4.0, 6.0, bz), v3(-1.0, 13.0, bz), 1.0, 1.0).seg(v3(-1.0, 13.0, bz), v3(-4.0, 21.0, bz), 1.0, 1.0);
                p.c(WHITE).seg(v3(-4.0, 6.0, bz), v3(-4.0, 21.0, bz), 0.5, 0.5);
            }
            Acc::Hammer(h, handle) => {
                p.on(LEG_R).c(handle.rgb()).vb(4, 4, 3, 9, -1, -1);
                p.c(h.rgb()).vb(4, 5, 1, 3, -3, 1);
            }
            Acc::Necklace(c) => {
                let c = c.rgb();
                p.decal(c).px(-2, 18, chest).px(-1, 17, chest).px(0, 17, chest).px(1, 17, chest).px(2, 18, chest);
            }
            Acc::ChestLight(c) => {
                let c = c.rgb();
                p.c(mix(c, WHITE, 0.2)).vb(-1, 1, 15, 17, chest - 1, chest - 1);
                p.c(WHITE).px(0, 16, chest - 1);
                p.decal(shade(c)).px(-1, 15, chest - 1).px(1, 15, chest - 1).px(-1, 17, chest - 1).px(1, 17, chest - 1);
            }
            Acc::SashTails(c) => {
                let c = c.rgb();
                p.part(0, v3(0.0, 11.0, back as f32), PartRole::Cloth);
                p.c(c).seg(v3(-1.0, 11.0, back as f32), v3(-2.0, 4.0, back as f32 + 1.5), 1.6, 1.0).seg(v3(1.0, 11.0, back as f32), v3(2.0, 5.0, back as f32 + 1.0), 1.6, 1.0);
            }
            Acc::Collar(c) => {
                p.c(c.rgb()).px(-1, 20, -5);
            }
            Acc::Bandolier(c) => {
                let c = c.rgb();
                for i in 0..8 {
                    p.decal(c).px(-3 + i, 18 - i, chest).px(-3 + i, 18 - i, back);
                }
                for i in 0..4 {
                    p.c(mix(c, WHITE, 0.4)).px(-2 + i * 2, 17 - i * 2, chest - 1);
                }
            }
            Acc::Mantle(c) => {
                let c = c.rgb();
                p.c(c).vb(-3, 3, 18, 19, -3, back).vb(-3, 3, 17, 17, back, back);
                for (id, s) in [(ARM_L, -1), (ARM_R, 1)] {
                    p.on(id).c(c).vb(s * 4, s * 7, 17, 19, -3, 1);
                }
            }
            Acc::Polearm(c, blade) => {
                let bz = back as f32 + 1.0;
                p.c(c.rgb()).rod(v3(-5.0, 3.0, bz), v3(5.0, 27.0, bz), 1.2);
                p.c(blade.rgb()).seg(v3(5.0, 27.0, bz), v3(0.5, 29.5, bz), 3.0, 0.8);
            }
            Acc::Axe(head, handle) => {
                let bz = back as f32 + 1.0;
                p.c(handle.rgb()).rod(v3(-4.0, 5.0, bz), v3(3.5, 22.0, bz), 1.2);
                let hd = head.rgb();
                p.c(hd).seg(v3(2.0, 20.5, bz), v3(6.5, 18.0, bz), 3.0, 1.0).seg(v3(3.0, 23.0, bz), v3(7.0, 22.5, bz), 3.4, 1.0);
                p.decal(mix(hd, WHITE, 0.4)).vb(6, 7, 18, 23, back + 1, back + 1);
            }
            Acc::Shell(c, rim) => {
                let (c, r) = (c.rgb(), rim.rgb());
                p.c(r).vb(-4, 4, 9, 18, back, back);
                p.c(c).el(v3(0.0, 13.5, back as f32 + 1.0), v3(9.0, 11.0, 5.0));
                p.decal(shade(c)).vb(-3, 3, 13, 13, back + 1, back + 4).vb(0, 0, 9, 18, back + 1, back + 4);
            }
            Acc::ArmCannon(c, glow, side) => {
                let (id, s) = if side == Side::Left { (ARM_L, -1) } else { (ARM_R, 1) };
                let c = c.rgb();
                p.on(id).c(c).vb(s * 4, s * 7, 8, 14, -4, 1);
                p.c(shade(c)).vb(s * 4, s * 7, 14, 14, -4, 1);
                p.c(glow.rgb()).vb(s * 5, s * 6, 9, 11, -5, -5);
            }
            Acc::Earpiece(c) => {
                p.on(HEAD).c(c.rgb()).vb(5, 5, 23, 24, -2, -1);
            }
        }
        p.on(0);
    });
}
