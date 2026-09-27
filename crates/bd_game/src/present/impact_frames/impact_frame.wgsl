// Impact frame (present/impact_frames): the anime freeze-frame treatment, one fullscreen pass on
// the HDR scene (after bloom, before tonemapping).
//
// scene → zoom toward the impact → chromatic split (radial) → luminance → posterized duotone in the
// style's ink/paper (optionally inverted) with ink edges → burned-in hot silhouettes (bodies are
// flashed to emissive ~7 during the hold) → halftone / brush grain → radial speed lines → bent
// lightning → crack → comic panel burst → vignette → mixed over the scene by strength × mask
// (mask = the whole screen, or a world-anchored bubble for bystanders + a shockwave ring).

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

struct ImpactFrame {
    a: vec4<f32>,      // center uv xy, bubble radius (screen-height units, 0 = full), feather
    b: vec4<f32>,      // strength, phase, zoom (>= 1), aspect
    ink: vec4<f32>,    // rgb, invert
    paper: vec4<f32>,  // rgb, threshold
    accent: vec4<f32>, // rgb, chroma
    fx1: vec4<f32>,    // lines, line density, lightning, crack
    fx2: vec4<f32>,    // halftone, glitch, brush, seed
    fx3: vec4<f32>,    // hit dir xy (screen), hot luma, lightning ink core
    fx4: vec4<f32>,    // vignette, ring, exposure, contrast
    fx5: vec4<f32>,    // levels, softness, edges, panel
}

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var texture_sampler: sampler;
@group(0) @binding(2) var<uniform> f: ImpactFrame;

const TAU: f32 = 6.28318530718;

fn hash11(x: f32) -> f32 {
    return fract(sin(x * 127.1 + 11.7) * 43758.5453);
}

fn hash21(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn vnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let u = fract(p);
    let s = u * u * (3.0 - 2.0 * u);
    let a = hash21(i);
    let b = hash21(i + vec2<f32>(1.0, 0.0));
    let c = hash21(i + vec2<f32>(0.0, 1.0));
    let d = hash21(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, s.x), mix(c, d, s.x), s.y);
}

fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

// Compressed luminance (HDR → 0..1, perceptual-ish).
fn lp(c: vec3<f32>) -> f32 {
    let l = max(luma(c), 0.0);
    return sqrt(l / (1.0 + l));
}

fn tap(uv: vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(screen_texture, texture_sampler, clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0).rgb;
}

