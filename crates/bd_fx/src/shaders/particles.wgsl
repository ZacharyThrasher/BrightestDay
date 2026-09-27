// Analytic GPU particles, drawn as cubes.
//
// The mesh is a batch of BATCH unit cubes (uv = face corner, uv_b.x = cube index in the batch);
// the entity's MeshTag is the batch's first particle index. Motion is a closed-form function of age,
// evaluated here, so the CPU only writes a particle once (at spawn).

#import bevy_pbr::{
    mesh_functions,
    view_transformations::position_world_to_clip,
    mesh_view_bindings::{view, globals, lights},
}
#import bd_fx::common::{hash1, hash3v, quat_rotate, axis_angle, fog_factor, fog_color, age_since}
#ifdef TONEMAP_IN_SHADER
#import bevy_core_pipeline::tonemapping::tone_mapping
#endif

struct Particle {
    p0: vec4<f32>,
    v0: vec4<f32>,
    color: vec4<f32>,
    a: vec4<f32>,
    b: vec4<u32>,
}

struct Anchor {
    x: vec4<f32>,
    y: vec4<f32>,
    z: vec4<f32>,
    t: vec4<f32>,
    ring: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<storage, read> particles: array<Particle>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<storage, read> anchors: array<Anchor>;

const K_BALLISTIC: u32 = 0u;
const K_STREAM: u32 = 1u;
const K_ORBIT: u32 = 2u;
const K_RISE: u32 = 3u;
const K_FLASH: u32 = 4u;
const K_STATIC: u32 = 5u;
const K_BEAM: u32 = 6u;

const F_SMOKE: u32 = 1u;
const F_STRETCH: u32 = 2u;
const F_FLOOR: u32 = 4u;
const F_REVERSE: u32 = 8u;
const F_WHITE_BIRTH: u32 = 16u;
const F_PLAIN: u32 = 32u;
const F_QUAT: u32 = 64u;
const F_GROW: u32 = 128u;
const F_POP: u32 = 256u;
const F_BOUNCE: u32 = 512u;

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
    @location(3) color: vec4<f32>,
    @location(4) @interpolate(flat) flags: u32,
}

fn anchor_point(i: u32, p: vec3<f32>) -> vec3<f32> {
    let a = anchors[i];
    return a.t.xyz + a.x.xyz * p.x + a.y.xyz * p.y + a.z.xyz * p.z;
}

fn basis_from_dir(d: vec3<f32>) -> mat3x3<f32> {
    let z = normalize(d);
    var up = vec3<f32>(0.0, 1.0, 0.0);
    if abs(z.y) > 0.95 {
        up = vec3<f32>(1.0, 0.0, 0.0);
    }
    let x = normalize(cross(up, z));
    let y = cross(z, x);
    return mat3x3<f32>(x, y, z);
}

fn dead() -> VOut {
    var out: VOut;
    out.clip = vec4<f32>(0.0, 0.0, 2.0, 1.0);
    out.world_pos = vec3<f32>(0.0);
    out.world_normal = vec3<f32>(0.0, 1.0, 0.0);
    out.uv = vec2<f32>(0.0);
    out.color = vec4<f32>(0.0);
    out.flags = 0u;
    return out;
}

