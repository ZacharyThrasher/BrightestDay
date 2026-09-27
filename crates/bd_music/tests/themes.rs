//! Franchise themes (`assets/music/themes`): every theme is complete, calibrated and clean, the
//! franchise table maps real powers, and the theme layer swells in on the map's beat, keeps the map
//! track in place underneath, switches themes on a bar line, hosts KO stingers on its own grid and
//! gives way to the big stingers. (Seamless loops of every theme are covered by `player.rs`, which
//! walks every track in the library.)

use std::sync::Arc;

use bd_music::analysis::{integrated_lufs, true_peak_db};
use bd_music::player::{THEME_DUCK_SECS, THEME_IN_SECS, THEME_OUT_SECS};
use bd_music::{MusicCommand, MusicLibrary, MusicPlayer, StingerKind, TrackId};

const SR: u32 = 48_000;

fn lib() -> Arc<MusicLibrary> {
    Arc::new(MusicLibrary::load_dir(&bd_music::default_music_dir()).expect("assets/music loads"))
}

fn render(p: &mut MusicPlayer, secs: f32, block: usize) -> Vec<f32> {
    let frames = (secs * SR as f32) as usize;
    let mut out = vec![0.0f32; frames * 2];
    for chunk in out.chunks_mut(block * 2) {
        p.render(chunk);
    }
    out
}

fn map_player(lib: &Arc<MusicLibrary>) -> MusicPlayer {
    let mut p = MusicPlayer::new(Arc::clone(lib), SR);
    p.command(MusicCommand::SetIntensity(0.6));
    p.command(MusicCommand::Play { track: lib.track_id("archipelago_combat").expect("map track"), skip_intro: true });
    p
}

fn theme(lib: &MusicLibrary, name: &str) -> TrackId {
    let id = lib.track_id(name).unwrap_or_else(|| panic!("no theme {name}"));
    assert!(lib.is_theme(id), "{name} is a theme");
    id
}

const THEMES: [&str; 12] = [
    "sewer_surf_stomp",
    "open_sea_jig",
    "chrome_titans",
    "sky_burst_overdrive",
    "shadow_step_shinobi",
    "menace_disco_strut",
    "skyline_vanguard",
    "distant_stars_march",
    "tall_grass_trek",
    "wildwood_valor",
    "four_winds_accord",
    "old_gods_thunder",
];

#[test]
fn every_theme_is_complete_original_and_mapped() {
    let lib = lib();
    assert_eq!(lib.themes().count(), THEMES.len(), "theme list");
    for name in THEMES {
        let id = theme(&lib, name);
        let s = lib.track(id).expect("song");
        let (intro, body) = s.structure_ticks();
        let secs = |t: u64| t as f64 / f64::from(bd_music::notation::PPQ) * 60.0 / f64::from(s.bpm);
        let sting_bars = intro / u64::from(s.bar_ticks);
        assert!((1..=4).contains(&sting_bars), "{name}: the sting is {sting_bars} bars");
        assert!(secs(intro) < 6.0, "{name}: the sting takes {:.1} s", secs(intro));
        assert!(secs(body) > 55.0, "{name}: loop body only {:.1} s", secs(body));
        assert!(s.ending.is_some(), "{name}: no ending");
        assert!(s.info.composer.contains("original") && s.info.source.starts_with("Original composition"), "{name}: provenance");
        assert!(lib.variants_of(id).is_empty() && !s.is_remix(), "{name}: themes have no remix variants");
        // Mapped by at least one franchise.
        assert!(lib.franchises.iter().any(|f| f.theme == id), "{name}: no franchise uses it");
    }
    for f in &lib.franchises {
        assert!(!f.files.is_empty() || !f.ids.is_empty(), "{}: maps nothing", f.name);
    }
}

#[test]
fn franchise_table_resolves_real_powers() {
    let lib = lib();
    let r = |id: &str, file: &str| bd_music::franchise::resolve(&lib.franchises, id, Some(file), None).map(|f| f.name.as_str());
    assert_eq!(r("gomu_gomu", "base_one_piece.ron"), Some("pirate"), "per-file");
    assert_eq!(r("kit_luffy", "anime_classics.ron"), Some("pirate"));
    assert_eq!(r("mutagen_turtle", "base_cartoons.ron"), Some("turtles"), "glob");
    assert_eq!(r("kit_michelangelo", "tv_cartoons.ron"), Some("turtles"));
    assert_eq!(r("kit_optimus_prime", "tv_cartoons.ron"), Some("robots"));
    assert_eq!(r("super_saiyan", "base_techniques.ron"), Some("saiyan"));
    assert_eq!(r("kit_naruto", "anime_classics.ron"), Some("shinobi"));
    assert_eq!(r("star_platinum", "base_jojo.ron"), Some("bizarre"));
    assert_eq!(r("kryptonian", "base_dc.ron"), Some("heroes"));
    assert_eq!(r("kit_iron_man", "movies_comics.ron"), Some("heroes"));
    assert_eq!(r("kit_jedi", "movies_comics.ron"), Some("space"));
    assert_eq!(r("ssb_pikachu", "games_arena.ron"), Some("critters"));
    assert_eq!(r("adv_link", "games_adventure.ron"), Some("overworld"));
    assert_eq!(r("waterbending", "tv_cartoons.ron"), Some("elements"));
    assert_eq!(r("myth_odin", "myth_folklore.ron"), Some("myth"));
    assert_eq!(r("great_sage", "base_folklore.ron"), Some("myth"));
    assert_eq!(r("kit_ichigo", "anime_classics.ron"), None, "unmapped franchises stay quiet");
    assert_eq!(r("adv_mario", "games_adventure.ron"), None);
    // A power's own `music` key wins, "none" opts out.
    let k = |key: &str| {
        bd_music::franchise::resolve(&lib.franchises, "kit_ichigo", Some("anime_classics.ron"), Some(key)).map(|f| f.name.as_str())
    };
    assert_eq!(k("shinobi"), Some("shinobi"));
    assert_eq!(k("sewer_surf_stomp"), Some("turtles"));
    assert_eq!(bd_music::franchise::resolve(&lib.franchises, "gomu_gomu", Some("base_one_piece.ron"), Some("none")), None);
}

