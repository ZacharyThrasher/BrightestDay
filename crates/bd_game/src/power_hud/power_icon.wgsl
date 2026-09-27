// Power HUD icon (bd_game power_hud::strip): one wordless icon per held power / status / pickup,
// plus the slim willpower arc. Colour always comes paired with a glyph SHAPE (colour-blind safe).
//
// params.color = rgb (linear) + opacity
// params.a     = (glyph id, fill 0..1 (ready / time left), form (-1 none, else 0..1), flash 0..1)
// params.b     = (mode: 0 power, 1 status pip, 2 willpower arc, 3 pickup,
//                 active 0/1, dim 0/1 (drained), extra: power = form colour mix, arc = cost preview)
// params.c     = form colour (rgb) + time seed

#import bevy_ui::ui_vertex_output::UiVertexOutput
#import bevy_render::globals::Globals

@group(0) @binding(1) var<uniform> globals: Globals;

struct IconParams {
    color: vec4<f32>,
    a: vec4<f32>,
    b: vec4<f32>,
    c: vec4<f32>,
};

@group(1) @binding(0) var<uniform> p: IconParams;

const TAU: f32 = 6.28318530718;

fn sd_circle(q: vec2<f32>, r: f32) -> f32 {
    return length(q) - r;
}

fn sd_box(q: vec2<f32>, b: vec2<f32>) -> f32 {
    let d = abs(q) - b;
    return length(max(d, vec2<f32>(0.0))) + min(max(d.x, d.y), 0.0);
}

fn sd_segment(q: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let pa = q - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - ba * h);
}

fn ndot(a: vec2<f32>, b: vec2<f32>) -> f32 {
    return a.x * b.x - a.y * b.y;
}

fn sd_rhombus(q0: vec2<f32>, b: vec2<f32>) -> f32 {
    let q = abs(q0);
    let h = clamp(ndot(b - 2.0 * q, b) / dot(b, b), -1.0, 1.0);
    let d = length(q - 0.5 * b * vec2<f32>(1.0 - h, 1.0 + h));
    return d * sign(q.x * b.y + q.y * b.x - b.x * b.y);
}

fn sd_tri(q0: vec2<f32>, r: f32) -> f32 {
    let k = sqrt(3.0);
    var q = q0;
    q.x = abs(q.x) - r;
    q.y = q.y + r / k;
    if (q.x + k * q.y > 0.0) {
        q = vec2<f32>(q.x - k * q.y, -k * q.x - q.y) / 2.0;
    }
    q.x = q.x - clamp(q.x, -2.0 * r, 0.0);
    return -length(q) * sign(q.y);
}

fn sd_hex(q0: vec2<f32>, r: f32) -> f32 {
    let k = vec3<f32>(-0.866025404, 0.5, 0.577350269);
    var q = abs(q0);
    q = q - 2.0 * min(dot(k.xy, q), 0.0) * k.xy;
    q = q - vec2<f32>(clamp(q.x, -k.z * r, k.z * r), r);
    return length(q) * sign(q.y);
}

fn rot45(q: vec2<f32>) -> vec2<f32> {
    let s = 0.70710678;
    return vec2<f32>(q.x * s - q.y * s, q.x * s + q.y * s);
}

