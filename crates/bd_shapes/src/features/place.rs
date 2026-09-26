//! Generic feature placement on any form: relative to the root part's bounds, probing the actual
//! surface so assemblies sit flush (+Y up, −Z front). Creatures get anatomy-aware spots: boilers on
//! the back, pistons along the legs, gears on the flanks/shoulders, a wind-up key on clockwork
//! creatures, drills/saws on the arms, headlamps on the head.

use bd_core::mechanics::{Feature, Form, FormCategory};
use bd_core::shape::{PartId, PartRole};
use glam::Vec3;

use super::kit::*;
use super::MotionSpec;
use crate::attach::{SpecBounds, spec_bounds, spec_bounds_of_part};
use crate::build::{A, B, C, D, P, odd, v3};
use crate::voxelize::PrimEval;

struct Anatomy {
    legs: Vec<(PartId, SpecBounds, Vec3)>,
    arms: Vec<(PartId, SpecBounds, f32)>,
    head: Option<(PartId, SpecBounds)>,
}

fn anatomy(b: &B) -> Anatomy {
    let mut a = Anatomy { legs: Vec::new(), arms: Vec::new(), head: None };
    for p in &b.spec.parts {
        let Some(bb) = spec_bounds_of_part(&b.spec, p.id) else { continue };
        match p.role {
            PartRole::LegFrontLeft | PartRole::LegFrontRight | PartRole::LegBackLeft | PartRole::LegBackRight => a.legs.push((p.id, bb, p.pivot)),
            PartRole::ArmLeft => a.arms.push((p.id, bb, -1.0)),
            PartRole::ArmRight => a.arms.push((p.id, bb, 1.0)),
            PartRole::Head if a.head.is_none() => a.head = Some((p.id, bb)),
            _ => {}
        }
    }
    a
}

/// March from `from` along `dir` until inside an additive prim of `part`; returns the last outside
/// point (the surface). `None` if nothing is hit within `max` units.
fn probe(b: &B, part: PartId, from: Vec3, dir: Vec3, max: f32) -> Option<Vec3> {
    let d = dir.normalize_or(Vec3::NEG_Y);
    let evals: Vec<PrimEval> = b
        .spec
        .prims
        .iter()
        .filter(|p| !p.subtract && p.part == part && p.center.is_finite() && p.size.is_finite() && p.rot_deg.is_finite())
        .map(|p| PrimEval::new(p.kind, p.center, p.size, p.rot_deg))
        .collect();
    let steps = ((max / 0.5).ceil() as usize).min(1200);
    let mut prev = from;
    for i in 0..=steps {
        let q = from + d * (i as f32 * 0.5);
        if evals.iter().any(|e| e.inside(q)) {
            return Some(prev);
        }
        prev = q;
    }
    None
}

