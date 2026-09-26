// Terrain: StandardMaterial PBR (vertex colours) + neon glow from the mesher's per-vertex emissive
// channel (vertex colour alpha; the opaque PBR path ignores alpha). See voxel_render.rs.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
}

#ifdef PREPASS_PIPELINE
#import bevy_pbr::{
    prepass_io::{VertexOutput, FragmentOutput},
    pbr_deferred_functions::deferred_output,
}
#else
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
}
#endif

struct VoxelGlow {
    // x = neon boost (map), y = gain
    params: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> glow: VoxelGlow;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    var e = 0.0;
#ifdef VERTEX_COLORS
    e = in.color.a;
#endif
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);
    // Neon (emissive 1) and crystals (0.8) glow with the map's boost; glass / water (< 0.5) never.
    let k = smoothstep(0.5, 1.0, e) * glow.params.x * glow.params.y;
    if k > 0.0 {
        // alpha 0 = raw HDR (not scaled by exposure): > 1 blooms with the camera's threshold-1 bloom.
        pbr_input.material.emissive = vec4<f32>(pbr_input.material.emissive.rgb + pbr_input.material.base_color.rgb * k, 0.0);
    }
#ifdef PREPASS_PIPELINE
    let out = deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#endif
    return out;
}
