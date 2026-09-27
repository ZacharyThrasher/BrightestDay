//! **COSTUME TINTS** — the player's costume reacts to the forms they hold ([`AscendTints`], from
//! `powers.ron` `ascend: (hair_tint, eye_glow, skin_tint, toon)`): Super Saiyan hair goes gold (Blue:
//! blue), Ultra Instinct silver, the Sharingan / Rinnegan tint the eyes, Gear Fifth goes bright,
//! white and toon.
//!
//! * No re-mesh: painted costumes tag their hair / eye / skin voxels at bake time
//!   (`bd_shapes::costume::region`, carried by `bd_fx::VoxelColors::regions` into the mesh), and the
//!   construct shader recolours those voxels from [`bd_fx::CostumeTint`] on the body's
//!   [`ConstructVisual`] — a per-frame uniform write.
//! * Fades in over ~0.35 s when a form starts and back out over ~0.5 s when it ends (KO, drained,
//!   back to base): the painted colours return exactly (amount 0 = untouched).
//! * First-person hands (`viewmodel.rs`) take the skin tint and the toon brightening
//!   ([`local_hand_tint`]).

use super::super::PresentSet;
use super::ascend::{AscendTint, AscendTints, lin};
use crate::present::players::PlayerBody;
use crate::tick::FrameSet;
use bd_core::PlayerId;
use bd_fx::{ConstructVisual, CostumeTint};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

/// Seconds to fade a form's tint in / out.
pub const FADE_IN: f32 = 0.35;
pub const FADE_OUT: f32 = 0.5;

/// Per player: the last tint a form asked for + how much of it shows (0..1).
#[derive(Resource, Default, Debug)]
pub struct CostumeFades(pub HashMap<PlayerId, (AscendTint, f32)>);

pub struct CostumeTintPlugin;

impl Plugin for CostumeTintPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AscendTints>()
            .init_resource::<CostumeFades>()
            .add_systems(Update, apply_costume_tints.in_set(FrameSet::Present).after(PresentSet::Sync));
    }
}

/// Step one player's fade toward `target` (`None` = no form: fade out, keep the colours meanwhile).
pub fn step_fade(fade: &mut (AscendTint, f32), target: Option<AscendTint>, dt: f32) {
    match target {
        Some(t) => {
            fade.0 = t;
            fade.1 = (fade.1 + dt / FADE_IN).min(1.0);
        }
        None => fade.1 = (fade.1 - dt / FADE_OUT).max(0.0),
    }
}

/// What the body shader gets for a tint shown at `k` (sRGB tints → linear; `k` 0 = nothing).
pub fn costume_tint(t: &AscendTint, k: f32) -> CostumeTint {
    if k <= 1e-3 {
        return CostumeTint::default();
    }
    let lane = |c: Option<[f32; 3]>| c.map(|c| (lin(c), k));
    CostumeTint { hair: lane(t.hair), eyes: lane(t.eyes), skin: lane(t.skin), toon: if t.toon { k } else { 0.0 } }
}

fn apply_costume_tints(time: Res<Time<Real>>, tints: Res<AscendTints>, mut fades: ResMut<CostumeFades>, mut bodies: Query<(&PlayerBody, &mut ConstructVisual)>) {
    let dt = time.delta_secs().min(0.1);
    for (b, mut cv) in &mut bodies {
        let target = tints.0.get(&b.id).copied();
        if target.is_none() && !fades.0.contains_key(&b.id) {
            if !cv.costume.is_none() {
                cv.costume = CostumeTint::default();
            }
            continue;
        }
        let f = fades.0.entry(b.id).or_insert((AscendTint::default(), 0.0));
        step_fade(f, target, dt);
        let want = costume_tint(&f.0, f.1);
        if f.1 <= 0.0 && target.is_none() {
            fades.0.remove(&b.id);
        }
        if cv.costume != want {
            cv.costume = want;
        }
    }
}

