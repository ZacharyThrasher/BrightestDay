// Crosshair willpower gauge (bd_game power_hud::gauge): a lantern-ring band of whole SPARKS on an arc
// under the crosshair, filling left -> right in the corps colour. Thin, translucent, hard-light.
//
// p.color = corps rgb (linear) + overall opacity
// p.a     = (sparks in the pool N, sparks filled (float), bite lo, bite hi)
// p.b     = (bite cost in sparks (0 = no preview), affordable 0/1, missing sparks, charging 0/1)
// p.c     = (low 0/1, glint spark (-1 none), glint age 0..1, full flash 1..0)
// p.d     = (break lo, break hi, break age 0..1 (-1 none), ghost max)
// p.e     = (radius px, thickness px, span rad, gap rad)
// p.f     = (calm 0/1 (screen flashes off: no flicker / shake), 0, 0, 0)

#import bevy_ui::ui_vertex_output::UiVertexOutput
#import bevy_render::globals::Globals

@group(0) @binding(1) var<uniform> globals: Globals;

struct GaugeParams {
    color: vec4<f32>,
    a: vec4<f32>,
    b: vec4<f32>,
    c: vec4<f32>,
    d: vec4<f32>,
    e: vec4<f32>,
    f: vec4<f32>,
};

@group(1) @binding(0) var<uniform> p: GaugeParams;

fn over(dst: vec4<f32>, rgb: vec3<f32>, a: f32) -> vec4<f32> {
    let na = a + dst.a * (1.0 - a);
    let nc = rgb * a + dst.rgb * dst.a * (1.0 - a);
    return vec4<f32>(select(vec3<f32>(0.0), nc / na, na > 1e-5), na);
}

// Which spark a pixel (px from the centre, y down) falls in: (index, fraction along it, coverage,
// signed distance to the spark's outline in px).
fn spark_at(q: vec2<f32>) -> vec4<f32> {
    let R = p.e.x;
    let T = p.e.y;
    let span = p.e.z;
    let gap = p.e.w;
    let n = max(p.a.x, 1.0);
    let pitch = span / n;
    let r = length(q);
    // 0 = straight down, + toward the right.
    let th = atan2(q.x, q.y);
    let u = (th + span * 0.5) / pitch;
    let i = floor(u);
    let f = u - i;
    let edge = (min(f, 1.0 - f) * pitch - gap * 0.5) * r;
    let band = T * 0.5 - abs(r - R);
    let d = min(edge, band);
    return vec4<f32>(i, f, clamp(d + 0.5, 0.0, 1.0), d);
}

