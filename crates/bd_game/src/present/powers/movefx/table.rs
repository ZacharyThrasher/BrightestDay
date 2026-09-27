//! `assets/tuning/move_fx.ron` — the move presentation table (hot-reloaded; mtime poll like
//! presence.ron; a parse error keeps the previous table). Content authors put a style KEY on a move
//! (`fx: "rubber"`); this file says what that key looks and sounds like. A new key here is a new
//! style — no code. The closed set in code is the [`Body`] (how a trace / limb / burst is SHAPED);
//! a style is a body + colours + numbers + sounds.

use bd_core::mechanics::Element;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::SystemTime;

/// How the grammar's pieces are SHAPED (closed set; the style picks one).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Body {
    /// Clean hard light: beams, bright cube motes, shell rings (the default).
    #[default]
    Energy,
    /// Elastic: limbs stretch from the shoulder, thin as they stretch, wobble, band-snap back.
    Rubber,
    /// Rigid: telescoping segments with bright bands, straight lines, metallic.
    Rigid,
    /// Lit puffs that bloom and linger (BAMF, smoke, steam, shadow clouds).
    Smoke,
    /// Jagged zig-zags re-struck every frame, white-hot core (lightning).
    Bolt,
    /// Long velocity-stretched streaks, afterimages, crackle (speedforce, instant speed).
    Streak,
    /// Drops on arcs with gravity, splash crowns (water, blood, ink).
    Liquid,
    /// Many small falling grains, dust walls (sand, earth, ash).
    Grain,
    /// Rising embers + a hot core (fire, cursed flame).
    Flame,
    /// Hard shards that hold, then shatter (ice, crystal).
    Shard,
    /// Spiralling thin lines (wind, chi currents).
    Gust,
    /// Offset blinking cubes, stepped motion (glitch, digital).
    Glitch,
    /// Flat fluttering sheets (paper, cards, petals).
    Paper,
    /// Rings and halos, rising light pillars (holy, psychic).
    Halo,
    /// Implodes first, dark core with a bright rim (void, gravity, cursed).
    Void,
}

/// A sound: (manifest name, pitch ×, gain). `""` = none.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Cue(pub String, #[serde(default = "one")] pub f32, #[serde(default = "one")] pub f32);

fn one() -> f32 {
    1.0
}

impl Cue {
    pub fn is_some(&self) -> bool {
        !self.0.is_empty()
    }
}

/// The sounds of a style, layered over the op's own verb sound.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StyleSound {
    /// Under the release (the body's signature: a boing, a BAMF crack, a metallic ring…).
    pub release: Cue,
    /// Where it lands / arrives / connects.
    pub impact: Cue,
    /// The clean exit (band snap-back, telescoping collapse, smoke hiss).
    pub exit: Cue,
    /// A stretch / extend under a limb or line (rubber creak, telescoping clacks).
    pub stretch: Cue,
}

/// One style (a key authors write in `fx:`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StyleDef {
    pub body: Body,
    /// Main colour, sRGB 0..1; `None` = the move's colour (juice.color / element / power / corps).
    pub color: Option<[f32; 3]>,
    /// Hot core colour (sRGB).
    pub core: [f32; 3],
    /// Secondary / shadow colour (sRGB): smoke body, ink, the dark of a void.
    pub dark: [f32; 3],
    /// Emission multiplier (≥ 1 blooms).
    pub glow: f32,
    /// Mote count multiplier.
    pub density: f32,
    /// Motes rise (+ m/s: fire, steam) or fall (− : sand, water, blood).
    pub rise: f32,
    /// Trace zig-zag amplitude (m): lightning.
    pub jitter: f32,
    /// Elastic wobble amplitude (m) of limbs / lines: rubber.
    pub wobble: f32,
    /// Beat time scale (< 1 snappier, > 1 slower).
    pub speed: f32,
    /// Exit / linger time scale (smoke lingers, speedforce vanishes).
    pub linger: f32,
    /// Ghost silhouettes left along movement (0 = none).
    pub afterimages: u8,
    /// Mote size multiplier.
    pub size: f32,
    pub sound: StyleSound,
    /// HUD chip glyph (power_hud `derive::glyph` ids).
    pub glyph: u8,
}