/// First-person hands: (base colour multiplier, emissive add) for the local player's current tint
/// (`None` = untinted). Skin tint → the hands take it; toon → bright, near-white.
pub fn local_hand_tint(fades: &CostumeFades, local: Option<PlayerId>) -> Option<([f32; 3], [f32; 3])> {
    let (t, k) = local.and_then(|id| fades.0.get(&id)).copied()?;
    if k <= 1e-3 {
        return None;
    }
    let mut mul = [1.0f32; 3];
    let mut add = [0.0f32; 3];
    if let Some(s) = t.skin.map(lin) {
        for i in 0..3 {
            mul[i] = 1.0 + (s[i] * 2.2 - 1.0).clamp(-0.85, 0.6) * 0.7 * k;
        }
    }
    if t.toon {
        for i in 0..3 {
            mul[i] = mul[i] + (1.35 - mul[i]) * k;
            add[i] = 0.25 * k;
        }
    }
    Some((mul, add))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ssj() -> AscendTint {
        AscendTint { hair: Some([1.0, 0.85, 0.2]), eyes: Some([0.3, 0.8, 0.7]), skin: None, toon: false }
    }

    #[test]
    fn a_form_tints_the_costume_and_reverts_cleanly() {
        let mut f = (AscendTint::default(), 0.0);
        // Fades in.
        step_fade(&mut f, Some(ssj()), 0.1);
        let mid = costume_tint(&f.0, f.1);
        let k = mid.hair.expect("hair tinted").1;
        assert!(k > 0.2 && k < 0.5, "partway in: {k}");
        for _ in 0..10 {
            step_fade(&mut f, Some(ssj()), 0.1);
        }
        let full = costume_tint(&f.0, f.1);
        let (gold, k) = full.hair.unwrap();
        assert_eq!(k, 1.0);
        assert!(gold[0] > gold[2] * 3.0, "gold, linear: {gold:?}");
        assert!(full.eyes.is_some() && full.skin.is_none() && full.toon == 0.0);
        // The form ends: fades out, the colours hold while fading, then nothing at all.
        step_fade(&mut f, None, 0.1);
        let out = costume_tint(&f.0, f.1);
        assert!(out.hair.is_some_and(|h| h.1 < 1.0 && h.1 > 0.5));
        for _ in 0..10 {
            step_fade(&mut f, None, 0.1);
        }
        assert_eq!(costume_tint(&f.0, f.1), CostumeTint::default(), "reverted");
        assert!(costume_tint(&f.0, f.1).is_none());
    }

    #[test]
    fn gear_fifth_is_toon_and_the_gpu_lanes_carry_it() {
        let g5 = AscendTint { hair: Some([1.0, 1.0, 1.0]), eyes: Some([1.0, 0.85, 0.3]), skin: None, toon: true };
        let t = costume_tint(&g5, 1.0);
        let [hair, eyes, skin, style] = t.pack();
        assert_eq!(hair[3], 1.0);
        assert!(hair[0] > 0.99 && eyes[3] == 1.0 && skin[3] == 0.0);
        assert_eq!(style[0], 1.0, "toon lane");
        assert_eq!(CostumeTint::default().pack(), [[0.0; 4]; 4], "untinted = the painted colours");
    }

    #[test]
    fn hands_follow_the_local_players_form() {
        let mut fades = CostumeFades::default();
        let me = PlayerId(1);
        assert!(local_hand_tint(&fades, Some(me)).is_none());
        fades.0.insert(me, (AscendTint { skin: Some([0.3, 0.08, 0.08]), ..default() }, 1.0));
        let (mul, _) = local_hand_tint(&fades, Some(me)).unwrap();
        assert!(mul[0] > mul[1], "reddened: {mul:?}");
        fades.0.insert(me, (AscendTint { toon: true, ..default() }, 1.0));
        let (mul, add) = local_hand_tint(&fades, Some(me)).unwrap();
        assert!(mul.iter().all(|m| *m > 1.2) && add[0] > 0.0, "bright toon hands");
        assert!(local_hand_tint(&fades, Some(PlayerId(2))).is_none());
    }
}
