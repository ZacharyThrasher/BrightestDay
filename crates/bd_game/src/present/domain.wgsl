// Domain expansions (present::domain). Linear HDR out (bloom takes everything above 1).
//  mode 0 BARRIER — one sphere, both faces: from OUTSIDE (front faces) a pitch-black orb with pulsing
//                   element-coloured veins; from INSIDE (back faces) the domain's sky, by look. The
//                   expansion closes it from the rim (ground) to the top like an eyelid (`misc.z`),
//                   with a glowing seam at the closing edge.
//  mode 1 FLOOR   — the domain's floor over the terrain (inside cameras only), by look.
//  mode 2 INK     — the black tide spreading from the caster's feet during the expansion.
//  mode 3 SHAPE   — rim silhouettes (shrine, spires, hands…): near-black with a thin coloured rim.
// Looks: 0 blade, 1 void, 2 fire, 3 water, 4 electric, 5 shadow, 6 ice, 7 light, 8 earth, 9 wind,
//        10 poison, 11 atomic.
#import bevy_pbr::{forward_io::VertexOutput, mesh_view_bindings::view}

struct Domain {
    // rgb = element colour, a = intensity (comfort)
    color: vec4<f32>,
    // xyz = centre (world), w = radius (m)
    center: vec4<f32>,
    // x = time, y = mode, z = expansion 0..1 (ink radius fraction in mode 2), w = look
    misc: vec4<f32>,
    // x = strike flash 0..1, y = clash heat 0..1, z = fade 0..1, w = unused
    extra: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> m: Domain;

fn hash31(p: vec3<f32>) -> f32 {
    var q = fract(p * 0.1031);
    q += dot(q, q.yzx + 33.33);
    return fract((q.x + q.y) * q.z);
}

fn noise3(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let n000 = hash31(i);
    let n100 = hash31(i + vec3(1.0, 0.0, 0.0));
    let n010 = hash31(i + vec3(0.0, 1.0, 0.0));
    let n110 = hash31(i + vec3(1.0, 1.0, 0.0));
    let n001 = hash31(i + vec3(0.0, 0.0, 1.0));
    let n101 = hash31(i + vec3(1.0, 0.0, 1.0));
    let n011 = hash31(i + vec3(0.0, 1.0, 1.0));
    let n111 = hash31(i + vec3(1.0, 1.0, 1.0));
    let x0 = mix(mix(n000, n100, u.x), mix(n010, n110, u.x), u.y);
    let x1 = mix(mix(n001, n101, u.x), mix(n011, n111, u.x), u.y);
    return mix(x0, x1, u.z);
}

fn fbm(p: vec3<f32>) -> f32 {
    var a = 0.5;
    var s = 0.0;
    var q = p;
    for (var i = 0; i < 4; i++) {
        s += a * noise3(q);
        q = q * 2.03 + vec3(1.7, 9.2, 3.1);
        a *= 0.5;
    }
    return s;
}

// Distance to the nearest cell edge (Voronoi F2 - F1) on a 2D plane: stained glass, lava crusts.
fn cells(p: vec2<f32>) -> vec3<f32> {
    let i = floor(p);
    var d1 = 9.0;
    var d2 = 9.0;
    var id = 0.0;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let g = i + vec2(f32(x), f32(y));
            let h = hash31(vec3(g, 7.0));
            let o = g + vec2(h, hash31(vec3(g, 3.0)));
            let d = length(p - o);
            if (d < d1) {
                d2 = d1;
                d1 = d;
                id = h;
            } else if (d < d2) {
                d2 = d;
            }
        }
    }
    return vec3(d2 - d1, id, d1);
}

