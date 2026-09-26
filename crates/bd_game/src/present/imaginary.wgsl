// "Imaginary mass" (present::signature_fx): an erasing orb (Hollow Purple) drawn as a sphere, not a
// voxel model. Linear HDR out (bloom picks up everything above 1).
//  * core (misc.z == 0): a dark violet heart with slow swirling bands, a bright magenta / white
//    fresnel rim; `misc.y` = spike (a brief emissive surge at the release / merge).
//  * halo (misc.z == 1): a faint shimmering shell around it (fresnel only, wobbling alpha) — the
//    cheap stand-in for a heat-distortion halo.
#import bevy_pbr::{forward_io::VertexOutput, mesh_view_bindings::view}

struct Imaginary {
    // rgb = core colour, a = core opacity
    core: vec4<f32>,
    // rgb = rim colour, w = fresnel power
    rim: vec4<f32>,
    // x = time (s), y = spike 0..1, z = mode (0 core, 1 halo), w = swirl speed
    misc: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> m: Imaginary;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let n = normalize(in.world_normal);
    let v = normalize(view.world_position - in.world_position.xyz);
    let ndv = clamp(dot(n, v), 0.0, 1.0);
    let fres = pow(1.0 - ndv, max(m.rim.w, 0.5));
    let t = m.misc.x * m.misc.w;
    let spike = clamp(m.misc.y, 0.0, 1.0);
    // Two interfering spirals around the sphere: slow, heavy, never still.
    let a1 = sin(dot(n, vec3(0.8, 0.5, 0.3)) * 7.0 + t * 1.7 + atan2(n.z, n.x) * 3.0);
    let a2 = sin(dot(n, vec3(-0.3, 0.9, -0.4)) * 5.0 - t * 1.3 + atan2(n.y, n.x) * 2.0);
    let swirl = clamp(0.5 + 0.5 * a1 * a2, 0.0, 1.0);
    if (m.misc.z > 0.5) {
        // Halo: a thin bright shell at the silhouette with a shimmering, breathing edge.
        let shimmer = 0.75 + 0.25 * sin(t * 9.0 + (n.x + n.y * 1.7) * 14.0);
        let edge = pow(fres, 3.0) * (1.0 - fres) * 6.0;
        let a = clamp(edge * shimmer * m.core.a, 0.0, 1.0);
        return vec4(m.rim.rgb * (2.5 + 3.0 * spike), a);
    }
    // Core: near-black violet in the middle (it swallows light), bands of deep purple, a hot rim.
    let heart = m.core.rgb * (0.12 + 0.55 * swirl * (1.0 - ndv * 0.6));
    let rim = m.rim.rgb * (2.2 + 4.0 * spike) * fres;
    let bands = m.rim.rgb * swirl * swirl * 0.35 * (1.0 - fres);
    // The spike is a violet surge in the heart (bloom does the rest), never a white card.
    let col = heart * (1.0 + 5.0 * spike) + bands + rim;
    return vec4(col, m.core.a);
}
