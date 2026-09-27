// Portal inner surface: a cheap swirling vortex (polar-coordinate spiral bands), premultiplied.
// Disc mesh in local XY (radius 1, scaled by the entity transform). State from constructs[tag]:
// primary/accent/core colours, state.y = open (0..1), state.z = flash, misc.y = seed.

#import bevy_pbr::{
    mesh_functions,
    view_transformations::position_world_to_clip,
    mesh_view_bindings::{view, globals},
}
#import bd_fx::common::{vnoise, fog_factor, fog_color}
#ifdef TONEMAP_IN_SHADER
#import bevy_core_pipeline::tonemapping::tone_mapping
#endif

struct Construct {
    primary: vec4<f32>,
    accent: vec4<f32>,
    dark: vec4<f32>,
    core: vec4<f32>,
    state: vec4<f32>,
    misc: vec4<f32>,
    origin: vec4<f32>,
    // Costume tints (unused here; keeps the stride of bd_fx::gpu::ConstructGpu).
    hair: vec4<f32>,
    eyes: vec4<f32>,
    skin: vec4<f32>,
    style: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<storage, read> constructs: array<Construct>;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
}

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) local: vec2<f32>,
    @location(2) @interpolate(flat) slot: u32,
}

@vertex
fn vertex(v: Vertex) -> VOut {
    var out: VOut;
    let slot = mesh_functions::get_tag(v.instance_index);
    let c = constructs[slot];
    let open = clamp(c.state.y, 0.0, 1.0);
    let world_from_local = mesh_functions::get_world_from_local(v.instance_index);
    let p = v.position * open;
    let wp = (world_from_local * vec4<f32>(p, 1.0)).xyz;
    out.world_pos = wp;
    out.clip = position_world_to_clip(wp);
    out.local = v.position.xy;
    out.slot = slot;
    return out;
}

@fragment
fn fragment(in: VOut) -> @location(0) vec4<f32> {
    let c = constructs[in.slot];
    let t = globals.time + c.misc.y * 0.1;
    let r = length(in.local);
    let a = atan2(in.local.y, in.local.x);
    // Spiral: angle + log radius → bands pulled into the centre.
    let sp = a * 3.0 + log(max(r, 0.02)) * 5.0 - t * 4.0;
    let bands = 0.5 + 0.5 * sin(sp);
    let n = vnoise(vec3<f32>(in.local * 3.0, t * 0.7));
    let swirl = smoothstep(0.35, 1.0, bands * 0.75 + n * 0.45);
    let rim = smoothstep(0.72, 1.0, r);
    let centre = 1.0 - smoothstep(0.0, 0.35, r);
    var e = c.primary.rgb * (0.25 + swirl * 1.3) + c.accent.rgb * swirl * rim * 1.5 + c.core.rgb * rim * rim * 2.0;
    e += c.core.rgb * centre * 0.6;
    var alpha = clamp(0.55 + swirl * 0.35 + rim * 0.3, 0.0, 1.0) * (1.0 - smoothstep(0.985, 1.0, r));
    e *= 1.0 - smoothstep(0.985, 1.0, r);
    e = mix(e, vec3<f32>(6.0), c.state.z);
    let f = fog_factor(in.world_pos);
    var out = vec4<f32>(e * (1.0 - f) + fog_color() * f * alpha, alpha);
#ifdef TONEMAP_IN_SHADER
    out = tone_mapping(out, view.color_grading);
#endif
    return out;
}
