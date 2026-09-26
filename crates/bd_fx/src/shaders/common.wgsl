#define_import_path bd_fx::common

#import bevy_pbr::mesh_view_bindings::{view, fog}

// PCG-ish integer hash → 0..1
fn hash_u(x: u32) -> u32 {
    var v = x * 747796405u + 2891336453u;
    let w = ((v >> ((v >> 28u) + 4u)) ^ v) * 277803737u;
    return (w >> 22u) ^ w;
}

fn hash1(x: u32) -> f32 {
    return f32(hash_u(x) >> 8u) * (1.0 / 16777216.0);
}

fn hash3i(p: vec3<i32>, seed: u32) -> f32 {
    let u = vec3<u32>(bitcast<u32>(p.x), bitcast<u32>(p.y), bitcast<u32>(p.z));
    return hash1(u.x * 73856093u ^ u.y * 19349663u ^ u.z * 83492791u ^ seed * 2654435761u);
}

fn hash3v(x: u32) -> vec3<f32> {
    return vec3<f32>(hash1(x), hash1(x ^ 0x68bc21ebu), hash1(x ^ 0x02e5be93u));
}

// Smooth 3D value noise, 0..1.
fn vnoise(p: vec3<f32>) -> f32 {
    let i = vec3<i32>(floor(p));
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash3i(i, 7u);
    let b = hash3i(i + vec3<i32>(1, 0, 0), 7u);
    let c = hash3i(i + vec3<i32>(0, 1, 0), 7u);
    let d = hash3i(i + vec3<i32>(1, 1, 0), 7u);
    let e = hash3i(i + vec3<i32>(0, 0, 1), 7u);
    let f2 = hash3i(i + vec3<i32>(1, 0, 1), 7u);
    let g = hash3i(i + vec3<i32>(0, 1, 1), 7u);
    let h = hash3i(i + vec3<i32>(1, 1, 1), 7u);
    return mix(mix(mix(a, b, u.x), mix(c, d, u.x), u.y), mix(mix(e, f2, u.x), mix(g, h, u.x), u.y), u.z);
}

fn quat_rotate(q: vec4<f32>, v: vec3<f32>) -> vec3<f32> {
    let t = 2.0 * cross(q.xyz, v);
    return v + q.w * t + cross(q.xyz, t);
}

fn axis_angle(axis: vec3<f32>, angle: f32) -> vec4<f32> {
    let h = angle * 0.5;
    return vec4<f32>(axis * sin(h), cos(h));
}

// Cheap distance fog (no in-scattering shadow lookups). Returns the fog blend factor 0..1.
fn fog_factor(world_pos: vec3<f32>) -> f32 {
#ifdef DISTANCE_FOG
    let d = length(world_pos - view.world_position.xyz);
    var f = 0.0;
    if fog.mode == 1u {
        f = clamp((d - fog.be.x) / max(fog.be.y - fog.be.x, 1e-3), 0.0, 1.0);
    } else if fog.mode == 2u {
        f = 1.0 - exp(-d * fog.be.x);
    } else if fog.mode == 3u {
        let k = d * fog.be.x;
        f = 1.0 - exp(-k * k);
    } else if fog.mode == 4u {
        f = 1.0 - exp(-d * max(max(fog.be.x, fog.be.y), fog.be.z));
    }
    return f * fog.base_color.a;
#else
    return 0.0;
#endif
}

fn fog_color() -> vec3<f32> {
    return fog.base_color.rgb;
}

fn ease_out_back(x: f32) -> f32 {
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    let y = x - 1.0;
    return 1.0 + c3 * y * y * y + c1 * y * y;
}

// Time since `t0` (both Time::elapsed_secs_wrapped), robust to the hourly wrap.
fn age_since(now: f32, t0: f32) -> f32 {
    var a = now - t0;
    if a < -1800.0 {
        a = a + 3600.0;
    }
    return a;
}