// Glyph SDF (q in -1..1, y up; r = glyph radius).
fn glyph(id: i32, q: vec2<f32>, r: f32) -> f32 {
    var d: f32 = 1.0;
    switch id {
        case 0: { d = sd_circle(q, r * 0.78); }
        case 1: { d = sd_rhombus(q, vec2<f32>(r * 0.78, r)); }
        case 2: { d = sd_tri(q - vec2<f32>(0.0, -r * 0.12), r * 0.9); }
        case 3: { d = min(sd_rhombus(q, vec2<f32>(r, r * 0.3)), sd_rhombus(q, vec2<f32>(r * 0.3, r))); }
        case 4: { d = sd_hex(q, r * 0.8); }
        case 5: {
            // Three claw slashes.
            let w = r * 0.13;
            let d0 = sd_segment(q, vec2<f32>(-r * 0.75, r * 0.55), vec2<f32>(-r * 0.2, -r * 0.8));
            let d1 = sd_segment(q, vec2<f32>(-r * 0.2, r * 0.8), vec2<f32>(r * 0.3, -r * 0.8));
            let d2 = sd_segment(q, vec2<f32>(r * 0.35, r * 0.8), vec2<f32>(r * 0.8, -r * 0.5));
            d = min(min(d0, d1), d2) - w;
        }
        case 6: { d = max(sd_circle(q, r * 0.85), -sd_circle(q - vec2<f32>(r * 0.42, r * 0.25), r * 0.72)); }
        case 7: { d = min(sd_box(q, vec2<f32>(r * 0.85, r * 0.24)), sd_box(q, vec2<f32>(r * 0.24, r * 0.85))); }
        case 8: {
            let w = r * 0.17;
            d = min(sd_segment(q, vec2<f32>(-r * 0.75, -r * 0.35), vec2<f32>(0.0, r * 0.45)), sd_segment(q, vec2<f32>(0.0, r * 0.45), vec2<f32>(r * 0.75, -r * 0.35))) - w;
        }
        case 9: {
            let w = r * 0.17;
            d = min(sd_segment(q, vec2<f32>(-r * 0.75, r * 0.35), vec2<f32>(0.0, -r * 0.45)), sd_segment(q, vec2<f32>(0.0, -r * 0.45), vec2<f32>(r * 0.75, r * 0.35))) - w;
        }
        case 10: {
            let body = sd_circle(q - vec2<f32>(0.0, -r * 0.28), r * 0.58);
            let tip = sd_tri(q - vec2<f32>(0.0, r * 0.2), r * 0.52);
            d = min(body, tip);
        }
        case 11: {
            let w = r * 0.14;
            let a = vec2<f32>(r * 0.25, r * 0.95);
            let b = vec2<f32>(-r * 0.3, r * 0.02);
            let c = vec2<f32>(r * 0.3, -r * 0.02);
            let e = vec2<f32>(-r * 0.25, -r * 0.95);
            d = min(min(sd_segment(q, a, b), sd_segment(q, b, c)), sd_segment(q, c, e)) - w;
        }
        case 12: { d = abs(sd_circle(q, r * 0.62)) - r * 0.17; }
        case 13: {
            let t = rot45(q);
            d = min(sd_box(t, vec2<f32>(r * 0.9, r * 0.22)), sd_box(t, vec2<f32>(r * 0.22, r * 0.9)));
        }
        default: { d = sd_circle(q, r * 0.7); }
    }
    return d;
}

// Coverage of an SDF with ~1 px antialiasing (px = SDF units per pixel).
fn cover(d: f32, px: f32) -> f32 {
    return clamp(0.5 - d / px, 0.0, 1.0);
}

// Clockwise angle from 12 o'clock, 0..1.
fn clock(q: vec2<f32>) -> f32 {
    return fract(atan2(q.x, q.y) / TAU + 1.0);
}

fn over(dst: vec4<f32>, rgb: vec3<f32>, a: f32) -> vec4<f32> {
    // Premultiplied "over", kept straight-alpha at the end.
    let na = a + dst.a * (1.0 - a);
    let nc = rgb * a + dst.rgb * dst.a * (1.0 - a);
    return vec4<f32>(select(vec3<f32>(0.0), nc / na, na > 1e-5), na);
}

fn grey(c: vec3<f32>) -> vec3<f32> {
    let l = dot(c, vec3<f32>(0.299, 0.587, 0.114));
    return vec3<f32>(l);
}

