// Hard-light construct surface.
//
// Mesh (bd_fx meshing of bd_shapes::mesh_part): per voxel face, uv = face corner (0/1),
// uv_b.x = palette slot | strong-edge bits << 2 | weak-point bit << 6 | has-colour << 7 |
// RGB565 sRGB voxel colour << 8 (terrain lifted by a graft), uv_b.y = AO shade.
// Per-construct state: `constructs[mesh tag]`.
//
// Look: emissive corps colour, bright contour lines on geometric creases (strong edges), a faint
// per-voxel grid, fresnel rim, scanline shimmer, premultiplied translucency (translucent variant).
// integrity → flicker, voxel dropouts and white-hot cracks; reveal → voxels grow in from the forming
// origin (ring) with a white leading edge; flash → white clash flash; dim → darker/fainter.

#import bevy_pbr::{
    mesh_functions,
    view_transformations::position_world_to_clip,
    mesh_view_bindings::{view, globals, lights},
}
#import bd_fx::common::{hash1, hash3i, vnoise, fog_factor, fog_color, ease_out_back}
#ifdef TONEMAP_IN_SHADER
#import bevy_core_pipeline::tonemapping::tone_mapping
#endif

struct Construct {
    primary: vec4<f32>,
    accent: vec4<f32>,
    dark: vec4<f32>,
    core: vec4<f32>,
    // integrity, reveal, flash, dim
    state: vec4<f32>,
    // voxel size, seed, opacity, mode (1 = dissolving)
    misc: vec4<f32>,
    // forming origin xyz (world), 1/radius
    origin: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<storage, read> constructs: array<Construct>;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) uv_b: vec2<f32>,
}

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) @interpolate(flat) packed: u32,
    @location(4) shade: f32,
    @location(5) @interpolate(flat) slot: u32,
    // xyz: local position (cracks), w: reveal transition 0..1 (1 = settled)
    @location(6) local: vec4<f32>,
    @location(7) @interpolate(flat) vhash: f32,
}

@vertex
fn vertex(v: Vertex) -> VOut {
    var out: VOut;
    let slot = mesh_functions::get_tag(v.instance_index);
    let c = constructs[slot];
    let vs = max(c.misc.x, 1e-3);
    let world_from_local = mesh_functions::get_world_from_local(v.instance_index);

    // Voxel centre (local): face point - half voxel along the normal - the corner offset.
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
    let corner = eu * (v.uv.x * 2.0 - 1.0) + ew * (v.uv.y * 2.0 - 1.0);
    let centre = v.position - (n + corner) * (0.5 * vs);
    let vid = vec3<i32>(round(centre / vs));
    let seed = u32(c.misc.y);
    let h = hash3i(vid, seed);
    let wc = (world_from_local * vec4<f32>(centre, 1.0)).xyz;

    // Reveal (forming / dissolve): voxels near the origin appear first.
    let reveal = c.state.y;
    var s = 1.0;
    if reveal < 0.999 {
        let d = clamp(length(wc - c.origin.xyz) * c.origin.w, 0.0, 1.0);
        let th = d * 0.78 + h * 0.22;
        s = clamp((reveal * 1.3 - th) / 0.3, 0.0, 1.0);
    }
    // Low integrity: voxels blink out briefly.
    let integ = c.state.x;
    if integ < 0.5 {
        let amt = (0.5 - integ) * 2.0;
        let blink = hash3i(vid, u32(globals.time * 14.0) + seed);
        if blink < amt * 0.12 {
            s = s * 0.15;
        }
    }
    let k = select(ease_out_back(s), s, c.misc.w > 0.5);
    let p = centre + (v.position - centre) * k;

    let wp = (world_from_local * vec4<f32>(p, 1.0)).xyz;
    out.world_pos = wp;
    out.clip = position_world_to_clip(wp);
#ifdef FX_OVERLAY
    // Reverse-Z: remap depth into [0.5, 1] so nothing farther than 2x the near plane can hide it,
    // while keeping the word's own depth order (self-occlusion stays correct).
    out.clip.z = (0.5 + 0.5 * out.clip.z / out.clip.w) * out.clip.w;
#endif
    out.world_normal = mesh_functions::mesh_normal_local_to_world(n, v.instance_index);
    out.uv = v.uv;
    out.packed = u32(v.uv_b.x + 0.5);
    out.shade = v.uv_b.y;
    out.slot = slot;
    out.local = vec4<f32>(p, s);
    out.vhash = h;
    return out;
}