// The domain's sky seen from inside, along direction `d`.
fn sky(d: vec3<f32>, look: i32, t: f32, col: vec3<f32>) -> vec3<f32> {
    let up = d.y;
    let h = clamp(up, -1.0, 1.0);
    switch look {
        case 0: { // blade: blood-red sky, a red moon, dark horizon
            var c = mix(vec3(0.12, 0.0, 0.005), vec3(0.55, 0.02, 0.02), smoothstep(-0.05, 0.7, h));
            c += vec3(0.4, 0.02, 0.0) * fbm(d * 3.0 + vec3(t * 0.05, 0.0, 0.0)) * smoothstep(0.0, 0.5, h);
            let moon = dot(d, normalize(vec3(0.35, 0.45, -0.82)));
            c += vec3(1.6, 0.25, 0.1) * smoothstep(0.988, 0.992, moon) + vec3(0.5, 0.03, 0.0) * pow(max(moon, 0.0), 40.0);
            return c;
        }
        case 1: { // void: an infinite starfield, washed in white light
            let s = hash31(floor(d * 260.0));
            var c = vec3(0.004, 0.004, 0.012) + vec3(1.0) * step(0.9965, s) * (1.0 + 2.0 * hash31(floor(d * 260.0) + 3.0));
            c += vec3(0.25, 0.22, 0.45) * pow(fbm(d * 2.0 + vec3(0.0, t * 0.02, 0.0)), 3.0) * 2.0;
            let glow = pow(max(dot(d, normalize(vec3(0.0, 1.0, 0.2))), 0.0), 6.0);
            c += vec3(2.5, 2.5, 2.8) * glow * (0.8 + 0.2 * sin(t * 1.3));
            return c;
        }
        case 2: { // fire: a burning sky
            let f = fbm(d * 3.5 + vec3(0.0, -t * 0.6, t * 0.2));
            return mix(vec3(0.35, 0.03, 0.0), vec3(1.6, 0.45, 0.05), f * smoothstep(-0.3, 0.6, h)) + vec3(1.8, 0.7, 0.1) * pow(1.0 - abs(h), 12.0);
        }
        case 3: { // water: endless ocean, a bright horizon
            if (up < 0.0) {
                let w = fbm(vec3(d.xz / max(-up, 0.05) * 0.6, t * 0.3));
                return mix(vec3(0.0, 0.05, 0.12), vec3(0.1, 0.35, 0.55), w) + vec3(0.8, 0.95, 1.0) * pow(1.0 + up, 60.0);
            }
            return mix(vec3(0.75, 0.9, 1.0), vec3(0.15, 0.45, 0.85), smoothstep(0.0, 0.6, h));
        }
        case 4: { // electric: a storm cell, constant lightning
            let cl = fbm(d * 4.0 + vec3(t * 0.3, 0.0, t * 0.1));
            let k = floor(t * 4.0);
            let flash = step(0.55, hash31(vec3(k, 1.0, 2.0))) * exp(-fract(t * 4.0) * 6.0);
            var c = mix(vec3(0.02, 0.02, 0.06), vec3(0.18, 0.16, 0.35), cl) * (1.0 + 4.0 * flash);
            let bolt = abs(sin(d.x * 30.0 + noise3(d * 12.0 + k) * 6.0));
            c += vec3(1.5, 1.6, 2.5) * smoothstep(0.03, 0.0, bolt) * flash * step(0.1, up);
            return c;
        }
        case 5: { // shadow: an abyss
            return vec3(0.004, 0.0, 0.01) + vec3(0.12, 0.02, 0.2) * pow(fbm(d * 2.5 + vec3(0.0, t * 0.1, 0.0)), 2.0) * smoothstep(0.4, -0.4, h);
        }
        case 6: { // ice: frozen, an aurora
            let a = sin(d.x * 6.0 + fbm(d * 3.0 + t * 0.1) * 5.0 + t * 0.5);
            return mix(vec3(0.6, 0.8, 0.9), vec3(0.08, 0.2, 0.4), smoothstep(0.0, 0.8, h)) + vec3(0.2, 1.2, 0.8) * smoothstep(0.7, 1.0, a) * smoothstep(0.2, 0.6, h);
        }
        case 7: { // light: stained-glass cathedral light
            let uv = vec2(atan2(d.z, d.x) * 3.0, h * 5.0);
            let v = cells(uv * 1.6);
            let glass = vec3(hash31(vec3(v.y, 1.0, 0.0)), hash31(vec3(v.y, 2.0, 0.0)), hash31(vec3(v.y, 3.0, 0.0)));
            let lead = smoothstep(0.02, 0.07, v.x);
            return (glass * 1.4 + vec3(0.3, 0.25, 0.1)) * lead * (0.6 + 0.4 * h) + vec3(2.0, 1.8, 1.2) * pow(max(h, 0.0), 8.0);
        }
        case 8: { // earth: a dark dusk behind obsidian spires
            return mix(vec3(0.05, 0.02, 0.01), vec3(0.45, 0.15, 0.05), smoothstep(-0.1, 0.5, h) * (0.6 + 0.4 * fbm(d * 3.0)));
        }
        case 9: { // wind: endless sky above the clouds
            if (up < 0.05) {
                let c = fbm(vec3(d.xz / max(0.1 - up, 0.05) * 0.5, t * 0.05));
                return mix(vec3(0.7, 0.8, 0.95), vec3(1.2, 1.2, 1.25), smoothstep(0.35, 0.7, c));
            }
            return mix(vec3(0.55, 0.75, 1.0), vec3(0.1, 0.3, 0.8), smoothstep(0.0, 0.8, h));
        }
        case 10: { // poison: a toxic swamp haze
            return mix(vec3(0.05, 0.12, 0.02), vec3(0.3, 0.55, 0.08), fbm(d * 3.0 + vec3(0.0, t * 0.15, 0.0)) * smoothstep(-0.2, 0.6, h));
        }
        default: { // atomic: a glowing reactor
            let ring = 0.5 + 0.5 * sin(h * 30.0 - t * 4.0);
            return mix(vec3(0.02, 0.1, 0.03), vec3(0.4, 1.4, 0.3), pow(ring, 6.0) * 0.6 + 0.2 * fbm(d * 4.0 + t * 0.2)) + vec3(1.2, 2.0, 0.6) * pow(max(h, 0.0), 10.0);
        }
    }
}

