//! Presentation of the sim (SIM engineer): everything the player sees and hears of
//! `bd_sim::World`, driven only by [`crate::sim_bridge::GameEvents`] + [`crate::sim_bridge::WorldViews`]
//! (+ the replicated snapshot in `ViewKeys`), so it works identically in practice, host and client.
//!
//! | module | what |
//! |---|---|
//! | [`sound`] | positional one-shots from the SFX manifest (pitch randomised), per-entity loops, mix duck, KO streak steps |
//! | [`terrain`] | render-mirror terrain edits (carve / extract / restore / arena reset) + debris with velocity inheritance + falling chunks |
//! | [`impact`] | hits → `Impact` juice, render-side hit-stop, crit snap, crosshair flare, KO shatter + streak + accents, wall splat, onomatopoeia, explosions |
//! | [`constructs`] | every construct as a bd_fx `ConstructVisual` (models built off-thread, cached by `ShapeKey`), anim state from the sim, forming / shatter / clash / creature telegraphs / held swings / transforms / loops |
//! | [`players`] | other players as costumes (bd_shapes) in corps colour, run/slide/tether/swing/cast/tumble, tether ropes, diegetic health |
//! | [`powers`] | fruits, rubber arm, afterimages, time-stop grey, ROOM dome, portals, links (chains), fields |
//! | [`guns`] | muzzle light, tracers/beams, casings, projectiles, bullet impacts |
//! | [`camera3p`] | over-the-shoulder rig (piloting / comfort), crosshair ray + projectile aim correction |
//! | [`local`] | local player: respawn facing, death cam, hop chain feedback |
//! | [`water`] | city-map sea: animated water plane + splashes |
//! | [`space`] | Orbit: space sky (stars, planet, moon), far silhouettes, zero-g dust, space / re-entry KOs, speed lines |
//! | [`soak`] / [`record`] | `--soak-view` chaos viewer, `--record` frame capture (PNG sequence or piped to ffmpeg) |
//! | [`director`] | spectator director camera (interest scoring, occlusion-safe framing, slow-mo) |

pub mod ads;
pub mod air_moves; // AIR JUMP / DODGE: hard-light step burst, dodge streak, SFX / camera hooks
pub mod animated_terrain;
pub mod body_motion; // ANIMATION PASS: player body motion / actions / channels + body effects (anim.ron)
pub mod taunt; // TAUNT & VOICE TELL: taunt key / push-to-talk bits, local prediction, tells, voice rings, 1P gesture
pub mod camera3p;
pub mod camera_shots; // CAMERA: BD_CAM_SHOT dev screenshots
pub mod valley; // VALLEY OF THE END: water sheets, falls veil, mist, rainbow, water-walk ripples
pub mod cybertron; // CYBERTRON: transforming panels (map movers), telegraph light + panel sounds
pub mod casino; // HIGH ROLLER: slot reels / levers, roulette house spin, glitter, pie splat (map interactables)
pub mod charge;
pub mod construct_anim;
pub mod constructs;
pub mod director;
pub mod extend; // WUKONG: telescoping poles (Mechanic::Extends): continuous iron/gold pole, thrust, pillar, vault, needle dismiss
pub mod guns;
pub mod impact;
pub mod impact_frames; // IMPACT FRAMES: anime freeze frames (post pass), hit reactions by weight (docs/impact/API.md)
pub mod legend; // LEGENDARY: legendary move visuals (storms, recall, waves, chains, gun specials), poses, BD_LEGEND_TEST
pub mod legend_sound; // LEGENDARY: sound signatures + juice of the legendary move rules (recall, storms, sabers, guns…)
pub mod local;
pub mod hyperline; // HYPERLINE: the endless city shaft, ring gates, streaks, sparks (visual only)
pub mod lookout; // THE LOOKOUT: the cloud sea + cumulus far below the sky temple (visual only)
pub mod giant; // GIANT'S ROOM: sunbeam through the window + dust motes
pub mod giants_fx; // GIANTS: weak-point glow, warnings, shockwave rings, severed limbs, topple slam, command ping
pub mod mirror; // MIRRORFOLD: folded-city backdrop, mirrored void, rift
pub mod outline; // OUTLINE: player outlines (inverted hull) for readability in chaos
pub mod players;
pub mod portal_bench; // PORTALS: `--scenario portals` (perf + screenshots)
pub mod portal_cross; // PORTALS: seamless crossing (view turns, eye offset, camera fold, 3P boom through)
pub mod portal_view; // PORTALS: see-through windows (2 pre-allocated portal cameras, budgeted, warmed up)
pub mod powers;
pub mod presence; // PRESENCE: the construct presence grammar (gather / manifest / material / presence / exit by tier)
pub mod record;
pub mod shout;
pub mod signature; // SIGNATURE: cut-in DANGER alert, domain grade, strike markers, audio cues
pub mod signature_fx; // SIGNATURE: fusion windup (Merge), erasing orbs (Hollow Purple)
pub mod palm_beam; // SIGNATURE: palm energy (Form::Hands): the orb between the hands + the sustained beam
pub mod domain; // SIGNATURE: domain expansions (ink tide, eyelid barrier, black orb, the inside world, sure-hit pulses)
pub mod pickups; // POWER-UPS: map pickups (beacon, heart / orb / star), the loaded hand, their sounds
pub mod first_call; // FIRST CALL + BURN ON DEATH: sigils, the rival clash, echoes / ash, Tab collection, hotbar states
pub mod soak;
pub mod sound;
pub mod space;
pub mod summon_ping;
pub mod surf; // SURF: map surf lines drawn as hard light, surf wind rush
pub mod terrain;
pub mod tracking; // TRACKING: ring compass, beacons, reveal pulses, speed trails, lunge glow, hook ropes
pub mod water;
pub mod weapon_feel; // WEAPON FEEL: per-class swing sounds, trails, camera, 3P body (assets/tuning/weapons.ron `feel`)
pub mod transformer_feel; // TRANSFORMER FEEL: transform cascade + camera kick, robot footfalls, engine voices, drift, robot combat / beast sounds