fn palette(c: Construct, t: u32) -> vec3<f32> {
    if t == 0u {
        return c.primary.rgb;
    } else if t == 1u {
        return c.accent.rgb;
    } else if t == 2u {
        return c.dark.rgb;
    }
    return c.core.rgb;
}

@fragment
fn fragment(in: VOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    let c = constructs[in.slot];
    let tint = in.packed & 3u;
    let edges = (in.packed >> 2u) & 15u;
    let weak = (in.packed >> 6u) & 1u;
    var base = palette(c, tint);
    // Terrain voxels (animated world): their own material colour, tinted with hard light.
    let earthy = ((in.packed >> 7u) & 1u) == 1u;
    if earthy {
        let q = in.packed >> 8u;
        let srgb = vec3<f32>(f32((q >> 11u) & 31u) / 31.0, f32((q >> 5u) & 63u) / 63.0, f32(q & 31u) / 31.0);
        base = mix(pow(srgb, vec3<f32>(2.2)) * 1.15, c.primary.rgb, 0.28);
    }
    let t = globals.time;

    let N = normalize(in.world_normal);
    let V = normalize(view.world_position.xyz - in.world_pos);
    let ndv = clamp(abs(dot(N, V)), 0.0, 1.0);
    let fres = pow(1.0 - ndv, 3.0);

    // Edge distances in voxel-face units (0 at the edge, 0.5 at the centre).
    let uv = in.uv;
    let du0 = uv.x;
    let du1 = 1.0 - uv.x;
    let dv0 = uv.y;
    let dv1 = 1.0 - uv.y;
    let grid_d = min(min(du0, du1), min(dv0, dv1));
    var strong_d = 1.0;
    if (edges & 1u) != 0u { strong_d = min(strong_d, du0); }
    if (edges & 2u) != 0u { strong_d = min(strong_d, du1); }
    if (edges & 4u) != 0u { strong_d = min(strong_d, dv0); }
    if (edges & 8u) != 0u { strong_d = min(strong_d, dv1); }
    // Screen-space size of one voxel face in uv units per pixel.
    let px = max(max(fwidth(uv.x), fwidth(uv.y)), 1e-4);
    let sw = max(0.075, px * 1.1);
    let strong = (1.0 - smoothstep(sw - px, sw + px, strong_d)) * clamp(1.6 - px * 2.2, 0.35, 1.0);
    let gw = max(0.03, px * 0.7);
    // accent.a = 1 hides the per-voxel grid (text: creases only).
    let grid = (1.0 - smoothstep(gw - px, gw + px, grid_d)) * clamp(1.4 - px * 5.0, 0.0, 1.0) * (1.0 - c.accent.a);
    // Soft inner bevel glow (each voxel reads as a lit cube).
    let bevel = (1.0 - smoothstep(0.0, 0.32, grid_d)) * (1.0 - c.accent.a);

    let ao = in.shade;
    let is_core = tint == 3u;
    let core_col = c.core.rgb;

    // Daylight adaptation: bright ambient (sunny maps) ⇒ denser, more saturated body and a dark
    // contour band inside the bright edge lines so constructs still pop against sky/grass.
    let amb = max(max(lights.ambient_color.r, lights.ambient_color.g), lights.ambient_color.b) * view.exposure;
    let day = smoothstep(0.12, 0.45, amb);
    let contour = (1.0 - smoothstep(sw * 2.6 - px, sw * 2.6 + px, strong_d)) * (1.0 - strong);

    // Emission.
    var e = base * (0.42 + 0.38 * ao) * (1.0 + 0.3 * day);
    e += base * bevel * 0.35;
    e += base * fres * 1.8;
    e += mix(base, core_col, 0.35) * grid * 0.9;
    e += mix(base, core_col, 0.55) * strong * 2.6;
    if is_core {
        // Hot core voxels: brighter and whiter, but keep hue + lines so they read as voxels.
        e = core_col * (1.05 + 0.45 * ao + 0.35 * bevel) + mix(core_col, vec3<f32>(1.0), 0.5) * strong * 1.6 + core_col * grid * 0.5;
    }
    // Scanline shimmer: fine lines + a slow bright band sweeping upward.
    let scan = pow(0.5 + 0.5 * sin(in.world_pos.y * 42.0 - t * 7.0), 6.0);
    let band = smoothstep(0.93, 1.0, fract(in.world_pos.y * 0.23 - t * 0.55 + c.misc.y * 0.013));
    e *= 1.0 + scan * 0.12;
    e += base * band * 0.9;
    // Weak points pulse.
    if weak == 1u {
        e += core_col * (0.25 + 0.25 * sin(t * 6.0));
    }

    // Heat glow (minigun barrels): orange → white-hot, strongest on edges.
    let heat = c.primary.a;
    if heat > 0.0 {
        let hot = mix(vec3<f32>(2.2, 0.55, 0.08), vec3<f32>(4.0, 3.2, 2.2), heat * heat);
        e = mix(e, hot * (0.7 + 0.6 * strong), clamp(heat * 0.85, 0.0, 1.0));
    }

    // Dark contour (day only; also skips the hot core).
    if !is_core {
        e = mix(e, c.dark.rgb * 0.5, contour * day * 0.75);
    }

    // Opacity: translucent body, solid lines and rims; much denser in daylight. The daylight floor
    // follows the requested opacity: a see-through request (< 0.7, e.g. a dome with a player inside)
    // keeps its body thin in daylight too; the default 0.72 and player bodies (0.92) are unchanged.
    var alpha = c.misc.z * (0.62 + 0.25 * ao);
    let day_floor = mix(0.45, 0.92, smoothstep(0.4, 0.7, c.misc.z));
    alpha = mix(alpha, max(alpha, day_floor), day);
    alpha = max(alpha, max(strong, max(fres * 0.9, max(grid * 0.6, contour * day))));
    if is_core { alpha = max(alpha, 0.85); }
    if earthy { alpha = max(alpha, 0.88); }

    // Integrity: cracks (white-hot seams across the model) + flicker.
    let integ = c.state.x;
    if integ < 0.8 {
        let amt = clamp((0.8 - integ) / 0.8, 0.0, 1.0);
        let q = in.local.xyz * 2.1 + vec3<f32>(c.misc.y * 0.37);
        let nv = vnoise(q) + 0.5 * vnoise(q * 2.3) - 0.75;
        let cw = 0.035 + 0.05 * amt;
        let crack = 1.0 - smoothstep(cw * 0.5, cw, abs(nv));
        let mask = step(vnoise(q * 0.45 + vec3<f32>(11.0)), amt * 1.1);
        let cr = crack * mask;
        e = mix(e, vec3<f32>(3.2, 3.2, 3.0) + core_col, cr);
        alpha = max(alpha, cr);
        let fl = hash1(u32(t * 18.0) + u32(c.misc.y));
        e *= 1.0 - amt * 0.55 * step(0.55, fl);
    }

    // Reveal leading edge: voxels in transition glow white-hot.
    let s = in.local.w;
    if s < 0.999 {
        let lead = (1.0 - s);
        e = mix(e, core_col * 3.5 + vec3<f32>(1.0), clamp(lead * 1.4, 0.0, 1.0));
        alpha = max(alpha, lead);
    }

    // Clash flash.
    let flash = c.state.z;
    e = mix(e, vec3<f32>(7.0, 7.0, 7.0), flash);
    alpha = mix(alpha, 1.0, flash);

    // Dim.
    let dim = c.state.w;
    e *= 1.0 - 0.75 * dim;
    alpha *= 1.0 - 0.45 * dim;

    // Fog.
    let f = fog_factor(in.world_pos);

#ifdef BLEND_PREMULTIPLIED_ALPHA
    // Fade-out (opacity < 0.3: lines and rims too — e.g. the local body when the third-person
    // camera is pushed into the head).
    let fade = clamp(c.misc.z / 0.3, 0.0, 1.0);
    alpha *= fade;
    var rgb = e * (1.0 - f) * fade + fog_color() * f * alpha;
    var out = vec4<f32>(rgb, alpha);
#else
    var rgb = mix(e, fog_color(), f);
    var out = vec4<f32>(rgb, 1.0);
#endif
#ifdef TONEMAP_IN_SHADER
    out = tone_mapping(out, view.color_grading);
#endif
    return out;
}