#[test]
fn themes_are_calibrated_and_never_clip() {
    let lib = lib();
    for name in THEMES {
        let mut p = MusicPlayer::new(Arc::clone(&lib), SR);
        p.command(MusicCommand::SetIntensity(1.0));
        p.command(MusicCommand::Play { track: theme(&lib, name), skip_intro: false });
        let a = render(&mut p, 30.0, 512);
        let lufs = integrated_lufs(&a, SR);
        let tp = true_peak_db(&a);
        println!("{name:22} {lufs:6.1} LUFS {tp:6.1} dBTP");
        assert!(a.iter().all(|x| x.is_finite()), "{name}: non-finite");
        assert!((-24.5..=-21.5).contains(&lufs), "{name}: {lufs:.1} LUFS (calibrate with render -- --calibrate --themes)");
        assert!(tp < -4.0, "{name}: true peak {tp:.1} dBTP");
    }
}

#[test]
fn a_theme_swells_in_on_a_beat_and_the_map_track_resumes_in_place() {
    let lib = lib();
    let t = theme(&lib, "sewer_surf_stomp");
    let mut plain = map_player(&lib);
    let mut with = map_player(&lib);
    // 5.1 s: deliberately between beats.
    assert_eq!(render(&mut plain, 5.1, 256), render(&mut with, 5.1, 256), "identical before the cue");
    let asked = with.status().sample;
    with.command(MusicCommand::Theme { track: Some(t) });
    let mut entered = None;
    let mut on_beat = false;
    // 1 s in 256-frame steps (48 000 = 187.5 × 256: step 375 × 128 frames instead, exactly 1 s).
    for _ in 0..375 {
        let mut buf = [0.0f32; 256];
        with.render(&mut buf);
        if entered.is_none() && with.theme_debug_log().bar_n > 0 {
            let e = with.theme_debug_log().bars[0];
            let log = with.debug_log();
            on_beat = log.beats.iter().take(log.beat_n.min(64)).any(|&b| b == e);
            entered = Some(e);
        }
    }
    let e = entered.expect("the theme started");
    assert!(on_beat, "the theme enters on a beat of the map track");
    assert!(e >= asked && (e - asked) < u64::from(SR), "within a beat of the cue: {} samples", e - asked);
    let st = with.status();
    assert_eq!(st.theme, Some(t));
    assert!(st.theme_level > 0.99, "swelled in over {THEME_IN_SECS} s: level {}", st.theme_level);
    let _ = render(&mut plain, 1.0, 256);
    // Theme up: the output is the theme, not the map (the map is ducked out after THEME_DUCK_SECS).
    let _ = render(&mut with, THEME_DUCK_SECS, 256);
    let a = render(&mut plain, 3.0, 256);
    let b = render(&mut with, 3.0, 256);
    let diff: f32 = a.iter().zip(&b).map(|(x, y)| (x - y) * (x - y)).sum::<f32>() / a.len() as f32;
    let sig: f32 = a.iter().map(|x| x * x).sum::<f32>() / a.len() as f32;
    assert!(diff > sig * 0.5, "the theme replaces the map track: diff {diff} vs map {sig}");
    let lv = |x: &[f32]| integrated_lufs(x, SR);
    assert!((lv(&b) - lv(&a)).abs() < 3.0, "similar level: map {:.1} vs theme {:.1}", lv(&a), lv(&b));
    // Theme off: fades out over THEME_OUT_SECS; the map track swells back IN PLACE (it kept sequencing),
    // so once the tails are gone the output is the plain map track again.
    let _ = render(&mut plain, THEME_DUCK_SECS, 256);
    with.command(MusicCommand::Theme { track: None });
    let _ = render(&mut plain, THEME_OUT_SECS + 3.0, 256);
    let _ = render(&mut with, THEME_OUT_SECS + 3.0, 256);
    assert_eq!(with.status().theme, None, "the theme layer retired");
    let a = render(&mut plain, 2.0, 256);
    let b = render(&mut with, 2.0, 256);
    let d: f32 = a.iter().zip(&b).map(|(x, y)| (x - y).abs()).fold(0.0, f32::max);
    assert!(d < 1e-3, "back to the map track, in place: max diff {d}");
    assert_eq!(plain.status().bars, with.status().bars, "the map track never lost its place");
}

