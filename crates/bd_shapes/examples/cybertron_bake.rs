//! Bake the Cybertron map's statues and relics from the Transformers icon library into terrain voxels.
//!
//! ```text
//! cargo run -p bd_shapes --example cybertron_bake --release
//! ```
//! Robot-mode statues use each icon's `alt_shape` (robot mode; `shape` is the vehicle), scaled to a
//! target height and voxelised at 0.5 m (terrain voxel size) in rest pose. The fallen colossus is
//! Megatron's robot-mode *head part only*, scaled to a giant. Relics (Matrix of Leadership, AllSpark)
//! use the single-mode icon shapes.
//!
//! Output: `crates/bd_voxel/src/maps/cybertron_icons.bin` (read by `maps/cybertron.rs` through
//! `include_bytes!`) and a contact sheet `target/cybertron/icons.png`.
//!
//! Format (little endian): `b"CYBI"`, u16 count, then per icon: u8 name length, name bytes,
//! u8 size x, y, z, u32 voxel count, voxels as `(x, y, z, tint)` u8 quads — minimum corner 0, +Y up,
//! the icon's front towards +Z; tint 0 Primary, 1 Accent, 2 Dark, 3 Core.

use std::path::PathBuf;

use bd_core::shape::{PartRole, Tint};
use bd_shapes::{spec_bounds, voxelize_at};
use glam::IVec3;

#[derive(Clone, Copy)]
enum Mode {
    /// Robot mode (`alt_shape`) of a two-mode icon.
    Robot,
    /// Only the robot-mode head part, scaled so the head is this tall.
    Head,
    /// Single-mode icon (`shape`).
    Plain,
}

/// (name in the map, phrase, mode, target height m).
const ICONS: &[(&str, &str, Mode, f32)] = &[
    ("optimus", "optimus prime", Mode::Robot, 24.0),
    ("rodimus", "rodimus prime", Mode::Robot, 19.0),
    ("primal", "optimus primal", Mode::Robot, 19.0),
    ("ironhide", "ironhide transformer", Mode::Robot, 19.0),
    ("ratchet", "ratchet transformer", Mode::Robot, 19.0),
    ("prowl", "prowl transformer", Mode::Robot, 19.0),
    ("megatron", "megatron", Mode::Robot, 22.0),
    ("colossus_head", "megatron", Mode::Head, 13.0),
    ("matrix", "matrix of leadership", Mode::Plain, 4.5),
    ("allspark", "allspark", Mode::Plain, 7.0),
];

fn flatten(model: &bd_shapes::VoxelModel, head_only: bool) -> Vec<(IVec3, u8)> {
    let mut vox: Vec<(IVec3, u8)> = Vec::new();
    for (i, part) in model.parts.iter().enumerate() {
        if head_only && part.role != Some(PartRole::Head) {
            continue;
        }
        let inst = model.instances.get(i).copied();
        let (src, off) = match inst {
            Some(p) if p.source != i => (&model.parts[p.source], (p.offset / model.voxel_size).round().as_ivec3()),
            _ => (part, IVec3::ZERO),
        };
        for &(v, t) in &src.voxels {
            let t = match t {
                Tint::Primary => 0,
                Tint::Accent => 1,
                Tint::Dark => 2,
                Tint::Core => 3,
            };
            let p = v + off;
            vox.push((IVec3::new(p.x, p.y, -p.z), t));
        }
    }
    vox.sort_by_key(|(p, _)| (p.y, p.z, p.x));
    vox.dedup_by_key(|(p, _)| *p);
    if vox.is_empty() {
        return vox;
    }
    let lo = vox.iter().fold(IVec3::splat(i32::MAX), |a, (p, _)| a.min(*p));
    for (p, _) in &mut vox {
        *p -= lo;
    }
    vox
}

/// Terrain voxels are pruned unless connected to the ground: join every loose piece (a weapon, a
/// shoulder cannon) to the main body with a short strut of Dark voxels. Returns how many pieces.
fn connect_components(vox: &mut Vec<(IVec3, u8)>) -> usize {
    use std::collections::{HashMap, VecDeque};
    let mut bridged = 0;
    loop {
        let set: HashMap<IVec3, usize> = vox.iter().enumerate().map(|(i, (p, _))| (*p, i)).collect();
        let mut comp = vec![usize::MAX; vox.len()];
        let mut sizes = Vec::new();
        for s in 0..vox.len() {
            if comp[s] != usize::MAX {
                continue;
            }
            let id = sizes.len();
            let mut n = 0;
            let mut q = VecDeque::from([s]);
            comp[s] = id;
            while let Some(i) = q.pop_front() {
                n += 1;
                for d in [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z] {
                    if let Some(&j) = set.get(&(vox[i].0 + d))
                        && comp[j] == usize::MAX
                    {
                        comp[j] = id;
                        q.push_back(j);
                    }
                }
            }
            sizes.push(n);
        }
        if sizes.len() <= 1 {
            return bridged;
        }
        // The body = the component touching the ground (y = 0) with the most voxels.
        let main = (0..sizes.len()).max_by_key(|&c| (vox.iter().zip(&comp).any(|((p, _), &k)| k == c && p.y == 0), sizes[c])).unwrap();
        let other = (0..sizes.len()).find(|&c| c != main).unwrap();
        // Nearest pair between `other` and `main`, then a 6-connected strut.
        let (mut best, mut a, mut b) = (i32::MAX, IVec3::ZERO, IVec3::ZERO);
        for (i, (p, _)) in vox.iter().enumerate() {
            if comp[i] != other {
                continue;
            }
            for (j, (q, _)) in vox.iter().enumerate() {
                if comp[j] == main {
                    let d = (*p - *q).abs().element_sum();
                    if d < best {
                        (best, a, b) = (d, *p, *q);
                    }
                }
            }
        }
        let mut v = a;
        while v != b {
            let d = b - v;
            if d.x != 0 {
                v.x += d.x.signum();
            } else if d.y != 0 {
                v.y += d.y.signum();
            } else {
                v.z += d.z.signum();
            }
            if !set.contains_key(&v) {
                vox.push((v, 2));
            }
        }
        bridged += 1;
    }
}

