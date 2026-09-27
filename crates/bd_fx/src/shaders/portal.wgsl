// Portal surface. Two looks, blended by the slot's view mix:
// * the swirling vortex (polar spiral bands, premultiplied) — the fallback, the back face, and what
//   portal-view cameras see (no recursion);
// * a WINDOW: the game's live portal-view picture (HDR, pre-tonemap) sampled at this fragment's
//   SCREEN position, so it lines up exactly with the view from the partner portal; a thin animated
//   rim in the owner's colour keeps it reading as a portal. Doorways add warm light at the frame.
// Mesh: unit disc (ring) or ±1 quad (doorway) in local XY, scaled to the opening by the transform;
// the entry side is local −Z (the PortalFx faces −Z).
// Slot data (constructs[tag]): primary/accent/core colours, state.y = open (0..1), state.z = flash,
// misc.y = seed, hair = (uv scale x, uv scale y, view mix, view index 0/1/2), eyes.x = style
// (0 ring, 1 doorway).
// Close-up ("band"): with the camera within a hand's width in front of the opening, its near plane
// would cut the disc; the vertex stage then emits a full-screen triangle and the fragment stage
// ray-casts the opening (writing the true depth), so stepping through never shows the wrong side.

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
    // Portal view data (see above); costume tints on constructs.
    hair: vec4<f32>,
    eyes: vec4<f32>,
    skin: vec4<f32>,
    style: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<storage, read> constructs: array<Construct>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var view_a: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var view_s: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var view_b: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var<uniform> params: vec4<f32>;

// Camera closer than this (m) in front of the opening → full-screen ray-cast mode.
const BAND: f32 = 0.3;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
}

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) local: vec2<f32>,
    @location(2) @interpolate(flat) slot: u32,
    @location(3) @interpolate(flat) band: u32,
    @location(4) @interpolate(flat) centre: vec3<f32>,
    @location(5) @interpolate(flat) nrm: vec3<f32>,
    @location(6) @interpolate(flat) ax: vec3<f32>,
    @location(7) @interpolate(flat) ay: vec3<f32>,
}

struct FOut {
    @location(0) color: vec4<f32>,
    @builtin(frag_depth) depth: f32,
}

fn inside(local: vec2<f32>, door: bool) -> bool {
    if door {
        return abs(local.x) <= 1.0 && abs(local.y) <= 1.0;
    }
    return dot(local, local) <= 1.0;
}

@vertex
fn vertex(v: Vertex) -> VOut {
    var out: VOut;
    let slot = mesh_functions::get_tag(v.instance_index);
    let c = constructs[slot];
    let open = clamp(c.state.y, 0.0, 1.0);
    let m = mesh_functions::get_world_from_local(v.instance_index);
    let centre = (m * vec4<f32>(0.0, 0.0, 0.0, 1.0)).xyz;
    let ax = (m * vec4<f32>(1.0, 0.0, 0.0, 0.0)).xyz * max(open, 1e-3);
    let ay = (m * vec4<f32>(0.0, 1.0, 0.0, 0.0)).xyz * max(open, 1e-3);
    let nrm = normalize((m * vec4<f32>(0.0, 0.0, -1.0, 0.0)).xyz);
    let door = c.eyes.x > 0.5;
    // Close-up band: live view, camera just in front of the opening and inside its outline.
    let cam = view.world_position;
    let s = dot(cam - centre, nrm);
    let rel = cam - centre - nrm * s;
    let cl = vec2<f32>(dot(rel, ax) / max(dot(ax, ax), 1e-6), dot(rel, ay) / max(dot(ay, ay), 1e-6));
    let live = params.x > 0.5 && c.hair.w > 0.5 && c.hair.z > 0.01;
    let band = live && s > -0.02 && s < BAND && inside(cl * 0.98, door);
    let p = v.position * open;
    let wp = (m * vec4<f32>(p, 1.0)).xyz;
    out.world_pos = wp;
    if band {
        // Covers the screen (the mesh spans ±1 → ±3 in NDC); the fragment ray-casts the opening.
        out.clip = vec4<f32>(v.position.xy * 3.0, 0.5, 1.0);
        out.band = 1u;
    } else {
        out.clip = position_world_to_clip(wp);
        out.band = 0u;
    }
    out.local = v.position.xy;
    out.slot = slot;
    out.centre = centre;
    out.nrm = nrm;
    out.ax = ax;
    out.ay = ay;
    return out;
}