#[test]
fn switching_themes_waits_for_the_themes_bar_line_and_repeats_are_no_ops() {
    let lib = lib();
    let a_id = theme(&lib, "open_sea_jig");
    let b_id = theme(&lib, "chrome_titans");
    let mut p = map_player(&lib);
    let _ = render(&mut p, 1.0, 256);
    p.command(MusicCommand::Theme { track: Some(a_id) });
    let _ = render(&mut p, 4.3, 256);
    // Repeating the playing theme changes nothing.
    let before = *p.theme_debug_log();
    p.command(MusicCommand::Theme { track: Some(a_id) });
    let _ = render(&mut p, 0.5, 256);
    assert_eq!(p.status().theme, Some(a_id));
    assert_eq!(p.theme_debug_log().bars[0], before.bars[0], "no restart");
    // Switch: the new theme starts on one of the old theme's bar lines.
    let song = lib.track(a_id).expect("a");
    let bar_len = f64::from(song.bar_ticks) * 60.0 * f64::from(SR) / (f64::from(song.bpm) * f64::from(bd_music::notation::PPQ));
    p.command(MusicCommand::Theme { track: Some(b_id) });
    let mut last_a_bar = 0u64;
    let mut switched_at = None;
    for _ in 0..(4 * SR as usize / 256) {
        let log = *p.theme_debug_log();
        if p.status().theme == Some(a_id) && log.bar_n > 0 {
            last_a_bar = log.bars[(log.bar_n - 1) % 256];
        }
        let _ = render(&mut p, 256.0 / SR as f32, 256);
        if switched_at.is_none() && p.status().theme == Some(b_id) {
            switched_at = Some(p.theme_debug_log().bars[0]);
        }
    }
    let s = switched_at.expect("switched");
    let k = (s - last_a_bar) as f64 / bar_len;
    assert!(k.round() >= 1.0 && ((s - last_a_bar) as f64 - k.round() * bar_len).abs() <= 1.5, "on a bar line: {k:.4} bars");
    assert!(p.status().theme_level > 0.99, "the new theme plays at full level");
}

#[test]
fn a_ko_stinger_lands_on_the_themes_beat_and_big_stingers_cut_the_theme() {
    let lib = lib();
    let t = theme(&lib, "menace_disco_strut");
    let mut p = map_player(&lib);
    let _ = render(&mut p, 1.0, 256);
    p.command(MusicCommand::Theme { track: Some(t) });
    let _ = render(&mut p, 3.37, 256);
    p.command(MusicCommand::Stinger(StingerKind::Ko));
    let _ = render(&mut p, 1.5, 256);
    let at = p.debug_log().stinger_start.expect("stinger started");
    let tl = p.theme_debug_log();
    assert!(tl.beats.iter().take(tl.beat_n.min(64)).any(|&b| b == at), "the KO lands on the theme's beat");
    assert_eq!(p.status().theme, Some(t), "a KO leaves the theme up");
    // Match point holds the map track and plays its stinger: the theme gives way quickly.
    p.command(MusicCommand::Stinger(StingerKind::MatchPoint));
    let _ = render(&mut p, 0.1, 256);
    let _ = render(&mut p, 0.6, 256);
    assert!(p.status().theme.is_none() || p.status().theme_level < 0.05, "cut: level {}", p.status().theme_level);
    // A theme asked for while a big stinger is pending gives way too; once it has played, the
    // theme can come back (the director re-sends it on the next use).
    let _ = render(&mut p, 7.0, 256);
    p.command(MusicCommand::Theme { track: Some(t) });
    let _ = render(&mut p, 5.0, 256);
    assert_eq!(p.status().theme, Some(t), "back after the stinger");
    // Stop cuts it too.
    p.command(MusicCommand::Stop);
    let _ = render(&mut p, 3.0, 256);
    assert_eq!(p.status().theme, None);
}

#[test]
fn a_theme_plays_with_nothing_else_and_is_callback_size_independent() {
    let lib = lib();
    let t = theme(&lib, "four_winds_accord");
    let run = |block: usize| {
        let mut p = MusicPlayer::new(Arc::clone(&lib), SR);
        p.command(MusicCommand::Theme { track: Some(t) });
        let mut out = render(&mut p, 2.0, block);
        assert_eq!(p.status().theme, Some(t), "starts at once in silence");
        p.command(MusicCommand::Play { track: lib.track_id("miami").expect("miami"), skip_intro: true });
        out.extend(render(&mut p, 2.0, block));
        p.command(MusicCommand::Theme { track: None });
        out.extend(render(&mut p, 4.0, block));
        out
    };
    let a = run(256);
    let b = run(480);
    assert!(a.iter().any(|x| x.abs() > 1e-3), "audible");
    assert_eq!(a, b, "identical for any callback size");
}