impl Default for StyleDef {
    fn default() -> Self {
        Self {
            body: Body::Energy,
            color: None,
            core: [1.0, 1.0, 1.0],
            dark: [0.1, 0.1, 0.12],
            glow: 3.0,
            density: 1.0,
            rise: 0.0,
            jitter: 0.0,
            wobble: 0.0,
            speed: 1.0,
            linger: 1.0,
            afterimages: 0,
            size: 1.0,
            sound: StyleSound::default(),
            glyph: 0,
        }
    }
}

/// The verb sound of each op kind (under the style's body sound).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OpSounds {
    pub blink_depart: Cue,
    pub blink_arrive: Cue,
    pub dash: Cue,
    pub skid: Cue,
    pub reel: Cue,
    pub reel_anchor: Cue,
    pub shove: Cue,
    pub crack: Cue,
    pub slam_dread: Cue,
    pub slam_land: Cue,
    pub field: Cue,
    pub cross: Cue,
    pub swap: Cue,
    pub time: Cue,
    pub explode: Cue,
    pub carve: Cue,
    pub cast: Cue,
    pub beam: Cue,
    pub pulse: Cue,
}

/// Global beat knobs (seconds / metres; styles scale the times).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Beats {
    /// Gather implosion before a blink / cast release (plays concurrently: never delays the sim).
    pub gather_secs: f32,
    /// How long a blink trace / swap arc hangs.
    pub trace_secs: f32,
    /// Dash streak ribbons / afterimage window.
    pub dash_secs: f32,
    /// Afterimage interval (s) and life (s).
    pub ghost_every: f32,
    pub ghost_life: f32,
    /// Shove cone shockwave travel time.
    pub shove_secs: f32,
    /// Limb extend / hold / snap-back (rubber arm, rigid pole) for Shove and Cast.
    pub limb_out: f32,
    pub limb_hold: f32,
    pub limb_back: f32,
    /// Reel line: max seconds, arrival distance (m).
    pub reel_secs: f32,
    pub reel_arrive: f32,
    /// SlamOnLand: longest airborne wait (s) for the landing.
    pub slam_wait: f32,
    /// Base mote count of a burst (× style density × graphics particle budget).
    pub motes: f32,
    /// Local player's own beats: mote scale near the camera (never a wall in your face).
    pub own_scale: f32,
    /// Beats alive at once (pooled; the oldest is recycled).
    pub max_beats: usize,
}