fn icon(in: UiVertexOutput) -> vec4<f32> {
    let q = (in.uv - vec2<f32>(0.5)) * vec2<f32>(2.0, -2.0);
    let px = 2.0 / max(in.size.x, 1.0);
    let t = globals.time + p.c.w;
    let mode = i32(p.b.x + 0.5);
    let gid = i32(p.a.x + 0.5);
    let fill = clamp(p.a.y, 0.0, 1.0);
    let form = p.a.z;
    let flash = clamp(p.a.w, 0.0, 1.0);
    let is_active = p.b.y;
    let dim = p.b.z;
    var col = p.color.rgb;
    if (dim > 0.5) {
        col = grey(col) * (0.55 + 0.1 * sin(t * 2.0));
    }
    let ang = clock(q);
    let r = length(q);
    let lit = ang <= fill || fill >= 0.999;
    var out = vec4<f32>(0.0);

    // Backing disc (contrast on any sky / map).
    out = over(out, vec3<f32>(0.02, 0.02, 0.035), cover(sd_circle(q, 0.8), px) * 0.62);

    // Sweep rim: the refilled part lit in colour, the rest a dim track.
    let rim_w = select(0.055, 0.075, is_active > 0.5);
    let rim = abs(r - 0.76) - rim_w;
    let rim_c = select(vec3<f32>(0.28), col, lit);
    out = over(out, rim_c, cover(rim, px) * select(0.55, 1.0, lit));

    // The glyph: dim + grey until it has refilled past it (clock wipe), then full colour.
    let gr = select(0.5, 0.42, mode == 1);
    let gd = glyph(gid, q, gr);
    var gc = select(mix(grey(col) * 0.45, col * 0.5, 0.35), col, lit);
    // Ready and whole: a soft halo.
    if (fill >= 0.999) {
        let halo = exp(-max(gd, 0.0) * 9.0) * 0.35;
        out = over(out, col, halo * (1.0 - cover(gd, px)));
    }
    // Burning (a form is on): flicker hot, whitened toward the form's aura.
    if (form >= 0.0) {
        let fl = 0.85 + 0.15 * sin(t * 13.0) * sin(t * 7.1 + 1.3);
        gc = mix(gc, mix(p.c.rgb, vec3<f32>(1.0, 0.95, 0.8), 0.35), 0.45) * (1.0 + 0.35 * fl);
        let ember = exp(-max(gd, 0.0) * 6.0) * 0.4 * fl;
        out = over(out, p.c.rgb, ember * (1.0 - cover(gd, px)));
    }
    gc = mix(gc, vec3<f32>(1.0), flash * 0.75);
    out = over(out, min(gc, vec3<f32>(1.6)), cover(gd, px));

    // Form duration ring (outer, draining clockwise).
    if (form >= 0.0) {
        let fr = abs(r - 0.93) - 0.05;
        let on = ang <= form || form >= 0.999;
        let fc = mix(p.c.rgb, vec3<f32>(1.0, 0.92, 0.7), 0.4 + 0.2 * sin(t * 9.0));
        out = over(out, select(vec3<f32>(0.2), fc, on), cover(fr, px) * select(0.3, 1.0, on));
    }

    // Ready flare: a ring expanding out of the icon.
    if (flash > 0.0) {
        let e = 1.0 - flash;
        let fr = abs(r - (0.55 + 0.45 * e)) - 0.06 * flash;
        out = over(out, vec3<f32>(1.0), cover(fr, px) * flash);
    }

    // Pickup: a slow glint orbiting its rim.
    if (mode == 3) {
        let g = vec2<f32>(sin(t * 1.7), cos(t * 1.7)) * 0.76;
        out = over(out, vec3<f32>(1.0), exp(-length(q - g) * 14.0));
    }
    return vec4<f32>(out.rgb, out.a * p.color.a);
}

// The slim willpower arc under the icons: a shallow smile, filled left → right.
fn arc(in: UiVertexOutput) -> vec4<f32> {
    let size = in.size;
    let pxy = (in.uv - vec2<f32>(0.5, 0.0)) * size;
    let half_w = size.x * 0.5;
    let R = half_w * 2.2;
    let c = vec2<f32>(0.0, size.y * 0.3 - R);
    let d = length(pxy - c) - R;
    let thick = max(size.y * 0.13, 1.5);
    let band = abs(d) - thick;
    let u = clamp(pxy.x / half_w * 0.5 + 0.5, 0.0, 1.0);
    let ends = smoothstep(0.0, 0.06, u) * smoothstep(1.0, 0.94, u);
    let t = globals.time + p.c.w;
    let fill = clamp(p.a.y, 0.0, 1.0);
    let cost = p.b.w;
    let a = clamp(0.5 - band, 0.0, 1.0) * ends;
    var out = vec4<f32>(0.02, 0.02, 0.035, 0.55 * a);
    var col = p.color.rgb;
    if (fill < 0.2) {
        col = col * (0.6 + 0.4 * abs(sin(t * 9.0)));
    }
    if (u <= fill) {
        out = over(out, col, a);
    }
    // Cost preview: the slice the typed phrase would spend flickers; unaffordable flickers warm.
    if (cost > 0.0) {
        let k = 0.55 + 0.45 * sin(t * 18.0);
        if (cost <= fill + 1e-4) {
            if (u > fill - cost && u <= fill) {
                out = over(out, vec3<f32>(1.0, 0.97, 0.9), a * k);
            }
        } else if (u <= fill) {
            out = over(out, vec3<f32>(1.0, 0.45, 0.2), a * k);
        }
    }
    return vec4<f32>(out.rgb, out.a * p.color.a);
}

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    if (i32(p.b.x + 0.5) == 2) {
        return arc(in);
    }
    return icon(in);
}
