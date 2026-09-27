//! Character costume gallery (software-rasterised, in-game costume resolution 1/16 m voxels).
//!
//! cargo run -p bd_shapes --example costume_gallery --release -- [--names gojo,vader] [--variants]
//!     [--before-after] [--derived N] [--out PATH]
//!
//! Default: every character in assets/costumes/characters.ron (first variant), front + 3/4 view with
//! the name under each → docs/costume_gallery.png.
//! `--variants`: every variant of every (or the named) character → docs/costume_variants.png.
//! `--before-after`: vocabulary costume (the old path) vs the character look for 6 characters →
//! docs/costume_before_after.png. `--derived N`: N humanoid icons through the derived tier →
//! docs/costume_derived.png.

use std::path::PathBuf;

use bd_core::mechanics::{Form, Recipe};
use bd_core::shape::ShapeSpec;
use bd_shapes::costume::{self, Costume, look};
use bd_shapes::*;
use glam::Vec3;

// Canvas, camera, rasteriser, font and colouring are shared with the `phrase` tool.
#[path = "raster/mod.rs"]
mod raster;
use raster::{Canvas, Dressed, FRONT, THREE_Q, draw_view, text, text_w};

fn prepare(label: String, sub: String, c: &Costume) -> Dressed {
    let pm = costume::costume_model_painted(c);
    Dressed::new(label, sub, pm.model, Some(pm.colors.as_slice()), 0.0)
}

/// A summoned costume construct (`text` cast with a recipe of `scale` m), coloured like the
/// construct shader does: the costume palette blended toward the lantern (corps) colour.
fn prepare_summon(label: String, text: &str, scale: f32) -> Dressed {
    let mut r = Recipe::unknown(text);
    r.form = Form::Biped;
    r.scale = scale;
    let mut shape = None;
    assert!(costume::summon::apply(text, &mut r, &mut shape), "{text} is not a costume summon");
    let s = shape.expect("shape");
    let mut d = raster::summon_dressed(label, String::new(), &r, &s).expect("colours");
    d.sub = format!("SUMMON {:.0} M  {}V  {:.3} M VOXELS", scale, d.model.voxel_count, d.model.voxel_size);
    d
}

/// Grid of dressed costumes: two views each, name (+ sub line) under the pair.
fn sheet(items: &[Dressed], cols: usize, title: &str, zoom: f32) -> Canvas {
    let (vw, vh) = ((150.0 * zoom) as i32, (230.0 * zoom) as i32);
    let tw = vw * 2 + 8;
    let th = vh + 40;
    let head = 30;
    let rows = items.len().div_ceil(cols);
    let mut c = Canvas::new(cols * tw as usize, rows * th as usize + head);
    text(&mut c, 8, 9, title, [215, 225, 235], 2);
    // Pixels per metre, shared by every tile so sizes compare (a 1.8 m figure ~160 px at zoom 1).
    let scale = 90.0f32 * zoom;
    for (i, d) in items.iter().enumerate() {
        let x0 = (i % cols) as i32 * tw;
        let y0 = (i / cols) as i32 * th + head as i32;
        c.rect(x0 + 2, y0 + 2, tw - 4, th - 4, [26, 29, 38]);
        draw_view(&mut c, d, FRONT, x0 + 4, y0 + 4, vw, vh, scale);
        draw_view(&mut c, d, THREE_Q, x0 + 4 + vw, y0 + 4, vw, vh, scale);
        let lw = text_w(&d.label, 2);
        text(&mut c, x0 + (tw - lw) / 2, y0 + vh + 8, &d.label, [240, 240, 244], 2);
        if !d.sub.is_empty() {
            let sw = text_w(&d.sub, 1);
            text(&mut c, x0 + (tw - sw) / 2, y0 + vh + 26, &d.sub, [130, 140, 155], 1);
        }
    }
    c
}

