// Gradient sky dome: zenith → horizon → ground, plus a sun disk and glow.
// Output is linear HDR; the sun core is > 1 so it blooms.
#import bevy_pbr::{forward_io::VertexOutput, mesh_view_bindings::view}

struct Sky {
    zenith: vec4<f32>,
    horizon: vec4<f32>,
    ground: vec4<f32>,
    // xyz = direction toward the sun, w = disk sharpness exponent
    sun_dir: vec4<f32>,
    // rgb = sun colour, w = wide glow strength
    sun_color: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> sky: Sky;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let dir = normalize(in.world_position.xyz - view.world_position);
    let y = dir.y;
    var col: vec3<f32>;
    if (y >= 0.0) {
        let t = pow(clamp(y, 0.0, 1.0), 0.5);
        col = mix(sky.horizon.rgb, sky.zenith.rgb, t);
    } else {
        let t = pow(clamp(-y, 0.0, 1.0), 0.55);
        col = mix(sky.horizon.rgb, sky.ground.rgb, t);
    }
    // Thin bright band just above the horizon: sells the "big open sky" look.
    col += sky.horizon.rgb * 0.18 * exp(-abs(y) * 22.0);
    let s = max(dot(dir, sky.sun_dir.xyz), 0.0);
    let disk = smoothstep(0.9993, 0.9997, s) * 14.0;
    let glow = pow(s, 10.0) * sky.sun_color.w + pow(s, sky.sun_dir.w) * 1.5;
    col += sky.sun_color.rgb * (disk + glow);
    return vec4<f32>(col, 1.0);
}