fn vortex(c: Construct, local: vec2<f32>, r: f32, t: f32) -> vec4<f32> {
    let a = atan2(local.y, local.x);
    // Spiral: angle + log radius → bands pulled into the centre.
    let sp = a * 3.0 + log(max(r, 0.02)) * 5.0 - t * 4.0;
    let bands = 0.5 + 0.5 * sin(sp);
    let n = vnoise(vec3<f32>(local * 3.0, t * 0.7));
    let swirl = smoothstep(0.35, 1.0, bands * 0.75 + n * 0.45);
    let rim = smoothstep(0.72, 1.0, r);
    let centre = 1.0 - smoothstep(0.0, 0.35, r);
    var e = c.primary.rgb * (0.25 + swirl * 1.3) + c.accent.rgb * swirl * rim * 1.5 + c.core.rgb * rim * rim * 2.0;
    e += c.core.rgb * centre * 0.6;
    let alpha = clamp(0.55 + swirl * 0.35 + rim * 0.3, 0.0, 1.0);
    return vec4<f32>(e, alpha);
}

@fragment
fn fragment(in: VOut) -> FOut {
    var out: FOut;
    let c = constructs[in.slot];
    let door = c.eyes.x > 0.5;
    var local = in.local;
    var depth = in.clip.z;
    var world_pos = in.world_pos;
    if in.band == 1u {
        // Ray from the eye through this pixel onto the opening's plane.
        let uv = (in.clip.xy - view.viewport.xy) / view.viewport.zw;
        let ndc = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
        let fp = view.world_from_clip * vec4<f32>(ndc, 1e-4, 1.0);
        let dir = normalize(fp.xyz / fp.w - view.world_position);
        let dn = dot(dir, in.nrm);
        let h = dot(view.world_position - in.centre, in.nrm);
        if dn > -1e-5 {
            discard;
        }
        let tt = h / -dn;
        let hit = view.world_position + dir * tt;
        let rel = hit - in.centre;
        local = vec2<f32>(dot(rel, in.ax) / max(dot(in.ax, in.ax), 1e-6), dot(rel, in.ay) / max(dot(in.ay, in.ay), 1e-6));
        if !inside(local, door) {
            discard;
        }
        let hc = view.clip_from_world * vec4<f32>(hit, 1.0);
        depth = clamp(hc.z / hc.w, 0.0, 1.0);
        world_pos = hit;
    }
    let t = globals.time + c.misc.y * 0.1;
    // Doorways: square distance (rim along the frame); rings: radial.
    let r = select(length(local), max(abs(local.x), abs(local.y)), door);
    let edge = 1.0 - smoothstep(0.985, 1.0, r);
    var sw = vortex(c, local, r, t);
    sw = vec4<f32>(sw.rgb * edge, sw.a * edge);
    sw = vec4<f32>(mix(sw.rgb, vec3<f32>(6.0), c.state.z), sw.a);
    let f = fog_factor(world_pos);
    var col = vec4<f32>(sw.rgb * (1.0 - f) + fog_color() * f * sw.a, sw.a);

    // The window: the partner portal's view, where this pixel would see it.
    let facing = dot(view.world_position - in.centre, in.nrm) > -0.02;
    let mixv = select(0.0, clamp(c.hair.z, 0.0, 1.0), params.x > 0.5 && c.hair.w > 0.5 && facing);
    if mixv > 0.0 {
        let suv = clamp((in.clip.xy - view.viewport.xy) / view.viewport.zw, vec2<f32>(0.0), vec2<f32>(1.0)) * c.hair.xy;
        let va = textureSampleLevel(view_a, view_s, suv, 0.0).rgb;
        let vb = textureSampleLevel(view_b, view_s, suv, 0.0).rgb;
        var win = select(va, vb, c.hair.w > 1.5);
        // Thin animated rim in the owner's colour (plus the flash).
        let ang = select(atan2(local.y, local.x), (local.x + local.y) * 3.14159, door);
        let pulse = 0.65 + 0.35 * sin(ang * 5.0 - t * 3.5) * sin(ang * 2.0 + t * 1.3);
        let rim = smoothstep(0.9, 0.995, r) * edge;
        let glow = smoothstep(0.75, 0.99, r) * edge * 0.35;
        win = win * (1.0 - rim * 0.85) + c.primary.rgb * (rim * 2.2 * pulse + glow) + c.core.rgb * rim * rim * 1.5;
        if door {
            // Warm light spilling through the open door.
            let warm = vec3<f32>(1.0, 0.58, 0.24);
            let spill = smoothstep(0.55, 1.0, r) * 0.55 + (1.0 - smoothstep(-1.0, 0.2, local.y)) * 0.12;
            win += warm * spill * edge;
        }
        win = mix(win, vec3<f32>(6.0), c.state.z);
        let wa = edge;
        col = mix(col, vec4<f32>(win * wa, wa), mixv);
    }
#ifdef TONEMAP_IN_SHADER
    col = tone_mapping(col, view.color_grading);
#endif
    out.color = col;
    out.depth = depth;
    return out;
}
