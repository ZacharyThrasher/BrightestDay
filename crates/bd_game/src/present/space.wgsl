// Orbit's backdrop on a camera-following dome (present::space). Linear HDR out, no fog.
//  * three star layers at finite distances (subtle parallax as you move), streaked along a
//    direction when launched / during the space-KO shot
//  * the planet far below: oceans, continents, polar ice, a scrolling cloud layer, day/night
//    terminator with city lights, a fresnel atmosphere rim and a halo above the limb
//  * a faint nebula band, the moon drifting past, the sun
#import bevy_pbr::{forward_io::VertexOutput, mesh_view_bindings::view}

struct Space {
    // xyz = direction toward the sun
    sun_dir: vec4<f32>,
    // rgb = sun colour, w = time (s)
    sun_color: vec4<f32>,
    // xyz = camera world position, w = moon orbit angle (rad)
    cam: vec4<f32>,
    // xyz = streak direction (unit, world), w = amount 0..1
    streak: vec4<f32>,
    // x = style: 0 Orbit, 1 Cybertron (metal planet, amber/energon seams, fires, two moons)
    style: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> sp: Space;

const PLANET_R: f32 = 5000.0;
const PLANET_C: vec3<f32> = vec3<f32>(0.0, -5320.0, 0.0);

fn pcg3(v0: vec3<u32>) -> vec3<u32> {
    var v = v0 * 1664525u + 1013904223u;
    v.x += v.y * v.z; v.y += v.z * v.x; v.z += v.x * v.y;
    v ^= v >> vec3<u32>(16u);
    v.x += v.y * v.z; v.y += v.z * v.x; v.z += v.x * v.y;
    return v;
}

fn hash3(p: vec3<f32>) -> vec3<f32> {
    let i = vec3<i32>(floor(p));
    return vec3<f32>(pcg3(bitcast<vec3<u32>>(i))) * (1.0 / 4294967295.0);
}

fn vnoise(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash3(i).x;
    let b = hash3(i + vec3(1.0, 0.0, 0.0)).x;
    let c = hash3(i + vec3(0.0, 1.0, 0.0)).x;
    let d = hash3(i + vec3(1.0, 1.0, 0.0)).x;
    let e = hash3(i + vec3(0.0, 0.0, 1.0)).x;
    let f1 = hash3(i + vec3(1.0, 0.0, 1.0)).x;
    let g = hash3(i + vec3(0.0, 1.0, 1.0)).x;
    let h = hash3(i + vec3(1.0, 1.0, 1.0)).x;
    let x0 = mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
    let x1 = mix(mix(e, f1, u.x), mix(g, h, u.x), u.y);
    return mix(x0, x1, u.z) * 2.0 - 1.0;
}

fn fbm(p0: vec3<f32>, oct: i32) -> f32 {
    var p = p0;
    var s = 0.0;
    var a = 0.5;
    var n = 0.0;
    for (var o = 0; o < oct; o++) {
        s += a * vnoise(p);
        n += a;
        a *= 0.5;
        p = p * 2.03 + vec3(17.1, 3.7, 9.2);
    }
    return s / n;
}

// One star layer: point stars at hashed spots in direction cells.
fn star_layer(dir: vec3<f32>, scale: f32, keep: f32, seed: f32) -> vec3<f32> {
    let q = dir * scale;
    let cell = floor(q);
    let h = hash3(cell + vec3(seed));
    if (h.x < keep) {
        return vec3(0.0);
    }
    let jit = hash3(cell + vec3(seed + 11.0)) * 0.6 + 0.2;
    let d = length(q - cell - jit);
    let a = max(1.0 - d / 0.2, 0.0);
    let tint = h.y;
    let twinkle = 0.75 + 0.25 * sin(sp.sun_color.w * (1.0 + h.z * 3.0) + h.z * 40.0);
    return vec3(0.8 + 0.2 * tint, 0.85, 1.0 - 0.25 * tint) * a * a * 3.0 * twinkle * (0.4 + h.z);
}

fn stars(d: vec3<f32>) -> vec3<f32> {
    // Layers at finite distances: moving 100 m shifts the near layer ~1.4°.
    let cam = sp.cam.xyz;
    let d0 = normalize(d * 4000.0 - cam);
    let d1 = normalize(d * 9000.0 - cam);
    return star_layer(d0, 70.0, 0.965, 1.0) * 1.1 + star_layer(d1, 160.0, 0.95, 7.0) * 0.6 + star_layer(d, 380.0, 0.93, 13.0) * 0.35;
}

// Cybertron seen from a raised city plate: gunmetal plates on a grid, district shading, glowing
// amber / energon seams (brighter on the night side), concentric ring works, scattered fires.
fn cybertron_surface(n: vec3<f32>, dir: vec3<f32>, sun: vec3<f32>, t: f32) -> vec3<f32> {
    let ndl = dot(n, sun);
    let day = clamp((ndl + 0.15) / 0.35, 0.0, 1.0);
    let cell = floor(n * 90.0);
    let h = hash3(cell);
    let district = fbm(n * 7.0, 4) * 0.5 + 0.5;
    var base = mix(vec3(0.045, 0.05, 0.065), vec3(0.13, 0.14, 0.17), h.x * 0.6 + district * 0.4);
    var col = base * (0.03 + 1.25 * day * sqrt(max(ndl, 0.0)));
    // Seams: a coarse plate grid, amber in some districts and energon blue in others.
    let g = abs(fract(n * 24.0) - vec3(0.5));
    let line = smoothstep(0.455, 0.49, max(max(g.x, g.y), g.z));
    let sel = fbm(n * 2.6 + vec3(9.0), 3);
    let seam = select(vec3(0.25, 0.75, 2.0), vec3(2.2, 0.75, 0.16), sel > -0.05);
    let lit_mask = smoothstep(-0.15, 0.35, fbm(n * 11.0 + vec3(4.0), 3));
    col += seam * line * lit_mask * (0.25 + 0.75 * (1.0 - day));
    // Concentric ring works around a few hubs.
    for (var k = 0; k < 3; k++) {
        let c = normalize(hash3(vec3(f32(k) * 7.0 + 1.0, 3.0, 5.0)) * 2.0 - vec3(1.0));
        let a = acos(clamp(dot(n, c), -1.0, 1.0));
        let r = abs(fract(a * 45.0) - 0.5);
        col += select(vec3(2.0, 0.7, 0.15), vec3(0.3, 0.8, 2.0), k == 1) * smoothstep(0.47, 0.5, r) * smoothstep(0.35, 0.05, a) * 0.6;
    }
    // Fires and damage.
    let fire = fbm(n * 30.0 + vec3(t * 0.03, 0.0, 0.0), 3);
    col += vec3(2.4, 0.7, 0.12) * max(fire - 0.5, 0.0) * 5.0 * step(0.2, fbm(n * 4.0 + vec3(2.0), 2));
    // Dusty amber haze at the rim.
    let rim = pow(clamp(1.0 + dot(n, dir), 0.0, 1.0), 5.0);
    return mix(col, vec3(0.55, 0.28, 0.12) * (0.15 + 0.6 * day), clamp(rim * 0.8, 0.0, 0.6));
}

fn planet(ro: vec3<f32>, dir: vec3<f32>, sun: vec3<f32>, t: f32) -> vec4<f32> {
    let oc = ro - PLANET_C;
    let b = dot(oc, dir);
    let c = dot(oc, oc) - PLANET_R * PLANET_R;
    let h = b * b - c;
    if (h <= 0.0 || -b - sqrt(h) <= 0.0) {
        // Sky: atmosphere halo just above the limb (lit side brighter); nothing when looking away
        // from the planet (space is black overhead).
        // Height of the ray's closest approach above the surface; looking up it keeps growing with
        // elevation (continuous at the horizon), so the glow hugs the limb and space overhead is black.
        let hd = select(max(length(oc - dir * b) - PLANET_R, 0.0), length(oc) - PLANET_R + b * 1.5, b >= 0.0);
        let side = normalize(oc - dir * b);
        let lit = clamp(dot(side, sun) * 0.8 + 0.35, 0.05, 1.0);
        let halo = exp(-hd / 120.0) * 0.8 + exp(-hd / 600.0) * 0.12;
        let halo_col = select(vec3(0.10, 0.28, 0.85), vec3(0.45, 0.22, 0.10), sp.style.x > 0.5);
        return vec4(halo_col * halo * lit, 0.0);
    }
    let tt = -b - sqrt(h);
    let n = normalize(oc + dir * tt);
    if (sp.style.x > 0.5) {
        return vec4(cybertron_surface(n, dir, sun, t), 1.0);
    }
    let ndl = dot(n, sun);
    let day = clamp((ndl + 0.1) / 0.32, 0.0, 1.0);
    let q = n * 3.2;
    let land = fbm(q, 5);
    let is_land = land > 0.06;
    let lat = abs(n.z);
    var base: vec3<f32>;
    if (is_land) {
        let dry = fbm(q * 2.1 + vec3(5.0), 3);
        let g = mix(vec3(0.07, 0.18, 0.05), vec3(0.40, 0.30, 0.16), clamp((dry + 0.2) * 1.8, 0.0, 1.0));
        base = mix(g, vec3(0.85, 0.9, 0.95), clamp((lat - 0.8) * 8.0, 0.0, 1.0));
    } else {
        let shallow = clamp((land + 0.06) * 9.0, 0.0, 1.0);
        base = mix(vec3(0.008, 0.04, 0.14), vec3(0.03, 0.16, 0.30), shallow);
        base = mix(base, vec3(0.8, 0.88, 0.95), clamp((lat - 0.86) * 10.0, 0.0, 1.0));
    }
    // Clouds: rotate slowly about the planet's axis (scrolling).
    let ang = t * 0.0035;
    let cs = cos(ang);
    let sn = sin(ang);
    let nc = vec3(n.x * cs - n.z * sn, n.y, n.x * sn + n.z * cs);
    let cq = nc * vec3(5.0, 9.0, 5.0);
    let cloud = clamp((fbm(cq + vec3(31.0), 5) - 0.02) * 2.4, 0.0, 1.0);
    var col = mix(base, vec3(0.92), cloud * 0.85) * (0.025 + 1.3 * day * sqrt(max(ndl, 0.0)));
    // Ocean glint.
    if (!is_land) {
        let r = reflect(dir, n);
        col += vec3(1.0, 0.9, 0.7) * pow(max(dot(r, sun), 0.0), 60.0) * day * (1.0 - cloud) * 0.8;
    }
    // City lights on the night side.
    if (is_land && day < 0.6) {
        let cell = hash3(n * 160.0);
        let cluster = fbm(n * 22.0, 2);
        if (cell.x > 0.9 - cluster * 0.15 && land > 0.12) {
            col += vec3(1.0, 0.68, 0.32) * (1.0 - day / 0.6) * (1.0 - cloud * 0.8) * (0.5 + cell.y);
        }
    }
    // Atmosphere rim (fresnel), bluer on the day side.
    let rim = pow(clamp(1.0 + dot(n, dir), 0.0, 1.0), 6.0);
    col = mix(col, vec3(0.22, 0.45, 0.95) * (0.1 + 0.8 * day), clamp(rim * 0.8, 0.0, 0.7));
    return vec4(col, 1.0);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let dir = normalize(in.world_position.xyz - view.world_position);
    let sun = normalize(sp.sun_dir.xyz);
    let t = sp.sun_color.w;
    var col = vec3(0.003, 0.004, 0.012);
    let pl = planet(sp.cam.xyz, dir, sun, t);
    if (pl.w > 0.5) {
        col = pl.xyz;
    } else {
        // Nebula: a faint coloured band for depth.
        let nb = fbm(dir * 2.2 + vec3(3.0), 4);
        let band = exp(-abs(dot(dir, normalize(vec3(0.3, 0.9, -0.3)))) * 3.0);
        col += mix(vec3(0.05, 0.015, 0.08), vec3(0.01, 0.05, 0.07), nb * 0.5 + 0.5) * max(nb + 0.3, 0.0) * band * 0.6;
        if (sp.style.x > 0.5) {
            // Cybertron: a rich blue-violet nebula across the sky.
            let nb2 = fbm(dir * 3.4 + vec3(11.0), 5);
            let band2 = exp(-abs(dot(dir, normalize(vec3(-0.5, 0.7, 0.5)))) * 2.2);
            col += mix(vec3(0.02, 0.05, 0.16), vec3(0.10, 0.03, 0.14), nb2 * 0.5 + 0.5) * max(nb2 + 0.35, 0.0) * band2 * 1.1;
        }
        // Stars (streaked when launched: smeared away from the travel direction).
        let k = sp.streak.w;
        if (k > 0.01) {
            let v = sp.streak.xyz;
            let tg = v - dir * dot(v, dir);
            let tl = length(tg);
            var acc = vec3(0.0);
            if (tl > 1e-4) {
                let step = -tg / tl * (0.0009 * k);
                for (var i = 0; i < 16; i++) {
                    acc += stars(normalize(dir + step * f32(i)));
                }
                col += acc * (1.0 / 8.0);
            } else {
                col += stars(dir);
            }
        } else {
            col += stars(dir);
        }
        col += pl.xyz;
    }
    // Moon: drifts slowly across the sky as a scale reference.
    let ma = sp.cam.w;
    let md = normalize(vec3(cos(ma) * 0.82, 0.24 + 0.08 * sin(ma * 0.7), sin(ma) * 0.82));
    let mc = dot(dir, md);
    // Cybertron's second moon (smaller, rust-tinted).
    if (sp.style.x > 0.5 && pl.w < 0.5) {
        let md2 = normalize(vec3(cos(ma * 1.6 + 2.2) * 0.7, 0.42, sin(ma * 1.6 + 2.2) * 0.7));
        let mc2 = dot(dir, md2);
        if (mc2 > 0.99955) {
            let off2 = (dir - md2 * mc2) / 0.03;
            let z2 = sqrt(max(1.0 - dot(off2, off2), 0.0));
            let mn2 = normalize(off2 - md2 * z2);
            col = vec3(0.55, 0.36, 0.26) * (0.6 + 0.4 * fbm(off2 * 6.0, 2)) * (0.02 + 1.3 * max(dot(mn2, sun), 0.0));
        }
    }
    if (mc > 0.99865 && pl.w < 0.5) {
        // Sphere normal of the visible hemisphere from the offset in the moon's tangent plane.
        let off = (dir - md * mc) / 0.0519;
        let z = sqrt(max(1.0 - dot(off, off), 0.0));
        let mn = normalize(off - md * z);
        let crater = fbm(off * 7.0 + md * 3.0, 3);
        col = vec3(0.5 + 0.18 * crater) * (0.015 + 1.4 * max(dot(mn, sun), 0.0));
    }
    // Sun.
    let s = max(dot(dir, sun), 0.0);
    col += sp.sun_color.rgb * (smoothstep(0.99955, 0.9998, s) * 30.0 + pow(s, 900.0) * 4.0 + pow(s, 16.0) * 0.06);
    return vec4<f32>(col, 1.0);
}