fn hash1(x: f32) -> f32 {
    return fract(sin(x * 91.3458) * 47453.5453);
}

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let t = globals.time;
    let calm = p.f.x > 0.5;
    let n = p.a.x;
    let have = p.a.y;
    let bite_lo = p.a.z;
    let bite_hi = p.a.w;
    let cost = p.b.x;
    let afford = p.b.y > 0.5;
    let missing = p.b.z;
    let charging = p.b.w > 0.5;
    let low = p.c.x > 0.5;
    let full = have >= n - 1e-3;
    var col = p.color.rgb;
    var q = (in.uv - vec2<f32>(0.5)) * in.size;
    // Can't afford it: the gauge shakes (not in calm mode).
    let refuse = cost > 0.0 && !afford;
    if (refuse && !calm) {
        q = q + vec2<f32>(sin(t * 47.0) * 1.7, sin(t * 31.0 + 1.3) * 0.8);
    }
    var out = vec4<f32>(0.0);
    let s = spark_at(q);
    let i = s.x;
    let f = s.y;
    let cov = s.z;
    let x = i + f; // position along the gauge, in sparks
    if (cov > 0.0 && i >= 0.0 && i < n) {
        let fill = clamp(have - i, 0.0, 1.0);
        // Dark backing: readable over snow and sky.
        out = over(out, vec3<f32>(0.015, 0.015, 0.03), cov * 0.42);
        // Empty spark: a dim track of the band.
        var c = col * 0.2 + vec3<f32>(0.03);
        var a = cov * 0.5;
        if (f < fill) {
            // Filled; a partly filled spark is dimmer until it clicks in whole.
            c = col * select(0.6, 1.0, fill >= 0.999);
            a = cov;
            if (low) {
                // Low: dim and sputter (matches the dimming constructs).
                let n1 = hash1(floor(t * 9.0));
                c = c * select(0.45 + 0.55 * n1, 0.7, calm);
            }
            if (refuse) {
                c = mix(c, vec3<f32>(1.0, 0.16, 0.1), 0.8);
            } else if (cost > 0.0 && x >= bite_lo - 1e-3 && x < bite_hi + 1e-3) {
                // The bite: the exact sparks this phrase will spend flicker light.
                let k = select(0.45 + 0.3 * sin(t * 21.0), 0.6, calm);
                c = mix(c, vec3<f32>(1.0, 0.98, 0.92), k);
            }
            // Full: a flash as it tops off.
            c = mix(c, vec3<f32>(1.0), p.c.w * 0.65);
        }
        // Charging: a shimmer rises along the band (left -> right) and the filling edge glows.
        if (charging) {
            let w = fract(t * 0.8) * (n + 2.0) - 1.0;
            let sh = exp(-(x - w) * (x - w) * 2.5);
            let edge = exp(-abs(x - have) * 9.0);
            c = c + vec3<f32>(0.55) * (sh + edge) * select(1.0, 0.6, calm);
            a = max(a, cov * min(0.5 + sh + edge, 1.0));
        }
        // Glint: a white sweep across the spark that just clicked in.
        if (p.c.y >= 0.0 && abs(i - p.c.y) < 0.5) {
            let g = p.c.z;
            let at = g * 1.4 - 0.2;
            c = mix(c, vec3<f32>(1.0), exp(-(f - at) * (f - at) * 30.0) * (1.0 - g));
        }
        // Missing sparks inside the gauge: red ghost outlines where the cast would need them.
        if (refuse && x >= have - 1e-3 && x < have + missing + 1e-3) {
            let ring = cov * (1.0 - clamp(s.w - 1.1, 0.0, 1.0));
            out = over(out, c, a);
            c = vec3<f32>(1.0, 0.3, 0.22);
            a = ring * 0.95;
        }
        out = over(out, c, a);
    } else if (cov > 0.0 && refuse && i >= n && i < n + min(p.d.w, ceil(have + missing - n))) {
        // Beyond the gauge's end: the rest of what it would take, as empty ghosts.
        let ring = cov * (1.0 - clamp(s.w - 1.1, 0.0, 1.0));
        out = over(out, vec3<f32>(1.0, 0.3, 0.22), ring * 0.85);
    }
    // Spent sparks break off and streak away down-right (toward the ring hand, never across the
    // crosshair), with a short trail.
    if (p.d.z >= 0.0) {
        let age = p.d.z;
        let dir = normalize(vec2<f32>(0.55, 0.85));
        for (var k = 0; k < 3; k = k + 1) {
            let lag = f32(k) * 0.13;
            let ak = max(age - lag, 0.0);
            let qb = q - dir * ak * ak * 34.0 - normalize(q + vec2<f32>(1e-3)) * ak * 6.0;
            let sb = spark_at(qb);
            let xb = sb.x + sb.y;
            if (sb.z > 0.0 && xb >= p.d.x - 1e-3 && xb < p.d.y + 1e-3) {
                let fade = pow(1.0 - age, 1.5) * select(1.0, 0.45, k > 0) * (1.0 - f32(k) * 0.25);
                out = over(out, mix(col, vec3<f32>(1.0, 0.97, 0.9), 0.55), sb.z * fade);
            }
        }
    }
    return vec4<f32>(out.rgb, out.a * p.color.a);
}