fn lexicon(root: &std::path::Path) -> Option<bd_lexicon::Lexicon> {
    bd_lexicon::Lexicon::load(root.join("assets/lexicon.bin")).ok()
}

fn resolve(lex: Option<&bd_lexicon::Lexicon>, text: &str) -> (Recipe, Option<ShapeSpec>) {
    match lex {
        Some(l) => {
            let r = l.resolve(text);
            (r.recipe, r.shape)
        }
        None => {
            let mut r = Recipe::unknown(text);
            r.form = Form::Biped;
            (r, None)
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |f: &str| args.iter().any(|a| a == f);
    let val = |f: &str| args.iter().position(|a| a == f).and_then(|i| args.get(i + 1)).cloned();
    let root = std::env::var("BD_ROOT").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."));
    let docs = root.join("docs");
    let t = look::table();
    if let Some(e) = &t.error {
        panic!("characters.ron: {e}");
    }
    let names: Option<Vec<String>> = val("--names").map(|s| s.split(',').map(|x| x.trim().to_string()).collect());
    let pick: Vec<usize> = match &names {
        Some(ns) => ns.iter().filter_map(|n| look::find(n).map(|f| f.index)).collect(),
        None => (0..t.chars.len()).collect(),
    };
    let tweaks = CostumeTweaks::default();
    let cols: usize = val("--cols").and_then(|s| s.parse().ok()).unwrap_or(6);
    let zoom: f32 = val("--zoom").and_then(|s| s.parse().ok()).unwrap_or(1.0);

    if has("--mix") {
        let phrases = [
            "gojo but bald",
            "donatello with a straw hat",
            "goku with naruto's headband",
            "red goku",
            "goku x luffy",
            "gojo but flibbertigibbet",
            "gojo with pink hair",
            "darth vader with a straw hat",
            "batman with a gold cape",
            "naruto x sasuke",
            "master chief but no helmet",
            "straw hat",
        ];
        let items: Vec<Dressed> = phrases
            .iter()
            .map(|ph| {
                let c = costume::dress(ph, &Recipe::unknown(ph), None, None, 0, tweaks);
                prepare(ph.to_uppercase(), c.character.clone().unwrap_or_default().to_uppercase(), &c)
            })
            .collect();
        let c = sheet(&items, cols.min(6), "MIX AND MATCH (BASE CHARACTER + WITH / BUT / COLOUR / MASHUP)", zoom);
        c.save(&val("--out").map(PathBuf::from).unwrap_or(docs.join("costume_mix.png")));
        return;
    }
    if has("--summons") {
        let items = vec![
            prepare_summon("GOKU".into(), "goku", 1.9),
            prepare_summon("NARUTO".into(), "naruto", 1.8),
            prepare_summon("GOJO".into(), "gojo", 1.9),
            prepare_summon("LUFFY".into(), "luffy", 1.8),
            prepare_summon("GIANT GOKU".into(), "giant goku", 16.0),
            prepare_summon("GIANT GOJO".into(), "giant gojo", 16.0),
        ];
        let c = sheet(&items, 3, "SUMMONED COSTUME CONSTRUCTS (HARD LIGHT: PALETTE BLENDED TOWARD THE LANTERN COLOUR)", zoom);
        c.save(&val("--out").map(PathBuf::from).unwrap_or(docs.join("costume_summons.png")));
        return;
    }
    if has("--before-after") {
        let lex = lexicon(&root);
        let list = ["gojo", "darth vader", "goku", "naruto", "spider-man", "master chief"];
        let mut items = Vec::new();
        for n in list {
            let (r, s) = resolve(lex.as_ref(), n);
            let before = Costume::plain(costume_with(&r, s.as_ref(), None, 0, tweaks));
            items.push(prepare(format!("{} (BEFORE)", n.to_uppercase()), format!("OLD: RESOLVED AS {}", r.name.to_uppercase()), &before));
            let after = costume::dress(n, &r, s.as_ref(), None, 0, tweaks);
            items.push(prepare(n.to_uppercase(), "NEW CHARACTER LOOK".into(), &after));
        }
        let c = sheet(&items, 4, "COSTUMES BEFORE / AFTER (OLD VOCABULARY PATH VS CHARACTER LOOK)", zoom);
        c.save(&val("--out").map(PathBuf::from).unwrap_or(docs.join("costume_before_after.png")));
        return;
    }
    if let Some(n) = val("--derived").and_then(|s| s.parse::<usize>().ok()) {
        let lex = lexicon(&root).expect("lexicon needed for --derived");
        let mut items = Vec::new();
        for (name, rec, spec) in humanoid_icons(&root, &lex).into_iter().take(n) {
            let c = costume::derived_costume(&rec, &spec, tweaks);
            items.push(prepare(name.to_uppercase(), "DERIVED FROM ICON".into(), &c));
        }
        let c = sheet(&items, cols, "DERIVED COSTUMES (HUMANOID ICONS WITHOUT A HAND-AUTHORED LOOK)", zoom);
        c.save(&val("--out").map(PathBuf::from).unwrap_or(docs.join("costume_derived.png")));
        return;
    }
    let mut items = Vec::new();
    for &i in &pick {
        let ch = &t.chars[i];
        let nv = if has("--variants") { ch.variant_count() } else { 1 };
        for v in 0..nv {
            let c = costume::character_costume(i, v, tweaks);
            let vol = {
                let b = spec_bounds(&c.spec);
                let s = (b.max - b.min + Vec3::ONE) * COSTUME_UNIT;
                s.x * s.y * s.z / (0.8 * 1.8 * 0.8)
            };
            let vname = ch.variants.get(v).map(|x| x.name.clone()).unwrap_or_default();
            let label = ch.names[0].to_uppercase();
            let pm = costume::costume_model_painted(&c);
            let sub = if has("--variants") || ch.variant_count() > 1 {
                format!("{} {}/{}  {}V {:.2}X", vname.to_uppercase(), v + 1, ch.variant_count(), pm.model.voxel_count, vol)
            } else {
                format!("{}V {:.2}X", pm.model.voxel_count, vol)
            };
            items.push(prepare(label, sub, &c));
        }
    }
    let default_out = if has("--variants") { "costume_variants.png" } else { "costume_gallery.png" };
    let title = format!("CHARACTER COSTUMES ({} LOOKS) - FRONT + 3/4 - 1/16 M VOXELS", items.len());
    let c = sheet(&items, cols, &title, zoom);
    c.save(&val("--out").map(PathBuf::from).unwrap_or(docs.join(default_out)));
}

/// Humanoid icons (Biped with head + arms) that have no hand-authored character look.
fn humanoid_icons(root: &std::path::Path, lex: &bd_lexicon::Lexicon) -> Vec<(String, Recipe, ShapeSpec)> {
    let mut files = vec![root.join("assets/icons.ron")];
    if let Ok(rd) = std::fs::read_dir(root.join("assets/icons")) {
        let mut v: Vec<PathBuf> =
            rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "ron")).collect();
        v.sort();
        files.extend(v);
    }
    let mut out = Vec::new();
    for f in files {
        let Ok(src) = std::fs::read_to_string(&f) else { continue };
        for line in src.lines() {
            let l = line.trim();
            let Some(rest) = l.strip_prefix("names: [\"") else { continue };
            let Some(name) = rest.split('"').next() else { continue };
            if costume::grammar::parse(name).is_some() {
                continue;
            }
            let r = lex.resolve(name);
            if let Some(s) = r.shape
                && costume::is_character_icon(&r.recipe, &s)
                && !out.iter().any(|(n, _, _): &(String, Recipe, ShapeSpec)| n == name)
            {
                out.push((name.to_string(), r.recipe, s));
            }
        }
    }
    out
}