impl Default for Beats {
    fn default() -> Self {
        Self {
            gather_secs: 0.12,
            trace_secs: 0.28,
            dash_secs: 0.45,
            ghost_every: 0.05,
            ghost_life: 0.28,
            shove_secs: 0.16,
            limb_out: 0.09,
            limb_hold: 0.06,
            limb_back: 0.16,
            reel_secs: 1.1,
            reel_arrive: 2.2,
            slam_wait: 3.0,
            motes: 22.0,
            own_scale: 0.55,
            max_beats: 96,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MoveFxTable {
    pub beats: Beats,
    pub sounds: OpSounds,
    /// Style key → style.
    pub styles: BTreeMap<String, StyleDef>,
    /// Other spellings authors may use ("tech" → "hardlight", "bamf" → "smoke").
    pub aliases: BTreeMap<String, String>,
    /// Default style of a move of this element when nothing names one.
    pub elements: Vec<(Element, String)>,
}

impl Default for MoveFxTable {
    fn default() -> Self {
        let mut styles = BTreeMap::new();
        styles.insert("energy".to_string(), StyleDef::default());
        Self { beats: Beats::default(), sounds: OpSounds::default(), styles, aliases: BTreeMap::new(), elements: Vec::new() }
    }
}

/// The table flattened for the hot path: styles by index (beats store a `u16`).
#[derive(Resource, Clone, Debug)]
pub struct MoveFx {
    pub t: MoveFxTable,
    pub keys: Vec<String>,
    pub defs: Vec<StyleDef>,
    /// Index of "energy" (the fallback).
    pub energy: u16,
}

impl MoveFx {
    pub fn new(t: MoveFxTable) -> Self {
        let keys: Vec<String> = t.styles.keys().cloned().collect();
        let mut defs: Vec<StyleDef> = t.styles.values().cloned().collect();
        let energy = match keys.iter().position(|k| k == "energy") {
            Some(i) => i as u16,
            None => {
                defs.push(StyleDef::default());
                (defs.len() - 1) as u16
            }
        };
        Self { t, keys, defs, energy }
    }

    /// A key (case / space / dash insensitive, aliases followed) → style index.
    pub fn find(&self, key: &str) -> Option<u16> {
        let key = key.trim();
        if key.is_empty() {
            return None;
        }
        let eq = |a: &str, b: &str| {
            let mut x = a.bytes().filter(|c| !matches!(c, b' ' | b'_' | b'-')).map(|c| c.to_ascii_lowercase());
            let mut y = b.bytes().filter(|c| !matches!(c, b' ' | b'_' | b'-')).map(|c| c.to_ascii_lowercase());
            loop {
                match (x.next(), y.next()) {
                    (None, None) => return true,
                    (a, b) if a == b => continue,
                    _ => return false,
                }
            }
        };
        if let Some(i) = self.keys.iter().position(|k| eq(k, key)) {
            return Some(i as u16);
        }
        let target = self.t.aliases.iter().find(|(a, _)| eq(a, key)).map(|(_, t)| t.as_str())?;
        self.keys.iter().position(|k| eq(k, target)).map(|i| i as u16)
    }

    pub fn for_element(&self, e: Element) -> Option<u16> {
        self.t.elements.iter().find(|(k, _)| *k == e).and_then(|(_, s)| self.find(s))
    }

    pub fn def(&self, i: u16) -> &StyleDef {
        self.defs.get(i as usize).unwrap_or(&self.defs[self.energy as usize])
    }
}

impl Default for MoveFx {
    fn default() -> Self {
        Self::new(MoveFxTable::default())
    }
}

pub fn path() -> PathBuf {
    crate::assets_dir().join("tuning").join("move_fx.ron")
}

pub fn parse(text: &str) -> Result<MoveFxTable, ron::error::SpannedError> {
    ron::from_str::<MoveFxTable>(text)
}

pub fn load() -> MoveFx {
    match std::fs::read_to_string(path()) {
        Ok(text) => MoveFx::new(parse(&text).unwrap_or_else(|e| {
            eprintln!("move fx: failed to parse {}: {e}; using defaults", path().display());
            MoveFxTable::default()
        })),
        Err(_) => MoveFx::default(),
    }
}

pub fn hot_reload(time: Res<Time<Real>>, mut fx: ResMut<MoveFx>, mut last: Local<(f32, Option<SystemTime>)>) {
    last.0 += time.delta_secs();
    if last.0 < 0.5 {
        return;
    }
    last.0 = 0.0;
    let Ok(modified) = std::fs::metadata(path()).and_then(|m| m.modified()) else { return };
    match last.1 {
        None => last.1 = Some(modified),
        Some(prev) if prev != modified => {
            last.1 = Some(modified);
            if let Ok(text) = std::fs::read_to_string(path()) {
                match parse(&text) {
                    Ok(t) => {
                        info!("move fx reloaded");
                        *fx = MoveFx::new(t);
                    }
                    Err(e) => warn!("move fx: parse error, keeping previous: {e}"),
                }
            }
        }
        _ => {}
    }
}
