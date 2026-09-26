//! `--view summon [--shots a,b,c --screenshot out.png]`: a shouted cast — "TANK!!" erupts from the
//! caster's ring, and its letter voxels stream into the ring beam and land as the tank's summon
//! bricks (bd_fx shout ↔ `ConstructAssembly` hook; the game drives the same animator in
//! bd_game `present/construct_anim.rs`). Repeats every 4 s from t = 4 s (real time).
//! `--shots` takes screenshots at those seconds after the cast (`<stem>_<i>.png`).

use super::*;
use bd_shapes::behaviours::{AnimDrive, AnimEvent, ConstructAnimator};

const FIRST: f32 = 4.0;
const PERIOD: f32 = 4.0;

#[derive(Default)]
pub struct State {
    t: f32,
    cast_at: Option<f32>,
    casts: u32,
    target: Option<Entity>,
    anim: Option<ConstructAnimator>,
    shots: Vec<f32>,
    shot: usize,
    parsed: bool,
}

/// Bevy → bd_shapes (glam) vector.
fn g(v: Vec3) -> glam::Vec3 {
    glam::Vec3::from_array(v.to_array())
}

fn ring() -> Vec3 {
    Vec3::new(5.5, 1.5, 5.0)
}

#[allow(clippy::too_many_arguments)]
pub fn run(
    mut commands: Commands,
    real: Res<Time<Real>>,
    args: Res<Args>,
    models: Res<Models>,
    mut st: Local<State>,
    mut q: Query<(&Transform, &mut PartOverlay, &mut ConstructAssembly)>,
) {
    let st = &mut *st;
    if !st.parsed {
        st.parsed = true;
        let a: Vec<String> = std::env::args().collect();
        st.shots = a
            .windows(2)
            .find(|w| w[0] == "--shots")
            .map(|w| w[1].split(',').filter_map(|s| s.trim().parse().ok()).collect())
            .unwrap_or_default();
    }
    let dt = real.delta_secs().min(0.1);
    st.t += dt;
    let due = FIRST + st.casts as f32 * PERIOD;
    if st.t >= due {
        st.casts += 1;
        if let Some(e) = st.target.take() {
            commands.entity(e).try_insert(ConstructEnd::Remove);
        }
        let d = models.get("tank");
        let mut vis = ConstructVisual::new(d.model.clone(), Corps::Willpower.color()).with_key(d.key.clone()).forming_from(ring());
        vis.seed = 7 + st.casts;
        let mut anim = ConstructAnimator::new(&d.model, 7);
        anim.event(AnimEvent::Summon);
        let e = commands
            .spawn((
                Name::new("SummonDemoTank"),
                vis,
                AutoForm::new(0.3),
                PartOverlay { locals: anim.locals().to_vec() },
                ConstructAssembly { ring: ring(), ..default() },
                Transform::from_rotation(Quat::from_rotation_y(0.5)),
            ))
            .id();
        let mut s = Shout::new("TANK!!".into(), Corps::Willpower.color(), 1.7, false, ring());
        s.head = ring() + Vec3::new(-1.5, 1.4, 0.0);
        s.toward = Vec3::Y;
        s.target = Some(e);
        s.seed = st.casts as u64;
        commands.spawn(s);
        st.target = Some(e);
        st.anim = Some(anim);
        st.cast_at = Some(st.t);
    }
    // Drive the animator exactly like bd_game's construct_anim does.
    if let (Some(e), Some(anim)) = (st.target, st.anim.as_mut())
        && let Ok((tf, mut ov, mut asm)) = q.get_mut(e)
    {
        let t = st.t - st.cast_at.unwrap_or(st.t);
        anim.hold_stages(asm.hold_request);
        let inv = tf.compute_affine().inverse();
        let ring_m = inv.transform_point3(ring());
        anim.update(&AnimDrive { time: t, ring: Some(g(ring_m)), ..Default::default() }, dt);
        ov.locals.clear();
        ov.locals.extend_from_slice(anim.locals());
        let a = anim.assembly();
        let sched = anim.stage_schedule();
        let world = |p: glam::Vec3| tf.transform_point(Vec3::from_array(p.to_array()));
        *asm = ConstructAssembly {
            progress: if a.active { a.progress } else { 1.0 },
            stage: a.stage,
            stages: a.stages,
            ring: ring(),
            next_world: a.next.map(|(p, _)| world(p)),
            next_in: a.next.map_or(0.0, |(_, s)| s),
            total: anim.assembly_seconds(),
            slots: sched.map(|(p, _, _)| world(p)),
            land_in: sched.map(|(_, s, _)| s),
            radius: sched.map(|(_, _, r)| r),
            hold_request: 0.0,
        };
        // Screenshot sequence (first cast only).
        if st.casts == 1
            && let (Some(path), Some(&at)) = (args.screenshot.as_ref(), st.shots.get(st.shot))
            && t >= at
        {
            let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "summon".into());
            let out = path.with_file_name(format!("{stem}_{}.png", st.shot + 1));
            info!("summon demo: shot {} at +{t:.3}s (hold {:.3}s) -> {}", st.shot + 1, anim.assembly_seconds(), out.display());
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(out));
            st.shot += 1;
        }
    }
}