use crate::sim_bridge::{ActiveWorld, BridgeSet, SimRole};
use crate::tick::FrameSet;
use bd_core::corps::Corps;
use bd_core::PlayerId;
use bevy::prelude::*;

/// Ordering inside `FrameSet::Present` for presentation systems.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum PresentSet {
    /// Read `GameEvents` → messages / state (runs first).
    Events,
    /// Sync entities to `WorldViews` (transforms, anim layers).
    Sync,
}

pub struct PresentPlugin;

impl Plugin for PresentPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(bd_fx::BdFxPlugin)
            .configure_sets(Update, (PresentSet::Events, PresentSet::Sync).chain().in_set(FrameSet::Present).after(BridgeSet::Views))
            .configure_sets(Update, PresentSet::Events.before(crate::debris::DebrisSimSet))
            .add_plugins((
                sound::SoundPlugin,
                terrain::TerrainPlugin,
                impact::ImpactPlugin,
                constructs::ConstructsPlugin,
                players::PlayersPlugin,
                powers::PowersPlugin,
                guns::GunsPlugin,
                camera3p::Camera3pPlugin,
                local::LocalPlugin,
                soak::SoakPlugin,
                director::DirectorPlugin,
                record::RecordPlugin,
            ))
            .add_plugins((water::WaterPlugin, space::SpacePlugin, shout::ShoutPlugin))
            .add_plugins(charge::ChargePlugin) // CHARGE: aura, world dim, audio cues
            .add_plugins(summon_ping::SummonPingPlugin) // off-screen summon ping + preview window
            .add_plugins(construct_anim::ConstructAnimPlugin) // role x event construct animation
            .add_plugins(tracking::TrackingPlugin) // TRACKING: diegetic tracker + lunge / hook presentation
            .add_plugins(outline::OutlinePlugin) // OUTLINE: player outlines
            .add_plugins(air_moves::AirMovesPlugin) // AIR JUMP / DODGE: hard-light step + streak
            .add_plugins(body_motion::BodyMotionPlugin) // ANIMATION PASS: body motion layers + body fx
            .add_plugins(taunt::TauntPlugin) // TAUNT & VOICE TELL: wordless taunts + the voice-cast tell
            .add_plugins(mirror::MirrorPlugin) // MIRRORFOLD: folded-city backdrop
            .add_plugins(hyperline::HyperlinePlugin) // HYPERLINE: the world streams past the train
            .add_plugins(lookout::LookoutPlugin) // THE LOOKOUT: the sea of clouds below the temple
            .add_plugins(giant::GiantPlugin) // GIANT'S ROOM: sunbeam + dust motes
            .add_plugins(giants_fx::GiantsFxPlugin) // GIANTS: kaiju you fight (weak points, shockwaves, severed limbs)
            .add_plugins(surf::SurfPlugin) // SURF: hard-light surf lines + wind rush
            .add_plugins(cybertron::CybertronPlugin) // CYBERTRON: map movers (no-op on maps without them)
            .add_plugins(casino::CasinoPlugin) // HIGH ROLLER: map interactables (no-op on maps without them)
            .add_plugins(valley::ValleyPlugin) // VALLEY: water sheets + falls + mist (no-op without water sheets)
            .add_plugins((signature::SignaturePlugin, signature_fx::SignatureFxPlugin, palm_beam::PalmBeamPlugin, domain::DomainPlugin)) // SIGNATURE: alerts, grades, markers, fusion / orb fx, palm beams, domains
            .add_plugins(impact_frames::ImpactFramesPlugin) // IMPACT FRAMES: freeze frames + hit reactions
            .add_plugins(presence::PresencePlugin) // PRESENCE: every construct's gravitas, by tier
            .add_plugins(pickups::PickupsPlugin) // POWER-UPS: map pickups (no-op while the mode is off)
            .add_plugins(first_call::FirstCallPlugin) // FIRST CALL + BURN ON DEATH (no-op while both rules are off)
            .add_plugins(extend::ExtendPlugin) // WUKONG: telescoping poles
            .add_plugins(legend::LegendPlugin) // LEGENDARY: legendary move visuals + poses
            .add_plugins(legend_sound::LegendSoundPlugin) // LEGENDARY: move sounds + juice
            .add_plugins(weapon_feel::WeaponFeelPlugin) // WEAPON FEEL: weapon classes sound / look / feel different
            .add_plugins(transformer_feel::TransformerFeelPlugin) // TRANSFORMER FEEL: any transformer feels awesome
            .add_plugins((portal_view::PortalViewPlugin, portal_cross::PortalCrossPlugin)); // PORTALS: see-through + seamless crossing
        app.add_systems(Update, (perf_experiments, animated_terrain::dev_graft));
        app.add_systems(PostUpdate, shot_camera.after(crate::camera::CameraUpdateSet).after(camera3p::place_camera).before(bevy::transform::TransformSystems::Propagate));
        use crate::warmup::WarmupAppExt;
        app.add_warmup(|w: &mut World| {
            for e in bd_fx::spawn_warmup(w) {
                w.entity_mut(e).insert(crate::warmup::WarmupItem);
            }
        });
    }
}