pub(super) fn place(b: &mut B, feats: &[Feature], form: Form) {
    use Feature::*;
    let body = spec_bounds_of_part(&b.spec, 0).unwrap_or_else(|| spec_bounds(&b.spec));
    let e = body.size();
    let c = body.center();
    let (mn, mx) = (body.min, body.max);
    let s = e.max_element();
    let an = anatomy(b);
    let legged = !an.legs.is_empty();
    let upright = legged && an.legs.len() <= 2 && e.y >= e.z * 0.9;
    let held = form.category() == FormCategory::Held;
    let has = |f: Feature| feats.contains(&f);
    let reach = s + 8.0;
    // Surface helpers (fall back to the bounds).
    let top_at = |b: &B, x: f32, z: f32| probe(b, 0, v3(x, mx.y + 4.0, z), Vec3::NEG_Y, e.y + 8.0).map_or(mx.y, |p| p.y);
    let side_at = |b: &B, sd: f32, y: f32, z: f32| probe(b, 0, v3(if sd > 0.0 { mx.x + 4.0 } else { mn.x - 4.0 }, y, z), Vec3::X * -sd, e.x + 8.0).map_or(if sd > 0.0 { mx.x } else { mn.x }, |p| p.x);
    let front_at = |b: &B, x: f32, y: f32| probe(b, 0, v3(x, y, mn.z - 4.0), Vec3::Z, e.z + 8.0).map_or(mn.z, |p| p.z);
    let back_at = |b: &B, x: f32, y: f32| probe(b, 0, v3(x, y, mx.z + 4.0), Vec3::NEG_Z, e.z + 8.0).map_or(mx.z, |p| p.z);
    let _ = reach;

    let mut wheel_part: Option<PartId> = None;
    let mut boiler_top: Option<Vec3> = None;
    let mut boiler_back: Option<Vec3> = None;
    b.on(0);

    // --- running gear
    if has(Wheels) {
        if legged {
            // Roller wheels under every foot.
            for (id, lb, _) in &an.legs {
                let wd = odd((lb.size().x.min(lb.size().z) * 1.1).clamp(3.0, 9.0));
                let cz = lb.center().z;
                for sd in [-1.0f32, 1.0] {
                    let x = lb.center().x + sd * (lb.size().x * 0.5 + 0.5);
                    let w = wheel(b, *id, v3(x, lb.min.y + wd * 0.5 - 0.5, cz), wd, 1.0, WheelStyle::Spoked, 0.0);
                    wheel_part.get_or_insert(w);
                }
            }
        } else {
            let wd = odd((e.y * 0.5).clamp(3.0, 15.0));
            let zz = (e.z * 0.5 - wd * 0.6).max(0.0);
            for (sx, sz) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                let cc = v3(c.x + sx * (e.x * 0.5 + 0.5), mn.y + wd * 0.3, c.z + sz * zz);
                let w = wheel(b, 0, cc, wd, 1.0, if s > 24.0 { WheelStyle::Spoked } else { WheelStyle::Tire }, sx + sz);
                wheel_part.get_or_insert(w);
            }
        }
    }
    if has(Treads) {
        if legged {
            for (id, lb, _) in &an.legs {
                let l = lb.size().z.max(3.0) + 2.0;
                b.on(*id);
                let y = lb.min.y + 1.0;
                b.bx(v3(lb.center().x, y, lb.center().z), v3(lb.size().x + 1.0, 2.0, l), D);
                let mut z = lb.center().z - l * 0.5 + 0.5;
                while z < lb.center().z + l * 0.5 {
                    b.bx(v3(lb.center().x, y - 0.5, z), v3(lb.size().x + 1.6, 1.0, 1.0), A);
                    z += 2.0;
                }
            }
            b.on(0);
        } else {
            let th = odd((e.y * 0.45).clamp(3.0, 15.0));
            let tw = odd((e.x * 0.2).clamp(2.0, 9.0)).max(3.0);
            for sd in [-1.0f32, 1.0] {
                tread(b, 0, c.x + sd * (e.x * 0.5 + tw * 0.5), mn.z, mx.z, th, tw, sd);
            }
        }
    }

    // --- steam plant
    if has(Boiler) {
        if upright || legged {
            let d = odd((e.x * 0.5).clamp(3.0, 20.0));
            let z = back_at(b, c.x, c.y) + d * 0.5 + 0.5;
            let y0 = mn.y + e.y * 0.15;
            let y1 = mx.y - e.y * 0.05;
            let (bot, topp) = (v3(c.x, y0, z), v3(c.x, y1, z));
            boiler(b, bot, topp, d, false);
            b.cy(topp + Vec3::Y * 0.5, d - 1.0, 1.0, A);
            b.bx(bot + v3(0.0, 1.5, d * 0.5), v3((d * 0.5).max(1.0), 2.0, 1.0), C);
            b.bolts(bot + v3(0.0, 1.5, d * 0.5 + 0.4), Vec3::Z, (d * 0.35).max(1.0), 6, D);
            boiler_top = Some(topp + Vec3::Y * 0.5);
            boiler_back = Some(v3(c.x, (y0 + y1) * 0.5, z));
        } else {
            let d = odd((e.x.min(e.y) * 0.6).clamp(3.0, 24.0));
            let y = top_at(b, c.x, c.z) + d * 0.5 - 0.5;
            let a = v3(c.x, y, mn.z + e.z * 0.15);
            let z1 = v3(c.x, y, mx.z - e.z * 0.15);
            boiler(b, a, z1, d, true);
            boiler_top = Some(v3(c.x, y + d * 0.5, a.z + 2.0));
            boiler_back = Some(z1);
        }
    }
    if has(Smokestack) {
        let d = odd((s * 0.1).clamp(3.0, 9.0));
        let h = (s * 0.22).clamp(3.0, 20.0);
        let (idle, per_m) = if legged { (0.8, 0.6) } else { (1.0, 0.0) };
        if let Some(t) = boiler_top {
            if upright {
                for sd in [-1.0f32, 1.0] {
                    smokestack(b, 0, t + v3(sd * d * 0.6, -0.5, 0.0), h, d.max(3.0) - 0.0, wheel_part, 4.0, idle);
                    set_last_puff_per_m(b, per_m);
                }
            } else {
                smokestack(b, 0, t - Vec3::Y * 0.5, h, d, wheel_part, 4.0, idle);
                set_last_puff_per_m(b, per_m);
            }
        } else if upright {
            let z = back_at(b, c.x, mx.y - 1.0);
            for sd in [-1.0f32, 1.0] {
                smokestack(b, 0, v3(c.x + sd * e.x * 0.3, mx.y - 1.0, z - d * 0.3), h, d, wheel_part, 4.0, idle);
                set_last_puff_per_m(b, per_m);
            }
        } else {
            let z = mn.z + e.z * 0.18;
            smokestack(b, 0, v3(c.x, top_at(b, c.x, z) - 1.0, z), h, d, wheel_part, 4.0, idle);
            set_last_puff_per_m(b, per_m);
        }
    }
    if has(Pipes) {
        let d = (s * 0.05).clamp(1.0, 3.0).round();
        let src = boiler_back.unwrap_or(v3(c.x, c.y, mx.z));
        for sd in [-1.0f32, 1.0] {
            let x = side_at(b, sd, c.y, c.z) + sd * (d * 0.5 + 0.5);
            let dest = if let Some((_, ab, _)) = an.arms.iter().find(|a| a.2 == sd) { v3(ab.center().x, ab.max.y + d, ab.center().z) } else { v3(x, mn.y + e.y * 0.3, mn.z + e.z * 0.15) };
            let p0 = src + v3(sd * (e.x * 0.15), e.y * 0.25, 0.0);
            let p1 = v3(x, p0.y, p0.z);
            let p2 = v3(x, dest.y, dest.z);
            b.on(0);
            pipe_run(b, &[p0, p1, p2, dest], d);
        }
    }
    if has(Pistons) {
        if legged {
            for (k, (id, lb, pivot)) in an.legs.iter().enumerate() {
                let x = if lb.center().x >= c.x { lb.max.x + 1.0 } else { lb.min.x - 1.0 };
                let top = v3(x, pivot.y - 0.5, lb.center().z + lb.size().z * 0.3);
                let len = (lb.size().y * 0.6).max(3.0);
                piston(b, *id, top, Vec3::NEG_Y, len, (lb.size().x * 0.3).clamp(1.0, 4.0).round(), 0.9, 0.0, k as f32 * 1.6);
            }
        } else {
            let d = (e.y * 0.2).clamp(1.0, 5.0).round();
            for sd in [-1.0f32, 1.0] {
                let x = side_at(b, sd, c.y, c.z) + sd * (d * 0.5 + 1.0);
                piston(b, 0, v3(x, c.y - e.y * 0.1, c.z + e.z * 0.3), Vec3::NEG_Z, (e.z * 0.5).max(4.0), d, 0.7, 0.8, if sd > 0.0 { 0.0 } else { std::f32::consts::PI });
            }
        }
    }
    if has(Gears) {
        let r1 = (e.y.min(e.z) * 0.3).clamp(2.0, 14.0);
        let thick = if s > 20.0 { 2.0 } else { 1.0 };
        let surf = 0.35;
        if upright {
            // Shoulder gears (big, meshing with a small one below) + hip gear.
            for sd in [-1.0f32, 1.0] {
                let y = mx.y - r1 - 0.5;
                let x = side_at(b, sd, y, c.z) + sd * (thick * 0.5 + 0.5);
                let arm_x = an.arms.iter().find(|a| a.2 == sd).map(|a| if sd > 0.0 { a.1.min.x } else { a.1.max.x });
                let x = arm_x.map_or(x, |ax| if sd > 0.0 { x.min(ax - 1.0) } else { x.max(ax + 1.0) });
                gear_train(b, 0, v3(x, y, c.z), v3(0.0, -0.7, 0.7), Vec3::X, &[r1, r1 * 0.55], thick, surf, 0.5, true);
            }
            let z = front_at(b, c.x, mn.y + e.y * 0.25);
            gear(b, 0, v3(c.x, mn.y + e.y * 0.25, z - 0.5), Vec3::Z, (e.x * 0.18).clamp(1.5, 8.0), 1.0, 1.0, surf, 0.5, true);
        } else {
            for sd in [-1.0f32, 1.0] {
                let y = c.y + e.y * 0.05;
                let z0 = c.z - e.z * 0.18;
                let x = side_at(b, sd, y, z0) + sd * (thick * 0.5);
                let radii = [r1, r1 * 0.6, r1 * 0.8];
                gear_train(b, 0, v3(x, y, z0), v3(0.0, 0.35, 1.0), Vec3::X, &radii, thick, surf, 0.6, true);
            }
            if legged && form.category() == FormCategory::Creature {
                wind_key(b, v3(c.x, top_at(b, c.x, c.z + e.z * 0.1), c.z + e.z * 0.1), (s * 0.12).clamp(2.0, 10.0));
            }
        }
    }

    // --- propulsion
    if has(Exhaust) {
        let r = (s * 0.05).clamp(1.0, 4.0);
        for sd in [-1.0f32, 1.0] {
            let y = mn.y + e.y * 0.35;
            let x = c.x + sd * e.x * 0.28;
            exhaust_pipe(b, 0, v3(x, y, back_at(b, x, y) + r * 2.0), v3(0.0, 0.2, 1.0), r, true);
        }
    }
    if has(Propeller) {
        let z = back_at(b, c.x, c.y);
        b.bar(v3(c.x, c.y, z), v3(c.x, c.y, z + 2.0), 1.5, D);
        rotor(b, 0, v3(c.x, c.y, z + 2.5), Vec3::Z, (e.x.max(e.y) * 0.45).clamp(2.5, 20.0), 3, 18.0, P);
    }
    if has(Rotor) {
        let y = top_at(b, c.x, c.z);
        let h = (s * 0.12).clamp(2.0, 8.0);
        b.cy(v3(c.x, y + h * 0.5, c.z), 1.5, h, D);
        rotor(b, 0, v3(c.x, y + h + 0.5, c.z), Vec3::Y, (e.x.max(e.z) * 0.6).clamp(4.0, 40.0), 4, 13.0, P);
    }
    if has(Jet) || has(Turbine) {
        let r = (e.y * 0.16).clamp(1.5, 8.0);
        for sd in [-1.0f32, 1.0] {
            let y = c.y + if has(Jet) { -e.y * 0.05 } else { e.y * 0.15 };
            let x = side_at(b, sd, y, c.z) + sd * (r + 0.5);
            let len = (e.z * 0.35).max(4.0);
            b.bar(v3(x - sd * r, y, c.z), v3(x - sd * (r + 1.5), y, c.z), 1.0, D);
            jet(b, 0, v3(x, y, c.z - len * 0.5), v3(x, y, c.z + len * 0.5), r);
        }
    }

    // --- superstructure
    let mut top_used_center = false;
    if has(Sails) || has(Mast) {
        let h = (s * 0.9).clamp(6.0, 60.0);
        let y = top_at(b, c.x, c.z);
        mast(b, 0, v3(c.x, y - 0.5, c.z), h, (e.x * 1.4).clamp(4.0, 40.0), if has(Sails) { if h > 12.0 { 2 } else { 1 } } else { 0 }, 0.0);
        top_used_center = true;
    }
    if has(Cockpit) {
        if upright || form == Form::Kaiju {
            let y = mn.y + e.y * 0.62;
            let z = front_at(b, c.x, y);
            b.on(0);
            canopy(b, v3(c.x, y, z), (e.x * 0.45).max(2.0), (e.y * 0.12).max(1.5), 2.5);
        } else {
            let z = mn.z + e.z * 0.25;
            let y = top_at(b, c.x, z);
            canopy(b, v3(c.x, y + 0.5, z), (e.x * 0.5).max(2.0), (e.y * 0.25).clamp(1.5, 6.0), (e.z * 0.3).max(3.0));
        }
    }
    if has(Cab) {
        let l = (e.z * 0.3).clamp(3.0, 20.0);
        let z = mx.z - l * 0.5 - e.z * 0.05;
        let y = top_at(b, c.x, z);
        let h = (e.y * 0.5).clamp(3.0, 16.0);
        cab(b, v3(c.x, y + h * 0.5 - 0.5, z), (e.x * 0.8).max(3.0), h, l);
    }
    if has(Windows) {
        let size = (e.y * 0.15).clamp(1.0, 4.0).round();
        for sd in [-1.0f32, 1.0] {
            let y = c.y + e.y * 0.1;
            let x = side_at(b, sd, y, c.z);
            window_row(b, x, y, mn.z + e.z * 0.2, mx.z - e.z * 0.2, size);
        }
    }
    let mut turret_mount: Option<PartId> = None;
    if has(Turret) {
        let w = odd((e.x * 0.5).clamp(3.0, 20.0));
        let (x, z) = if upright { (c.x + e.x * 0.3, c.z + e.z * 0.1) } else { (c.x, c.z) };
        let y = top_at(b, x, z);
        let (_, mount) = turret(b, 0, v3(x, y - 0.5, z), w, (e.z * 0.4).clamp(3.0, 20.0), (e.y * 0.2).clamp(2.0, 10.0), (s * 0.45).clamp(3.0, 30.0), odd((s * 0.05).clamp(1.0, 3.0)), has(Barrels));
        turret_mount = Some(mount);
        top_used_center |= !upright;
    }
    if has(Barrels) && turret_mount.is_none() {
        let d = odd((e.y * 0.12).clamp(1.0, 5.0));
        let len = (s * 0.35).clamp(3.0, 20.0);
        if upright {
            // Shoulder cannons.
            for sd in [-1.0f32, 1.0] {
                let p = v3(c.x + sd * e.x * 0.35, mx.y + d * 0.5, c.z);
                b.bx(p, v3(d + 2.0, d + 1.0, d + 2.0), D);
                barrel(b, p, v3(0.0, 0.1, -1.0), len, d);
            }
        } else if held {
            let z = mn.z;
            for sd in [-1.0f32, 1.0] {
                barrel(b, v3(c.x + sd * (d * 0.5 + 0.5), c.y - e.y * 0.2, z), Vec3::NEG_Z, len * 0.6, d);
            }
        } else {
            for sd in [-1.0f32, 1.0] {
                let x = c.x + sd * e.x * 0.25;
                let y = c.y - e.y * 0.1;
                barrel(b, v3(x, y, front_at(b, x, y) + 1.0), Vec3::NEG_Z, len, d);
            }
        }
    }
    if has(Drill) {
        let len = (s * 0.45).clamp(3.0, 30.0);
        let r = (e.x.min(e.y) * 0.25).clamp(1.5, 12.0);
        if let Some((id, ab, _)) = an.arms.iter().find(|a| a.2 > 0.0) {
            let base = v3(ab.center().x, ab.min.y + 1.0, ab.center().z);
            drill(b, *id, base, Vec3::NEG_Y, (ab.size().y * 0.6).max(3.0), (ab.size().x * 0.6).max(1.5));
        } else if held {
            drill(b, 0, v3(c.x, c.y, mn.z + 0.5), Vec3::NEG_Z, len, r);
        } else {
            let y = c.y - e.y * 0.05;
            drill(b, 0, v3(c.x, y, front_at(b, c.x, y) + 1.0), Vec3::NEG_Z, len, r);
        }
    }
    if has(Saw) {
        let r = (e.y * 0.35).clamp(2.0, 16.0);
        if let Some((id, ab, _)) = an.arms.iter().find(|a| a.2 < 0.0) {
            let cc = v3(ab.min.x - 0.5, ab.min.y + r * 0.3, ab.center().z);
            saw(b, *id, cc, Vec3::X, r * 0.8);
        } else {
            let y = c.y;
            let z = front_at(b, c.x + e.x * 0.3, y) - r * 0.6;
            b.bar(v3(c.x + e.x * 0.3, y, z + r * 0.6 + 1.0), v3(c.x + e.x * 0.3, y, z), 1.5, D);
            saw(b, 0, v3(c.x + e.x * 0.3 + 1.0, y, z), Vec3::X, r);
        }
    }
    if has(CraneArm) {
        let z = mx.z - e.z * 0.2;
        crane(b, 0, v3(c.x, top_at(b, c.x, z) - 0.5, z), (s * 0.8).clamp(6.0, 40.0));
    }
    if has(Chains) {
        let size = (s * 0.08).clamp(1.5, 5.0);
        for sd in [-1.0f32, 1.0] {
            let y = mx.y - e.y * 0.2;
            let x = side_at(b, sd, y, c.z) + sd * 0.5;
            chain(b, 0, v3(x, y, c.z), 4, size, sd > 0.0);
        }
    }
    if has(Antenna) {
        let (x, z) = (c.x - e.x * 0.3, mx.z - e.z * 0.15);
        let head_top = an.head.as_ref().map(|(id, hb)| (*id, v3(hb.center().x + hb.size().x * 0.25, hb.max.y - 0.5, hb.center().z)));
        match head_top {
            Some((id, p)) if legged => {
                antenna(b, id, p, (s * 0.25).clamp(3.0, 14.0));
            }
            _ => {
                antenna(b, 0, v3(x, top_at(b, x, z) - 0.5, z), (s * 0.35).clamp(3.0, 24.0));
            }
        }
    }
    if has(Dish) {
        let (x, z) = if top_used_center { (c.x + e.x * 0.3, c.z + e.z * 0.25) } else { (c.x, c.z) };
        dish(b, 0, v3(x, top_at(b, x, z) - 0.5, z), (s * 0.15).clamp(2.0, 12.0));
    }
    if has(Headlight) {
        if let Some((id, hb)) = an.head {
            b.on(id);
            headlight(b, v3(hb.center().x, hb.max.y - 0.5, hb.min.z + 0.5), v3(0.0, 0.2, -1.0), (hb.size().x * 0.3).clamp(1.5, 5.0));
            b.on(0);
        } else {
            for sd in [-1.0f32, 1.0] {
                let x = c.x + sd * e.x * 0.3;
                let y = c.y + e.y * 0.1;
                headlight(b, v3(x, y, front_at(b, x, y) - 0.2), Vec3::NEG_Z, (s * 0.08).clamp(1.5, 5.0));
            }
        }
    }
    if has(Plow) {
        let z = front_at(b, c.x, mn.y + e.y * 0.2);
        plow(b, c.x, z - 0.5, mn.y + 0.5, mn.y + e.y * 0.45, (e.x + 2.0).min(60.0));
    }
    if has(Fins) {
        let h = (s * 0.3).clamp(2.0, 20.0);
        let z = mx.z - e.z * 0.2;
        let y = top_at(b, c.x, z);
        fin(b, v3(c.x, y - 0.5, z), v3(c.x, y + h, z + h * 0.5), (e.z * 0.25).max(2.0), Vec3::X);
        for sd in [-1.0f32, 1.0] {
            let x = side_at(b, sd, c.y, z);
            fin(b, v3(x, c.y, z), v3(x + sd * h * 0.8, c.y - h * 0.3, z + h * 0.5), (e.z * 0.2).max(2.0), Vec3::Y);
        }
    }
    if has(Spikes) {
        let n = ((e.z / 4.0).round() as usize).clamp(2, 8);
        let len = (s * 0.14).clamp(1.5, 10.0);
        for i in 0..n {
            let z = mn.z + e.z * (i as f32 + 0.5) / n as f32;
            let y = top_at(b, c.x, z);
            b.cone_to(v3(c.x, y - 0.5, z), v3(c.x, y + len, z + len * 0.3), (len * 0.5).max(1.5), C);
        }
        for (id, ab, sd) in &an.arms {
            b.on(*id);
            let p = v3(ab.center().x + sd * ab.size().x * 0.4, ab.max.y - 1.0, ab.center().z);
            b.cone_to(p, p + v3(sd * len * 0.6, len * 0.6, 0.0), (len * 0.4).max(1.5), C);
        }
        b.on(0);
    }
    if has(Armor) {
        for sd in [-1.0f32, 1.0] {
            let y = c.y;
            let x = side_at(b, sd, y, c.z);
            plate(b, v3(x + sd * 0.5, y, c.z), v3(1.0, (e.y * 0.55).max(2.0), (e.z * 0.7).max(2.0)), Vec3::X * sd, A);
        }
        for (id, ab, sd) in &an.arms {
            b.on(*id);
            let w = ab.size().x + 2.0;
            plate(b, v3(ab.center().x + sd * 0.5, ab.max.y + 0.5, ab.center().z), v3(w, 2.0, ab.size().z + 2.0), Vec3::Y, A);
        }
        b.on(0);
    }
    if has(Bell) {
        let (x, z) = (c.x - e.x * 0.2, c.z - e.z * 0.25);
        let sz = (s * 0.1).clamp(2.0, 8.0);
        bell(b, 0, v3(x, top_at(b, x, z) - 0.5, z), sz);
    }
    if has(Cage) {
        let y0 = top_at(b, c.x, c.z) - 0.5;
        let h = (e.y * 0.5).clamp(3.0, 20.0);
        let (hx, hz) = ((e.x * 0.4).max(2.0), (e.z * 0.35).max(2.0));
        let n = 3;
        for i in 0..=n {
            let f = i as f32 / n as f32;
            for (x, z) in [(c.x - hx + 2.0 * hx * f, c.z - hz), (c.x - hx + 2.0 * hx * f, c.z + hz), (c.x - hx, c.z - hz + 2.0 * hz * f), (c.x + hx, c.z - hz + 2.0 * hz * f)] {
                b.bar(v3(x, y0, z), v3(x, y0 + h, z), 1.0, A);
            }
        }
        b.bx(v3(c.x, y0 + h, c.z), v3(2.0 * hx + 1.0, 1.0, 2.0 * hz + 1.0), D);
        b.bx(v3(c.x, y0 + h + 1.0, c.z), Vec3::ONE, C);
    }
    b.on(0);
}

fn set_last_puff_per_m(b: &mut B, per_m: f32) {
    if let Some(p) = b.machine.puffs.last_mut() {
        p.per_m = per_m;
    }
}

/// Clockwork wind-up key on a creature's back (ticks round).
fn wind_key(b: &mut B, base: Vec3, s: f32) {
    b.cy(base + Vec3::Y * 0.5, 2.0, 1.0, D);
    let pivot = base + Vec3::Y * 1.0;
    let id = b.mpart(0, pivot, PartRole::Other, MotionSpec::Spin { axis: Vec3::Y, sign: 1.0, rate: 1.4, surf: 0.0, roll: 0.0, radius: 0.0, tick: true });
    b.cy(pivot + Vec3::Y * (s * 0.3), 1.0, s * 0.6, A);
    let hub = pivot + Vec3::Y * (s * 0.75);
    for sd in [-1.0f32, 1.0] {
        b.el(hub + Vec3::X * (sd * s * 0.4), v3(s * 0.7, s * 0.55, 1.0), A);
        b.el(hub + Vec3::X * (sd * s * 0.4), v3(s * 0.35, s * 0.25, 1.4), D);
    }
    b.bx(hub, v3(1.0, 1.0, 2.0), C);
    let _ = id;
    b.on(0);
}
