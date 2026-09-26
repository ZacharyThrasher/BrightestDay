// GPU-instanced debris cubes.
//
// The mesh is a batch of N unit cubes; UV.x of every vertex holds its cube's index inside the batch
// and the entity's MeshTag holds the batch's first instance. Per-cube data (position, size,
// rotation, colour, emissive) comes from one storage buffer written by the CPU every frame.
// Lighting goes through Bevy's PBR functions, so fog, shadows (cast + received) and exposure match
// the rest of the world. Used for both the main pass and the shadow/prepass (PREPASS_PIPELINE).

#import bevy_pbr::{
    mesh_functions,
    view_transformations::position_world_to_clip,
}

#ifdef PREPASS_PIPELINE
#import bevy_pbr::prepass_io::VertexOutput
#else
#import bevy_pbr::{
    mesh_bindings::mesh,
    mesh_view_bindings::view,
    pbr_types,
    pbr_functions,
}
#endif

struct Debris {
    // xyz = centre (world), w = edge length (0 = hidden)
    pos_size: vec4<f32>,
    // rotation quaternion (x, y, z, w)
    rot: vec4<f32>,
    // rgb = linear albedo, a = emissive strength (output units, not exposure-scaled)
    color: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<storage, read> debris: array<Debris>;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
#ifdef PREPASS_PIPELINE
    @location(1) uv: vec2<f32>,
#ifdef NORMAL_PREPASS_OR_DEFERRED_PREPASS
    @location(3) normal: vec3<f32>,
#endif
#else
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
#endif
}

fn quat_rotate(q: vec4<f32>, v: vec3<f32>) -> vec3<f32> {
    let t = 2.0 * cross(q.xyz, v);
    return v + q.w * t + cross(q.xyz, t);
}

fn fetch(v: Vertex) -> Debris {
    let idx = mesh_functions::get_tag(v.instance_index) + u32(v.uv.x + 0.5);
    return debris[idx];
}

#ifdef PREPASS_PIPELINE

@vertex
fn vertex(v: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let d = fetch(v);
    let world = d.pos_size.xyz + quat_rotate(d.rot, v.position * d.pos_size.w);
    out.world_position = vec4<f32>(world, 1.0);
    out.position = position_world_to_clip(world);
#ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
    out.unclipped_depth = out.position.z;
    out.position.z = min(out.position.z, 1.0);
#endif
#ifdef VERTEX_UVS_A
    out.uv = v.uv;
#endif
#ifdef NORMAL_PREPASS_OR_DEFERRED_PREPASS
    out.world_normal = quat_rotate(d.rot, v.normal);
#endif
#ifdef MOTION_VECTOR_PREPASS
    out.previous_world_position = out.world_position;
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = v.instance_index;
#endif
    return out;
}

#else

struct VOut {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) @interpolate(flat) instance_index: u32,
}

@vertex
fn vertex(v: Vertex) -> VOut {
    var out: VOut;
    let d = fetch(v);
    let world = d.pos_size.xyz + quat_rotate(d.rot, v.position * d.pos_size.w);
    out.world_position = vec4<f32>(world, 1.0);
    out.position = position_world_to_clip(world);
    out.world_normal = quat_rotate(d.rot, v.normal);
    out.color = d.color;
    out.instance_index = v.instance_index;
    return out;
}

@fragment
fn fragment(in: VOut) -> @location(0) vec4<f32> {
    var pbr_input = pbr_types::pbr_input_new();
    pbr_input.flags = mesh[in.instance_index].flags;
    pbr_input.is_orthographic = view.clip_from_view[3].w == 1.0;
    pbr_input.V = pbr_functions::calculate_view(in.world_position, pbr_input.is_orthographic);
    pbr_input.frag_coord = in.position;
    pbr_input.world_position = in.world_position;
    let n = normalize(in.world_normal);
    pbr_input.world_normal = n;
    pbr_input.N = n;
    pbr_input.material.base_color = vec4<f32>(in.color.rgb, 1.0);
    pbr_input.material.perceptual_roughness = 0.85;
    pbr_input.material.reflectance = vec3<f32>(0.3);
    // emissive.a = 0 → not scaled by camera exposure: strength is in output units (≥1 blooms).
    pbr_input.material.emissive = vec4<f32>(in.color.rgb * in.color.a, 0.0);
    pbr_input.material.flags = pbr_types::STANDARD_MATERIAL_FLAGS_FOG_ENABLED_BIT
        | pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_OPAQUE;
    var color = pbr_functions::apply_pbr_lighting(pbr_input);
    color = pbr_functions::main_pass_post_lighting_processing(pbr_input, color);
    return color;
}

#endif
