// Player outline: inverted hull for voxel meshes (bd_fx construct part meshes).
//
// Every voxel face quad is inflated around *its own voxel*: a vertex moves along (normal + corner
// direction), i.e. each exposed voxel becomes a cube `t` larger on every side. Faces of one voxel
// stay welded (no cracks at hard edges, which plain normal extrusion would open on a cube mesh) and
// neighbouring voxels simply overlap. Only back faces are drawn (front-face culling), so the hull
// shows as a rim around the silhouette and is hidden behind the body everywhere else; normal depth
// testing ⇒ visible in line of sight only.
//
// `t` is computed per vertex from the view depth so the line stays a constant pixel width at any
// distance. Two hulls per part (MeshTag bit 0): layer 0 = thin dark inner line, layer 1 = corps
// outer line (the inner hull is nearer, so it wins where both overlap). The colours are kept below
// the bloom threshold so the line itself never blooms (crisp even in heavy glow).
//
// Per-slot params: `outlines[tag >> 1]`.

#import bevy_pbr::{
    mesh_functions,
    view_transformations::position_world_to_clip,
    mesh_view_bindings::view,
}
#import bd_fx::common::{fog_factor, fog_color}
#ifdef TONEMAP_IN_SHADER
#import bevy_core_pipeline::tonemapping::tone_mapping
#endif

struct Outline {
    // rgb outer colour (linear), a = outer edge (px from the silhouette)
    outer: vec4<f32>,
    // rgb inner colour, a = inner line width (px)
    inner: vec4<f32>,
    // x = dash period (px, 0 = solid), y = duty (lit fraction), z = dash angle (rad), w = fog amount
    pattern: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<storage, read> outlines: array<Outline>;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
}

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) @interpolate(flat) tag: u32,
}

@vertex
fn vertex(v: Vertex) -> VOut {
    var out: VOut;
    let tag = mesh_functions::get_tag(v.instance_index);
    let o = outlines[tag >> 1u];
    let px = select(o.inner.a, o.outer.a, (tag & 1u) == 1u);
    let m = mesh_functions::get_world_from_local(v.instance_index);

    // Direction from the voxel centre to this corner (each component ±1).
    let n = v.normal;
    let an = abs(n);
    var eu = vec3<f32>(1.0, 0.0, 0.0);
    var ew = vec3<f32>(0.0, 1.0, 0.0);
    if an.x > 0.5 {
        eu = vec3<f32>(0.0, 1.0, 0.0);
        ew = vec3<f32>(0.0, 0.0, 1.0);
    } else if an.y > 0.5 {
        eu = vec3<f32>(0.0, 0.0, 1.0);
        ew = vec3<f32>(1.0, 0.0, 0.0);
    }
    let dir = n + eu * (v.uv.x * 2.0 - 1.0) + ew * (v.uv.y * 2.0 - 1.0);

    let wp0 = (m * vec4<f32>(v.position, 1.0)).xyz;
    // World metres per pixel at this depth (perspective or orthographic).
    let p11 = max(view.clip_from_view[1][1], 1e-4);
    let h = max(view.viewport.w, 1.0);
    var wpp = 2.0 / (p11 * h);
    if view.clip_from_view[3][3] != 1.0 {
        let vz = max(-(view.view_from_world * vec4<f32>(wp0, 1.0)).z, 0.05);
        wpp = wpp * vz;
    }
    // Local → world scale (parts are uniformly scaled by the body size / pose squash).
    let s = max(length(m[0].xyz), 1e-4);
    let t = min(px * wpp, 2.0);
    let wp = wp0 + (m * vec4<f32>(dir, 0.0)).xyz * (t / s);

    out.world_pos = wp;
    out.clip = position_world_to_clip(wp);
    out.tag = tag;
    return out;
}

@fragment
fn fragment(in: VOut) -> @location(0) vec4<f32> {
    let o = outlines[in.tag >> 1u];
    var col = o.inner.rgb;
    if (in.tag & 1u) == 1u {
        col = o.outer.rgb;
        // Colour-blind aid: a per-player dash pattern (screen space) with dark gaps.
        let period = o.pattern.x;
        if period > 0.5 {
            let a = o.pattern.z;
            let d = in.clip.x * cos(a) + in.clip.y * sin(a);
            if fract(d / period) > o.pattern.y {
                col = o.inner.rgb;
            }
        }
    }
    // `fog` only exists in views with DistanceFog (the cut-in camera has none).
#ifdef DISTANCE_FOG
    col = mix(col, fog_color(), fog_factor(in.world_pos) * o.pattern.w);
#endif
    var out = vec4<f32>(col, 1.0);
#ifdef TONEMAP_IN_SHADER
    out = tone_mapping(out, view.color_grading);
#endif
    return out;
}
