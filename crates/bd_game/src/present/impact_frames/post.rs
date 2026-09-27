//! The impact-frame post pass: one `FullscreenMaterial` (`impact_frame.wgsl`) on the player camera,
//! after bloom, before tonemapping (HDR in, so the burned-in silhouettes key off the bodies' hot
//! emissive). The main-world component always sits on the camera; it is only EXTRACTED while a frame
//! plays (`strength > 0`), so the render pass (and its bind group) doesn't exist when idle.

use bevy::asset::{load_internal_asset, uuid_handle};
use bevy::core_pipeline::fullscreen_material::{FullscreenMaterial, FullscreenMaterialPlugin};
use bevy::ecs::query::QueryItem;
use bevy::ecs::schedule::ScheduleConfigs;
use bevy::ecs::system::BoxedSystem;
use bevy::prelude::*;
use bevy::render::extract_component::ExtractComponent;
use bevy::render::render_resource::ShaderType;
use bevy::render::sync_component::SyncComponent;
use bevy::shader::ShaderRef;

pub const IMPACT_FRAME_SHADER: Handle<Shader> = uuid_handle!("b7d4a2c9-3e61-4f8a-9c05-1d2e7f6a8b34");

/// Uniform of the pass (see `impact_frame.wgsl` for the meaning of each lane).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, ShaderType)]
pub struct ImpactFramePost {
    /// center uv xy, bubble radius (screen-height units; 0 = full screen), bubble feather.
    pub a: Vec4,
    /// strength, phase (0..1 hold, 1..2 release), zoom factor (≥ 1), aspect.
    pub b: Vec4,
    /// ink rgb (linear), invert (0/1).
    pub ink: Vec4,
    /// paper rgb (linear), threshold.
    pub paper: Vec4,
    /// accent rgb (linear), chromatic split (screen-height fraction).
    pub accent: Vec4,
    /// lines, line density, lightning, crack.
    pub fx1: Vec4,
    /// halftone, glitch, brush, seed.
    pub fx2: Vec4,
    /// screen-space hit dir xy, hot luminance key, lightning ink core (0/1).
    pub fx3: Vec4,
    /// vignette, bubble ring (0..1 progress; < 0 = none), exposure, contrast (1 = full two-tone).
    pub fx4: Vec4,
    /// levels, softness, edges, panel.
    pub fx5: Vec4,
}

impl ImpactFramePost {
    pub fn active(&self) -> bool {
        self.b.x > 0.0
    }
}

impl SyncComponent for ImpactFramePost {
    type Target = ImpactFramePost;
}

impl ExtractComponent for ImpactFramePost {
    type QueryData = &'static ImpactFramePost;
    type QueryFilter = ();
    type Out = ImpactFramePost;

    fn extract_component(item: QueryItem<'_, '_, Self::QueryData>) -> Option<Self::Out> {
        // Idle: no render-world component → no pass, no bind group, no cost.
        item.active().then_some(*item)
    }
}

impl FullscreenMaterial for ImpactFramePost {
    fn fragment_shader() -> ShaderRef {
        IMPACT_FRAME_SHADER.into()
    }

    fn schedule_configs(system: ScheduleConfigs<BoxedSystem>) -> ScheduleConfigs<BoxedSystem> {
        system
            .in_set(bevy::core_pipeline::Core3dSystems::PostProcess)
            .after(bevy::post_process::bloom::bloom)
            .before(bevy::core_pipeline::tonemapping::tonemapping)
    }
}

pub fn build(app: &mut App) {
    load_internal_asset!(app, IMPACT_FRAME_SHADER, "impact_frame.wgsl", Shader::from_wgsl);
    app.add_plugins(FullscreenMaterialPlugin::<ImpactFramePost>::default());
    debug_build(app);
}

/// `BD_IF_DEBUG=1`: log what the render world has for the pass (diagnostics).
pub fn debug_build(app: &mut App) {
    if std::env::var_os("BD_IF_DEBUG").is_none() {
        return;
    }
    use bevy::core_pipeline::fullscreen_material::FullscreenMaterialPipelineId;
    use bevy::render::extract_component::DynamicUniformIndex;
    use bevy::render::{Render, RenderApp, RenderSystems};
    fn probe(
        q: Query<(Entity, Option<&ImpactFramePost>, Option<&DynamicUniformIndex<ImpactFramePost>>, Option<&FullscreenMaterialPipelineId>, Option<&bevy::render::view::ViewTarget>)>,
        cache: Res<bevy::render::render_resource::PipelineCache>,
        mut n: Local<u32>,
    ) {
        *n += 1;
        for (e, p, idx, pid, vt) in &q {
            if p.is_some() && *n % 5 == 0 {
                let state = pid.map(|id| format!("{:?}", cache.get_render_pipeline_state(id.0)));
                info!("IF probe {e:?}: post={} idx={} pipeline={:?} target={}", p.is_some(), idx.is_some(), state, vt.is_some());
            }
        }
    }
    if let Some(r) = app.get_sub_app_mut(RenderApp) {
        r.add_systems(Render, probe.in_set(RenderSystems::Render));
    }
}