@vertex
fn vertex(v: Vertex) -> VOut {
    let idx = mesh_functions::get_tag(v.instance_index) + u32(v.uv_b.x + 0.5);
    let p = particles[idx];
    let size0 = p.p0.w;
    if size0 <= 0.0 {
        return dead();
    }
    let kind = p.b.x & 255u;
    let flags = p.b.x >> 8u;
    let seed = p.b.z;
    let aux = bitcast<f32>(p.b.w);

    var pos = vec3<f32>(0.0);
    var vel = vec3<f32>(0.0);
    var size = size0;
    var scale = vec3<f32>(1.0);
    var rot = mat3x3<f32>(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(0.0, 0.0, 1.0));
    var col = p.color.rgb * p.color.a;
    var have_rot = false;
    var beam_len = 0.0;

    if kind == K_STATIC {
        pos = p.p0.xyz;
        if (flags & F_QUAT) != 0u {
            let q = p.v0;
            rot = mat3x3<f32>(quat_rotate(q, vec3<f32>(1.0, 0.0, 0.0)), quat_rotate(q, vec3<f32>(0.0, 1.0, 0.0)), quat_rotate(q, vec3<f32>(0.0, 0.0, 1.0)));
            scale = max(p.a.xyz, vec3<f32>(0.0));
        } else {
            let seg = p.v0.xyz;
            let len = length(seg);
            if len < 1e-5 {
                return dead();
            }
            rot = basis_from_dir(seg);
            scale = vec3<f32>(1.0, 1.0, len / size0);
        }
        have_rot = true;
    } else {
        let age = age_since(globals.time, p.v0.w);
        let life = p.a.x;
        if age < 0.0 || age >= life {
            return dead();
        }
        let t = age / life;
        let grav = p.a.y;
        let drag = p.a.z;
        let anchor = p.b.y;
        let hv = hash3v(seed);

        if kind == K_BALLISTIC {
            var f = age;
            if drag > 0.0 {
                f = (1.0 - exp(-drag * age)) / drag;
            }
            let g = vec3<f32>(0.0, -9.81 * grav, 0.0);
            pos = p.p0.xyz + p.v0.xyz * f + 0.5 * g * age * age;
            vel = p.v0.xyz * exp(-drag * age) + g * age;
            if anchor != 0u {
                pos = anchor_point(anchor, pos);
            }
            if (flags & F_BOUNCE) != 0u {
                // Closed-form bounces on the floor (gravity only), then rest.
                let gg = max(9.81 * grav, 0.1);
                let fh = aux + size0 * 0.5;
                var y0 = p.p0.y;
                var vy = p.v0.y;
                var hxz = p.v0.xz;
                var bxz = p.p0.xz;
                var tt = age;
                var resting = true;
                for (var k = 0; k < 4; k = k + 1) {
                    let disc = vy * vy + 2.0 * gg * max(y0 - fh, 0.0);
                    let th = (vy + sqrt(disc)) / gg;
                    if tt < th {
                        pos = vec3<f32>(bxz.x + hxz.x * tt, y0 + vy * tt - 0.5 * gg * tt * tt, bxz.y + hxz.y * tt);
                        vel = vec3<f32>(hxz.x, vy - gg * tt, hxz.y);
                        resting = false;
                        break;
                    }
                    bxz = bxz + hxz * th;
                    tt = tt - th;
                    y0 = fh;
                    vy = (gg * th - vy) * 0.42;
                    hxz = hxz * 0.55;
                }
                if resting {
                    pos = vec3<f32>(bxz.x, fh, bxz.y);
                    vel = vec3<f32>(0.0);
                }
            }
            if (flags & F_FLOOR) != 0u && pos.y < aux + size0 * 0.5 {
                pos.y = aux + size0 * 0.5;
                vel = vec3<f32>(vel.x, 0.0, vel.z) * 0.3;
            }
        } else if kind == K_STREAM {
            // From the owner's ring to a voxel slot (anchor-local target), or back (REVERSE).
            let a = anchors[anchor];
            var start = a.ring.xyz + p.p0.xyz;
            var end = anchor_point(anchor, p.v0.xyz);
            if (flags & F_REVERSE) != 0u {
                let tmp = start;
                start = end;
                end = tmp;
            }
            let d = end - start;
            let dist = length(d);
            let side = (hv - 0.5) * 2.0;
            let ctrl = start + d * 0.45 + vec3<f32>(0.0, 1.0, 0.0) * dist * 0.18 + side * dist * 0.35;
            // ease in-out (quick launch, soft landing)
            let u = t * t * (3.0 - 2.0 * t);
            let q0 = mix(start, ctrl, u);
            let q1 = mix(ctrl, end, u);
            pos = mix(q0, q1, u);
            vel = (q1 - q0);
            size = size0 * (0.45 + 0.55 * sin(3.14159 * min(t * 1.2, 1.0)) + t * 0.4);
            col = mix(vec3<f32>(4.0), col, clamp(t * 1.6, 0.0, 1.0));
        } else if kind == K_ORBIT {
            let centre = anchors[anchor].t.xyz + p.p0.xyz;
            let r0 = p.v0.x;
            let w = p.v0.y;
            let radial = p.v0.z;
            let r = max(r0 + radial * age, 0.05);
            let n = normalize(hv - 0.5 + vec3<f32>(0.0, 0.8, 0.0));
            let b = basis_from_dir(n);
            let ang = hv.x * 6.2832 + w * age * (1.0 + 0.6 * r0 / max(r, 0.05));
            let rel = (b[0] * cos(ang) + b[1] * sin(ang)) * r;
            pos = centre + rel;
            vel = normalize(cross(n, rel)) * sign(w) + normalize(rel) * sign(radial) * 0.4;
            if radial < 0.0 {
                size = size0 * clamp(r / max(r0, 0.01) * 1.3, 0.0, 1.0);
            }
        } else if kind == K_RISE {
            var f = age;
            if drag > 0.0 {
                f = (1.0 - exp(-drag * age)) / drag;
            }
            let wob = p.a.w;
            let ph = hv * 6.2832;
            let wobble = vec3<f32>(sin(age * 3.1 + ph.x), 0.0, cos(age * 2.3 + ph.y)) * wob * sqrt(age);
            pos = p.p0.xyz + p.v0.xyz * f + vec3<f32>(0.0, grav * age, 0.0) + wobble;
            if anchor != 0u {
                pos = anchor_point(anchor, pos);
            }
            vel = p.v0.xyz;
        } else if kind == K_BEAM {
            pos = p.p0.xyz;
            let seg = p.v0.xyz;
            beam_len = length(seg);
            if beam_len < 1e-5 {
                return dead();
            }
            rot = basis_from_dir(seg);
            have_rot = true;
            let u = 1.0 - t;
            size = size0 * (u * u * u * 0.75 + 0.25 * u);
            col = col * (0.35 + 0.65 * u * u);
        } else if kind == K_FLASH {
            pos = p.p0.xyz;
            // Pop out fast, then COLLAPSE while still lit: the cube is opaque, so a flash that only
            // darkened ended as a big black block. Colour dims a little; size carries the fade.
            size = size0 * (0.35 + 0.65 * sqrt(min(t / 0.3, 1.0))) * (1.0 - smoothstep(0.3, 1.0, t));
            col = col * (1.0 - 0.5 * t);
        }

        // Life shaping.
        if (flags & F_GROW) != 0u {
            size *= (0.35 + 1.1 * sqrt(t)) * (1.0 - smoothstep(0.6, 1.0, t));
        } else if (flags & F_POP) != 0u {
            size *= (0.3 + 0.9 * t) * (1.0 - smoothstep(0.9, 1.0, t));
        } else if kind != K_FLASH && kind != K_STREAM && kind != K_BEAM {
            size *= 1.0 - smoothstep(0.5, 1.0, t);
        }
        if kind != K_FLASH && kind != K_BEAM {
            col *= 1.0 - 0.6 * t;
        }
        if kind == K_BEAM {
            scale = vec3<f32>(1.0, 1.0, beam_len / max(size, 1e-4));
        }
        if (flags & F_WHITE_BIRTH) != 0u {
            col = mix(col, vec3<f32>(5.0) + col, exp(-age * 14.0));
        }

        if (flags & F_STRETCH) != 0u && dot(vel, vel) > 1e-4 {
            rot = basis_from_dir(vel);
            scale = vec3<f32>(1.0, 1.0, max(1.0, p.a.w));
            have_rot = true;
        }
        if !have_rot {
            let spin = p.a.w;
            let axis = normalize(hv - 0.5 + vec3<f32>(1e-3));
            let q = axis_angle(axis, hv.y * 6.2832 + spin * age);
            rot = mat3x3<f32>(quat_rotate(q, vec3<f32>(1.0, 0.0, 0.0)), quat_rotate(q, vec3<f32>(0.0, 1.0, 0.0)), quat_rotate(q, vec3<f32>(0.0, 0.0, 1.0)));
        }
    }

    // Readability: chunky shatter cubes shrink away within ~1.5 m of the camera and big clustered
    // ones are capped close to it (a 1 m cube in your face hides the fight). Not streaks, casings,
    // flashes, beams or streams (those live next to your own hand in first person).
    if kind == K_BALLISTIC && (flags & F_STRETCH) == 0u && size0 >= 0.1 {
        let cd = distance(pos, view.world_position.xyz);
        if cd < 3.0 {
            size = min(size, 0.3 + 0.25 * cd + max(cd - 2.0, 0.0) * max(cd - 2.0, 0.0)) * smoothstep(0.35, 1.5, cd);
        }
    }
    if size <= 1e-4 {
        return dead();
    }
    let local = v.position * scale * size;
    let wp = pos + rot * local;
    var out: VOut;
    out.world_pos = wp;
    out.clip = position_world_to_clip(wp);
    out.world_normal = rot * v.normal;
    out.uv = v.uv;
    out.color = vec4<f32>(col, p.color.a);
    out.flags = flags;
    return out;
}