// Angular distance wrapped to [-PI, PI].
fn adiff(a: f32, b: f32) -> f32 {
    return (fract((a - b) / TAU + 0.5) - 0.5) * TAU;
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let scene = textureSampleLevel(screen_texture, texture_sampler, uv, 0.0);
    let strength = f.b.x;
    if (uv.x < 0.5) { return vec4<f32>(1.0, 0.0, 0.0, 1.0); } // DEBUG
    if (strength <= 0.0005) {
        return scene;
    }
    return vec4<f32>(0.0, 0.0, 1.0, 1.0); // DEBUG
    let center = f.a.xy;
    let aspect = f.b.w;
    let seed = f.fx2.w;
    var d = uv - center;
    d.x = d.x * aspect;
    let r = length(d);
    let ang = atan2(d.y, d.x);

    // Mask: full screen, or a bubble around the impact (bystanders) + its shockwave ring.
    let radius = f.a.z;
    var mask = 1.0;
    var ring = 0.0;
    if (radius > 0.0) {
        let wob = 1.0 + 0.06 * (vnoise(vec2<f32>(ang * 3.0, seed * 7.0)) - 0.5);
        let rr = radius * wob;
        mask = 1.0 - smoothstep(rr * (1.0 - f.a.w), rr, r);
        if (f.fx4.y >= 0.0) {
            let ringr = radius * (0.9 + 1.4 * f.fx4.y);
            let w = 0.004 + 0.02 * radius;
            ring = exp(-pow((r - ringr) / w, 2.0)) * (1.0 - f.fx4.y);
        }
    }
    if (mask <= 0.001 && ring <= 0.001) {
        return scene;
    }

    // Zoom toward the impact (a lean in during the hold, snapping out on release).
    let zoom = max(f.b.z, 1.0);
    var suv = center + (uv - center) / zoom;

    // Glitch: torn horizontal blocks + a stepped channel offset.
    let glitch = f.fx2.y;
    if (glitch > 0.0) {
        let row = floor(uv.y * 28.0);
        let h = hash11(row * 1.37 + seed * 17.0);
        if (h > 0.62) {
            suv.x = suv.x + (hash11(row * 3.1 + seed) - 0.5) * 0.09 * glitch;
        }
        let block = floor(uv * vec2<f32>(9.0, 16.0));
        if (hash21(block + seed * 3.0) > 0.93) {
            suv = suv + (vec2<f32>(hash21(block + 1.3), hash21(block + 7.7)) - 0.5) * 0.05 * glitch;
        }
    }
    // Brush: wobbling ink edges.
    let brush = f.fx2.z;
    if (brush > 0.0) {
        let n = vec2<f32>(vnoise(uv * vec2<f32>(14.0 * aspect, 14.0) + seed), vnoise(uv * vec2<f32>(14.0 * aspect, 14.0) + seed + 19.0));
        suv = suv + (n - 0.5) * 0.01 * brush;
    }

    // Chromatic split along the radial direction (grows toward the edges), skewed by the hit.
    var dirn = vec2<f32>(0.0);
    if (r > 1e-4) {
        dirn = normalize(uv - center);
    }
    dirn = normalize(dirn + f.fx3.xy * 0.35 + vec2<f32>(1e-5, 0.0));
    let ca = f.accent.w * (0.3 + r);
    let cc = tap(suv);
    let cr = tap(suv + dirn * ca).r;
    let cb = tap(suv - dirn * ca).b;
    let split = vec3<f32>(cr, cc.g, cb);

    // Tone: posterized, contrast-blended compressed luminance with ink edges.
    let l0 = lp(split);
    let dims = vec2<f32>(textureDimensions(screen_texture));
    let px = 1.5 / dims;
    let e = abs(lp(tap(suv + vec2<f32>(px.x, 0.0))) - lp(tap(suv - vec2<f32>(px.x, 0.0))))
          + abs(lp(tap(suv + vec2<f32>(0.0, px.y))) - lp(tap(suv - vec2<f32>(0.0, px.y))));
    let edge = smoothstep(0.06, 0.2, e) * f.fx5.z;
    let thr = f.paper.w;
    let soft = max(f.fx5.y, 0.002);
    let levels = max(f.fx5.x, 2.0);
    var tone = smoothstep(thr - soft, thr + soft, l0);
    if (levels > 2.0) {
        // Washes: extra steps between ink and paper (sumi).
        let q = floor(clamp(l0 / max(thr * 2.0, 0.05), 0.0, 0.999) * levels) / (levels - 1.0);
        tone = mix(tone, q, 0.65);
    }
    tone = mix(l0, tone, f.fx4.w);
    tone = tone * (1.0 - edge);

    // Halftone (comic): a 45° dot screen whose dots grow in the shadows.
    let halftone = f.fx2.x;
    if (halftone > 0.0) {
        let pxs = uv * dims;
        let rot = vec2<f32>(pxs.x + pxs.y, pxs.x - pxs.y) * 0.7071;
        let cell = 7.0;
        let g = fract(rot / cell) - 0.5;
        let dot_r = sqrt(clamp(1.0 - tone, 0.0, 1.0)) * 0.62;
        let in_dot = 1.0 - smoothstep(dot_r - 0.06, dot_r + 0.06, length(g));
        let ht_tone = select(1.0, 1.0 - in_dot, tone < 0.98);
        tone = mix(tone, ht_tone * step(0.12, tone), halftone);
    }

    // Burned-in silhouettes: the hot bodies (and hot fx) key out as the brightest tone.
    let hot = smoothstep(f.fx3.z * 0.7, f.fx3.z, luma(cc));
    tone = max(tone, hot);
    let invert = f.ink.w;
    tone = mix(tone, 1.0 - tone, invert);

    var col = mix(f.ink.rgb, f.paper.rgb, tone);
    // A rim of accent around the silhouettes.
    let rim = hot * (1.0 - hot) * 4.0;
    col = mix(col, f.accent.rgb, clamp(rim, 0.0, 1.0) * 0.8);

    // Brush grain + splats.
    if (brush > 0.0) {
        let grain = vnoise(uv * dims * 0.35) * 0.5 + vnoise(uv * dims * 0.08) * 0.5;
        col = col * (1.0 - 0.18 * brush * grain);
        let sp = vnoise(d * 9.0 + seed * 5.0);
        let splat = smoothstep(0.78, 0.8, sp) * (1.0 - smoothstep(0.15, 0.55, r));
        col = mix(col, f.ink.rgb, splat * brush);
    }

    // Radial speed lines (ink) rushing to the impact.
    let lines = f.fx1.x;
    if (lines > 0.0) {
        let n = 110.0;
        let s = (ang / TAU + 0.5) * n;
        let cell = floor(s);
        let h = hash11(cell + seed * 31.0);
        let h2 = hash11(cell * 7.3 + seed);
        let r_in = 0.12 + 0.28 * h2;
        let across = abs(fract(s) - 0.5) * 2.0;
        let w = (0.25 + 0.55 * hash11(cell * 3.3)) * smoothstep(r_in, r_in + 0.45, r);
        let on = step(1.0 - f.fx1.y, h) * (1.0 - smoothstep(w * 0.6, w, across)) * smoothstep(r_in, r_in + 0.08, r);
        col = mix(col, f.ink.rgb, on * lines);
    }

    // Bent lightning: bolts radiating from the impact, jagged, bending through distorted space.
    let lightning = f.fx1.z;
    if (lightning > 0.0) {
        var glow = 0.0;
        var core = 0.0;
        for (var k = 0; k < 7; k = k + 1) {
            let kf = f32(k);
            let base = seed * TAU + kf * 0.8976 + (hash11(kf + seed * 3.0) - 0.5) * 0.7;
            let len = 0.25 + 0.55 * hash11(kf * 5.1 + seed);
            let bend = sin(r * 7.0 + kf * 2.1 + seed * 5.0) * 0.35 * r;
            let jag = (vnoise(vec2<f32>(r * 22.0 + kf * 9.0, seed * 3.0 + kf)) - 0.5) * 0.5
                    + (vnoise(vec2<f32>(r * 70.0, kf * 4.0 + seed)) - 0.5) * 0.14;
            let da = adiff(ang, base + bend + jag) * max(r, 0.02);
            let fade = 1.0 - smoothstep(len * 0.7, len, r);
            let wcore = 0.0022 + 0.004 * (1.0 - r);
            core = max(core, (1.0 - smoothstep(wcore * 0.5, wcore, abs(da))) * fade);
            glow = max(glow, exp(-abs(da) * 90.0) * fade);
        }
        let core_col = mix(f.accent.rgb * 2.5, f.ink.rgb, f.fx3.w);
        col = mix(col, f.accent.rgb * 2.0, clamp(glow * lightning, 0.0, 1.0));
        col = mix(col, core_col, clamp(core * lightning, 0.0, 1.0));
    }

    // Screen crack: jagged radial fractures + a broken ring near the impact.
    let crack = f.fx1.w;
    if (crack > 0.0) {
        var c = 0.0;
        for (var k = 0; k < 9; k = k + 1) {
            let kf = f32(k);
            let base = seed * 4.0 + kf * 0.698 + (hash11(kf * 2.7 + seed) - 0.5) * 0.5;
            let jag = (vnoise(vec2<f32>(r * 30.0 + kf * 5.0, seed + kf)) - 0.5) * 0.45;
            let da = adiff(ang, base + jag) * max(r, 0.02);
            let len = 0.2 + 0.5 * hash11(kf * 9.1 + seed);
            c = max(c, (1.0 - smoothstep(0.0012, 0.0028, abs(da))) * (1.0 - smoothstep(len * 0.8, len, r)));
        }
        let ringd = abs(r - (0.07 + 0.02 * vnoise(vec2<f32>(ang * 4.0, seed))));
        let seg = step(0.45, vnoise(vec2<f32>(ang * 6.0, seed * 2.0)));
        c = max(c, (1.0 - smoothstep(0.001, 0.003, ringd)) * seg);
        col = mix(col, f.accent.rgb * 1.6, c * crack);
    }

    // Comic panel: a jagged starburst behind the impact (accent fill, ink outline).
    let panel = f.fx5.w;
    if (panel > 0.0) {
        let spikes = 14.0;
        let sa = fract((ang / TAU + 0.5) * spikes + seed);
        let rad = 0.16 * (0.75 + 0.45 * abs(sa - 0.5) * 2.0) * (0.9 + 0.2 * hash11(floor((ang / TAU + 0.5) * spikes) + seed));
        let inside = 1.0 - smoothstep(rad - 0.003, rad, r);
        let outline = smoothstep(rad - 0.014, rad - 0.009, r) * (1.0 - smoothstep(rad, rad + 0.004, r));
        col = mix(col, f.accent.rgb * 1.3, inside * panel * (1.0 - hot));
        col = mix(col, f.ink.rgb, clamp(outline, 0.0, 1.0) * panel);
    }

    // Vignette toward ink.
    col = mix(col, f.ink.rgb, f.fx4.x * smoothstep(0.45, 1.25, r));
    col = col * f.fx4.z;

    var out = mix(scene.rgb, col, clamp(strength * mask, 0.0, 1.0));
    out = out + f.accent.rgb * ring * 3.0 * strength;
    return vec4<f32>(out, scene.a);
}