/// `BD_PERF_OFF=audio,lights,ono,shatter` — perf experiments (disable presentation channels).
pub fn perf_off(what: &str) -> bool {
    static V: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    V.get_or_init(|| std::env::var("BD_PERF_OFF").unwrap_or_default()).split(',').any(|w| w == what)
}

/// Linear corps colour.
pub fn corps_rgb(c: Corps) -> [f32; 3] {
    c.color()
}

pub fn rgb_color(c: [f32; 3]) -> Color {
    Color::linear_rgb(c[0], c[1], c[2])
}

/// Client role: the local player's own cosmetic moments were already played from local input.
pub fn replayed_locally(active: &ActiveWorld, owner: PlayerId) -> bool {
    active.role == SimRole::Client && active.local == Some(owner)
}

#[inline]
pub fn bv(v: bd_core::Vec3) -> Vec3 {
    Vec3::from_array(v.to_array())
}

#[inline]
pub fn bq(q: bd_core::Quat) -> Quat {
    Quat::from_array(q.to_array())
}

#[inline]
pub fn gv(v: Vec3) -> bd_core::Vec3 {
    bd_core::Vec3::from_array(v.to_array())
}

/// `BD_SHOT_CAM=x,y,z,yaw_deg,pitch_deg` — dev screenshots: pin the gameplay camera (look checks
/// such as Miami's neon, animated terrain). Never set in play.
fn shot_camera(mut cams: Query<&mut Transform, With<crate::camera::PlayerCamera>>) {
    static V: std::sync::OnceLock<Option<[f32; 5]>> = std::sync::OnceLock::new();
    let Some(v) = V.get_or_init(|| {
        let s = std::env::var("BD_SHOT_CAM").ok()?;
        let f: Vec<f32> = s.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        (f.len() == 5).then(|| [f[0], f[1], f[2], f[3], f[4]])
    }) else {
        return;
    };
    for mut t in &mut cams {
        t.translation = Vec3::new(v[0], v[1], v[2]);
        t.rotation = Quat::from_euler(EulerRot::YXZ, v[3].to_radians(), v[4].to_radians(), 0.0);
    }
}

fn perf_experiments(ono: Option<ResMut<bd_fx::OnomatopoeiaSettings>>, fx: Option<ResMut<bd_fx::FxSettings>>) {
    if perf_off("ono") && let Some(mut o) = ono {
        o.mode = bd_fx::OnoMode::Off;
    }
    if perf_off("particles") && let Some(mut f) = fx {
        f.particle_budget = 0.0;
        f.shatter_max_cubes = 1;
    }
}
