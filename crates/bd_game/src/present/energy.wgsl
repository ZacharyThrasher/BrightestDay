// Palm-beam energy (present::palm_beam): layered shells, linear HDR out (bloom picks up > 1).
//  * mode 0 BODY  — the saturated element colour, brighter toward the silhouette (fresnel), with
//                   streaks flowing down the beam (uv.y along the mesh, `misc.w` = length in m).
//  * mode 1 HALO  — an additive glow shell: brightest where it overlaps the body, fading to its edge.
//  * mode 2 CORE  — the white-hot centre third, tinted by the element.
//  * mode 3 CLASH — an unstable ball: two colours (`color` / `hot`) boiling against each other.
#import bevy_pbr::{forward_io::VertexOutput, mesh_view_bindings::view}

struct Energy {
    // rgb = element colour (saturated), a = opacity
    color: vec4<f32>,
    // rgb = hot colour (core tint / clash second colour), a = intensity (HDR multiplier)
    hot: vec4<f32>,
    // x = time (s), y = mode, z = flicker 0..1, w = length (m) for the streak frequency
    misc: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> m: Energy;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let n = normalize(in.world_normal);
    let v = normalize(view.world_position - in.world_position.xyz);
    let ndv = clamp(abs(dot(n, v)), 0.0, 1.0);
    let fres = 1.0 - ndv;
    let t = m.misc.x;
    let mode = m.misc.y;
    let k = m.hot.a * (1.0 - 0.12 * m.misc.z * sin(t * 71.0));
    // Streaks racing away from the palms along the mesh (uv.y), a few per metre, twisting.
    let along = in.uv.y * max(m.misc.w, 1.0);
    let s1 = sin(along * 2.1 - t * 38.0 + in.uv.x * 18.85);
    let s2 = sin(along * 0.9 - t * 23.0 - in.uv.x * 12.57);
    let streak = clamp(0.5 + 0.5 * s1 * s2, 0.0, 1.0);
    if (mode < 0.5) {
        // Body: saturated colour, hotter edge, flowing streaks.
        // A dense body with a bright rim reads as energy, not a translucent tube.
        let col = m.color.rgb * k * (0.85 + 1.8 * pow(fres, 3.0) + 0.55 * streak) + vec3(1.0) * k * 0.35 * pow(fres, 6.0);
        let a = clamp(m.color.a * (0.7 + 0.3 * pow(fres, 0.7)), 0.0, 1.0);
        return vec4(col, a);
    }
    if (mode < 1.5) {
        // Halo: soft additive glow, strongest through the middle of the shell.
        let g = pow(ndv, 2.2) * (0.8 + 0.2 * streak);
        return vec4(m.color.rgb * k * g, clamp(g * m.color.a, 0.0, 1.0));
    }
    if (mode < 2.5) {
        // Core: white-hot, faintly tinted, a little streaked.
        let col = mix(m.hot.rgb, vec3(1.0), 0.7) * k * (0.9 + 0.2 * streak);
        return vec4(col, m.color.a);
    }
    // Clash ball: two colours boiling, a hot rim.
    let a1 = sin(dot(n, vec3(0.8, 0.5, 0.3)) * 9.0 + t * 11.0 + atan2(n.z, n.x) * 3.0);
    let a2 = sin(dot(n, vec3(-0.3, 0.9, -0.4)) * 7.0 - t * 13.0 + atan2(n.y, n.x) * 2.0);
    let mixk = clamp(0.5 + 0.5 * a1 * a2, 0.0, 1.0);
    let col = mix(m.color.rgb, m.hot.rgb, mixk) * k * (1.0 + 2.5 * pow(fres, 2.0)) + vec3(1.0) * k * 0.4 * pow(ndv, 4.0);
    return vec4(col, m.color.a);
}