@fragment
fn fragment(in: VOut) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let d = min(min(uv.x, 1.0 - uv.x), min(uv.y, 1.0 - uv.y));
    let px = max(max(fwidth(uv.x), fwidth(uv.y)), 1e-4);
    let w = max(0.1, px);
    let edge = (1.0 - smoothstep(w - px, w + px, d)) * clamp(1.5 - px * 3.0, 0.0, 1.0);
    let N = normalize(in.world_normal);
    var rgb: vec3<f32>;
    if (in.flags & F_SMOKE) != 0u {
        // Sun-lit grey cube (not emissive).
        var lit = lights.ambient_color.rgb * view.exposure;
        if lights.n_directional_lights > 0u {
            let l = lights.directional_lights[0];
            lit += l.color.rgb * view.exposure * max(dot(N, l.direction_to_light), 0.0);
        }
        rgb = in.color.rgb / max(in.color.a, 1e-3) * (0.18 + lit * 0.9) * (1.0 - edge * 0.25);
    } else if (in.flags & F_PLAIN) != 0u {
        rgb = in.color.rgb;
    } else {
        rgb = in.color.rgb * (0.65 + 0.35 * (1.0 - d * 2.0)) + in.color.rgb * edge * 0.9;
    }
    let f = fog_factor(in.world_pos);
    rgb = mix(rgb, fog_color(), f);
    var out = vec4<f32>(rgb, 1.0);
#ifdef TONEMAP_IN_SHADER
    out = tone_mapping(out, view.color_grading);
#endif
    return out;
}