fn size_of(vox: &[(IVec3, u8)]) -> IVec3 {
    vox.iter().fold(IVec3::ZERO, |a, (p, _)| a.max(*p)) + IVec3::ONE
}

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lex = bd_lexicon::Lexicon::icons_only(&bd_lexicon::Lexicon::icon_files(&root.join("assets")));
    let mut bin: Vec<u8> = Vec::new();
    bin.extend_from_slice(b"CYBI");
    bin.extend_from_slice(&(ICONS.len() as u16).to_le_bytes());
    let mut sheet: Vec<(String, Vec<(IVec3, u8)>)> = Vec::new();
    for &(name, phrase, mode, height) in ICONS {
        let r = lex.resolve(phrase);
        let spec0 = match mode {
            Mode::Robot | Mode::Head => r.alt_shape.clone().unwrap_or_else(|| panic!("{phrase}: no robot mode")),
            Mode::Plain => r.shape.clone().unwrap_or_else(|| panic!("{phrase}: not a curated icon")),
        };
        let b = spec_bounds(&spec0);
        let h_units = (b.max.y - b.min.y).max(0.1);
        let mut factor = height / 0.25 / h_units;
        let head_only = matches!(mode, Mode::Head);
        if head_only {
            // Pass 1 at a known scale to measure the head, then rescale so the head is `height` tall.
            let mut s = spec0.clone();
            bd_shapes::scale_spec(&mut s, factor);
            let v = flatten(&voxelize_at(&s, 2), true);
            let hh = size_of(&v).y.max(1) as f32 * 0.5;
            factor *= height / hh;
        }
        let mut spec = spec0.clone();
        bd_shapes::scale_spec(&mut spec, factor);
        let mut vox = flatten(&voxelize_at(&spec, 2), head_only);
        let bridged = connect_components(&mut vox);
        let size = size_of(&vox);
        assert!(size.max_element() < 256, "{name}: too big ({size})");
        println!("{name:>14}: {} ({}), {} voxels, {}x{}x{} (0.5 m), {bridged} loose pieces bridged", r.recipe.name, phrase, vox.len(), size.x, size.y, size.z);
        bin.push(name.len() as u8);
        bin.extend_from_slice(name.as_bytes());
        bin.extend_from_slice(&[size.x as u8, size.y as u8, size.z as u8]);
        bin.extend_from_slice(&(vox.len() as u32).to_le_bytes());
        for (p, t) in &vox {
            bin.extend_from_slice(&[p.x as u8, p.y as u8, p.z as u8, *t]);
        }
        sheet.push((name.to_string(), vox));
    }
    let path = root.join("crates/bd_voxel/src/maps/cybertron_icons.bin");
    std::fs::write(&path, &bin).expect("write icons");
    println!("wrote {} ({} bytes)", path.display(), bin.len());
    contact_sheet(&root.join("target/cybertron/icons.png"), &sheet);
}

/// Iso contact sheet: one tile per icon, tints Primary blue / Accent red / Dark grey / Core white.
fn contact_sheet(path: &std::path::Path, props: &[(String, Vec<(IVec3, u8)>)]) {
    let (tw, th) = (260u32, 320u32);
    let mut img = image::RgbImage::from_pixel(tw * props.len() as u32, th, image::Rgb([20, 22, 28]));
    let pal = [[90.0, 130.0, 220.0], [220.0, 60.0, 60.0], [90.0, 90.0, 100.0], [250.0, 250.0, 250.0]];
    for (k, (_, vox)) in props.iter().enumerate() {
        let hi = vox.iter().fold(IVec3::ZERO, |a, (p, _)| a.max(*p));
        let s = (240.0 / (hi.x + hi.z + 2).max(hi.y + 2) as f32).clamp(1.0, 14.0);
        let mut sorted = vox.clone();
        sorted.sort_by_key(|(p, _)| (p.x + p.z, p.y));
        for (p, t) in sorted {
            let cx = tw as f32 * (k as f32 + 0.5) + (p.x - p.z) as f32 * s * 0.87;
            let cy = th as f32 - 20.0 - (p.x + p.z) as f32 * s * 0.5 - p.y as f32 * s;
            let c = pal[t as usize];
            for (dy, shade) in [(0.0, 1.0), (s * 0.5, 0.75)] {
                for yy in 0..(s as i32).max(1) {
                    for xx in -(s as i32).max(1)..(s as i32).max(1) {
                        let (x, y) = (cx as i32 + xx, (cy + dy) as i32 + yy - s as i32);
                        if x >= 0 && y >= 0 && (x as u32) < img.width() && (y as u32) < img.height() {
                            let f = if xx < 0 { shade } else { shade * 0.85 };
                            img.put_pixel(x as u32, y as u32, image::Rgb([(c[0] * f) as u8, (c[1] * f) as u8, (c[2] * f) as u8]));
                        }
                    }
                }
            }
        }
    }
    std::fs::create_dir_all(path.parent().unwrap()).ok();
    img.save(path).expect("save sheet");
    println!("contact sheet {}", path.display());
}