// The domain's floor at world point `p` (inside cameras only).
fn floor_col(p: vec3<f32>, v: vec3<f32>, look: i32, t: f32, col: vec3<f32>) -> vec4<f32> {
    let xz = p.xz;
    let refl = sky(reflect(-v, vec3(0.0, 1.0, 0.0)), look, t, col);
    let fres = pow(1.0 - clamp(v.y, 0.0, 1.0), 3.0);
    switch look {
        case 0: { // dark reflective blood-water
            let rip = 0.5 + 0.5 * sin(length(xz - vec2(3.0, -2.0)) * 3.0 - t * 2.0);
            return vec4(vec3(0.06, 0.0, 0.0) + refl * (0.35 + 0.5 * fres) * (0.9 + 0.1 * rip), 0.93);
        }
        case 1: { // void: a black mirror full of stars
            let s = step(0.996, hash31(vec3(floor(xz * 3.0), 1.0)));
            return vec4(vec3(0.005, 0.005, 0.015) + vec3(1.5) * s + refl * 0.3, 0.95);
        }
        case 2: { // lava plain: glowing cracks between crusts
            let c = cells(xz * 0.35);
            let glow = smoothstep(0.12, 0.0, c.x) * (0.8 + 0.4 * sin(t * 2.0 + c.y * 20.0));
            return vec4(mix(vec3(0.05, 0.02, 0.02), vec3(3.0, 0.8, 0.1), glow), 0.95);
        }
        case 3: { // ocean surface
            let w = fbm(vec3(xz * 0.25, t * 0.4));
            return vec4(mix(vec3(0.0, 0.06, 0.14), vec3(0.1, 0.4, 0.6), w) + refl * 0.4 * fres, 0.9);
        }
        case 6: { // frozen mirror lake
            let crack = smoothstep(0.03, 0.0, cells(xz * 0.4).x);
            return vec4(vec3(0.5, 0.7, 0.8) * 0.3 + refl * 0.7 + vec3(0.9, 1.0, 1.1) * crack * 0.5, 0.93);
        }
        case 7: { // cathedral floor: white marble, gold inlay
            let c = cells(xz * 0.5);
            return vec4(mix(vec3(0.75, 0.7, 0.6), vec3(1.4, 1.0, 0.4), smoothstep(0.04, 0.0, c.x)) + refl * 0.15, 0.9);
        }
        case 8: { // obsidian, red cracks
            let c = cells(xz * 0.3);
            return vec4(vec3(0.02, 0.015, 0.02) + refl * 0.25 * fres + vec3(1.5, 0.2, 0.05) * smoothstep(0.03, 0.0, c.x), 0.95);
        }
        case 9: { // walking on clouds
            let c = fbm(vec3(xz * 0.15, t * 0.05));
            return vec4(mix(vec3(0.75, 0.8, 0.9), vec3(1.1, 1.1, 1.15), c), 0.92);
        }
        case 10: { // toxic sludge
            let b = step(0.97, hash31(vec3(floor(xz * 1.5), floor(t * 2.0))));
            return vec4(mix(vec3(0.08, 0.2, 0.02), vec3(0.3, 0.6, 0.05), fbm(vec3(xz * 0.3, t * 0.2))) + vec3(0.6, 1.2, 0.2) * b, 0.92);
        }
        case 11: { // reactor grid
            let g = abs(fract(xz * 0.25) - 0.5);
            let line = smoothstep(0.03, 0.0, min(g.x, g.y));
            return vec4(vec3(0.02, 0.06, 0.02) + vec3(0.5, 2.0, 0.4) * line * (0.7 + 0.3 * sin(t * 3.0)), 0.93);
        }
        default: { // electric / shadow: dark wet stone with the sky in it
            return vec4(vec3(0.015, 0.012, 0.02) + refl * 0.35 * fres, 0.94);
        }
    }
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    let p = in.world_position.xyz;
    let c = m.center.xyz;
    let r = max(m.center.w, 0.01);
    let t = m.misc.x;
    let mode = i32(m.misc.y + 0.5);
    let prog = m.misc.z;
    let look = i32(m.misc.w + 0.5);
    let k = m.color.a;
    let col = m.color.rgb;
    let v = normalize(view.world_position - p);
    if (mode == 0) {
        let rel = (p - c) / r;
        // The eyelid: closes from the rim (ground) toward the top.
        if (rel.y > prog * 1.02 - 0.01) {
            discard;
        }
        let seam = smoothstep(0.08, 0.0, prog - rel.y) * step(prog, 0.995);
        if (front) {
            // Outside: pitch black, pulsing element veins, a faint rim.
            let n = fbm(rel * 3.2 + vec3(0.0, t * 0.12, 0.0));
            let vein = smoothstep(0.035, 0.0, abs(n - 0.5)) + 0.5 * smoothstep(0.02, 0.0, abs(fbm(rel * 7.0 - t * 0.05) - 0.5));
            let pulse = 0.55 + 0.45 * sin(t * 2.2 - length(rel.xz) * 6.0 + n * 8.0);
            let fres = pow(1.0 - clamp(abs(dot(normalize(in.world_normal), v)), 0.0, 1.0), 4.0);
            let clash = m.extra.y;
            var o = vec3(0.004) + col * vein * (1.2 + 2.8 * pulse) * k + col * fres * 0.35 * k;
            o += col * seam * 6.0 * k + vec3(1.0, 0.95, 0.9) * clash * vein * 4.0 * k;
            return vec4(o, 1.0);
        }
        // Inside: the domain's sky (+ the closing seam while it seals).
        let d = normalize(p - view.world_position);
        let s = sky(d, look, t, col) * (0.85 + 0.15 * k);
        return vec4(s + col * seam * 4.0 * k + vec3(1.0) * m.extra.x * 0.5, 1.0);
    }
    if (mode == 1) {
        let f = floor_col(p, v, look, t, col);
        let edge = smoothstep(r, r * 0.9, length(p.xz - c.xz));
        return vec4(f.rgb * (0.85 + 0.15 * k), f.a * edge * m.extra.z);
    }
    if (mode == 2) {
        // Ink: a black tide with a boiling, uneven edge.
        let d = length(p.xz - c.xz) / r;
        let ang = atan2(p.z - c.z, p.x - c.x);
        let edge = prog * (0.92 + 0.12 * fbm(vec3(ang * 2.0, d * 3.0, t * 1.5)));
        if (d > edge) {
            discard;
        }
        let rim = smoothstep(edge - 0.04, edge, d);
        return vec4(vec3(0.003) + col * rim * 2.5 * k, 0.96 * m.extra.z);
    }
    // Rim silhouettes.
    let fres = pow(1.0 - clamp(abs(dot(normalize(in.world_normal), v)), 0.0, 1.0), 3.0);
    return vec4(vec3(0.006) + col * fres * 0.8 * k, 1.0);
}
