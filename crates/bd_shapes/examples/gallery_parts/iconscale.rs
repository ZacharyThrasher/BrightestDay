//! `--iconscale`: the same icons from different packs, authored-units voxelisation (old) vs
//! normalised-to-real-size (0.25 m) vs a finer 0.125 m variant, all drawn at true world size.

use super::*;

pub fn run(docs: &std::path::Path, lex: Option<&bd_lexicon::Lexicon>) {
    let Some(lex) = lex else { return };
    let names = ["superman", "batman", "captain america", "thor", "leonardo", "michelangelo", "optimus prime", "iron man", "spider-man", "pikachu"];
    let mut items: Vec<(String, Recipe, ShapeSpec)> = Vec::new();
    for n in names {
        let r = lex.resolve(n);
        match r.shape {
            Some(s) => items.push((n.to_uppercase(), r.recipe, s)),
            None => println!("{n}: no curated shape ({:?})", r.recipe.form),
        }
    }
    let tile = 190usize;
    let rows = 3;
    let head = 22;
    let row_h = tile + 14;
    let mut c = Canvas::new(items.len() * tile, rows * row_h + head);
    text(&mut c, 6, 7, "ICON SCALE: ROW 1 AUTHORED UNITS (OLD)  ROW 2 NORMALISED @ 0.25 M  ROW 3 SHIPPING: BUILD_ICON_MODEL (0.125 M <= 3 M)  - TRUE WORLD SIZE", [200, 220, 200]);
    let cam = Cam::from_eye(Vec3::new(0.55, 0.25, -1.0));
    for row in 0..rows {
        // Build all models of the row with their world transform.
        let mut built: Vec<(VoxelModel, Vec<PartMesh>, Affine3A, String)> = Vec::new();
        for (label, r, s) in &items {
            let (m, root) = match row {
                0 => {
                    let m = voxelize_capped(s, voxel_cap(r.form));
                    let k = icon_scale_factor(r, s);
                    (m, Affine3A::from_scale(Vec3::splat(k)))
                }
                1 => (voxelize_capped(&normalize_icon_spec(r, s), voxel_cap(r.form)), Affine3A::IDENTITY),
                _ => (build_icon_model(r, s).1, Affine3A::IDENTITY),
            };
            let meshes = mesh_model(&m);
            let vs = m.voxel_size * root.matrix3.x_axis.x;
            let info = format!("{}V {:.2}M", m.voxel_count, vs);
            built.push((m, meshes, root, format!("{label}|{info}")));
        }
        let mut pts = Vec::new();
        for (m, meshes, root, _) in &built {
            pts.extend(world_points(&Item { model: m, meshes, pose: vec![Affine3A::IDENTITY; m.parts.len()], root: *root, pal: GREEN }));
        }
        // One scale for the whole sheet row: fit the tallest item.
        let mut fit_one = fit_points(&cam, &pts, 0.0, 0.0, tile as f32, tile as f32 - 20.0, 10.0);
        for (k, (m, meshes, root, label)) in built.iter().enumerate() {
            let x0 = (k * tile) as i32;
            let y0 = (row * row_h + head) as i32;
            c.rect(x0 + 1, y0 + 1, tile as i32 - 2, row_h as i32 - 2, [22, 25, 32]);
            c.frame(x0, y0, tile as i32, row_h as i32, [40, 44, 54]);
            let it = Item { model: m, meshes, pose: vec![Affine3A::IDENTITY; m.parts.len()], root: *root, pal: GREEN };
            let own = world_points(&it);
            let f0 = fit_points(&cam, &own, 0.0, 0.0, tile as f32, tile as f32 - 20.0, 10.0);
            fit_one.ox = f0.ox + (f0.scale - fit_one.scale) * 0.0;
            // Same scale for all, centred per tile, feet on a common ground line.
            let (_, gy, _) = cam.proj(Vec3::ZERO);
            let fit = Fit { scale: fit_one.scale, ox: x0 as f32 + tile as f32 * 0.5 - (f0.ox - tile as f32 * 0.5 - 0.0) * 0.0 - 0.0, oy: y0 as f32 + tile as f32 - 8.0 + gy * fit_one.scale };
            let cx = {
                let mut mn = f32::MAX;
                let mut mx = f32::MIN;
                for p in &own {
                    let (x, _, _) = cam.proj(*p);
                    mn = mn.min(x);
                    mx = mx.max(x);
                }
                (mn + mx) * 0.5
            };
            let fit = Fit { ox: fit.ox - cx * fit.scale, ..fit };
            c.clear_depth();
            draw(&mut c, &cam, fit, &it, (x0 + 1, y0 + 1, x0 + tile as i32 - 1, y0 + row_h as i32 - 1));
            let (name, info) = label.split_once('|').unwrap_or((label, ""));
            text(&mut c, x0 + 4, y0 + 4, name, [220, 235, 220]);
            text(&mut c, x0 + 4, y0 + row_h as i32 - 11, info, [110, 130, 120]);
        }
    }
    c.save(&docs.join("gallery_icon_scale.png"));
}
