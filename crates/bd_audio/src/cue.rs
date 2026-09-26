//! Cue table (`assets/audio/cues.ron`): how each sound behaves in the mix — which bus it feeds,
//! its priority and voice limit, distance model, air absorption, reverb send, frequency carve, and
//! its micro-variation (pitch in cents, gain jitter in dB). Resolution order: per-sound entry >
//! category entry (the manifest's `category`) > `defaults`.

use std::collections::HashMap;

use serde::Deserialize;

use crate::bank::SoundBank;

/// Mixer buses. The first five form the **World** group (HDR window, shared reverb, occlusion,
/// sidechain-ducked by Feedback); Feedback / Ui / Music sit beside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize, Default)]
pub enum Bus {
    /// Hits, explosions, shatter, destruction.
    #[default]
    Impacts = 0,
    /// Guns, casings, reloads.
    Weapons = 1,
    /// Footsteps, jumps, landings, tethers, boost, slide — the body.
    Foley = 2,
    /// The ring's hard-light constructs.
    Constructs = 3,
    /// Beds + positional ambience.
    Ambience = 4,
    /// Hit confirms, crits, KO confirms, streak, heartbeat: "you did something". Ducks the World.
    Feedback = 5,
    /// Menus, typing.
    Ui = 6,
    /// Adaptive soundtrack (driven by a [`crate::mixer::MusicSource`] on the audio thread).
    Music = 7,
}

pub const NUM_BUSES: usize = 8;
pub const WORLD_BUSES: usize = 5;
pub const ALL_BUSES: [Bus; NUM_BUSES] =
    [Bus::Impacts, Bus::Weapons, Bus::Foley, Bus::Constructs, Bus::Ambience, Bus::Feedback, Bus::Ui, Bus::Music];

impl Bus {
    #[inline]
    pub fn index(self) -> usize {
        self as usize
    }
    #[inline]
    pub fn is_world(self) -> bool {
        (self as usize) < WORLD_BUSES
    }
    pub fn name(self) -> &'static str {
        match self {
            Bus::Impacts => "Impacts",
            Bus::Weapons => "Weapons",
            Bus::Foley => "Foley",
            Bus::Constructs => "Constructs",
            Bus::Ambience => "Ambience",
            Bus::Feedback => "Feedback",
            Bus::Ui => "Ui",
            Bus::Music => "Music",
        }
    }
    pub fn from_index(i: usize) -> Bus {
        ALL_BUSES.get(i).copied().unwrap_or(Bus::Impacts)
    }
}

/// Fully resolved per-sound mix behaviour (plain data, copied into every voice).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CueParams {
    pub bus: Bus,
    /// 0..100. Higher survives voice stealing.
    pub priority: f32,
    /// Real (rendered) instances of this sound at once; the rest go virtual (the closest/loudest win).
    pub max_instances: u8,
    /// Full volume inside this distance (m).
    pub min_dist: f32,
    /// Silent (virtual) beyond this distance (m).
    pub max_dist: f32,
    /// Distance exponent: gain = (min_dist / d)^rolloff.
    pub rolloff: f32,
    /// Air absorption scale (1 = normal; highs roll off with distance).
    pub air: f32,
    /// Reverb send (dB, relative to the dry level at the source).
    pub send_db: f32,
    /// Participates in the HDR loudness window (World sounds do; Feedback / Ui don't).
    pub hdr: bool,
    /// Static trim (dB).
    pub gain_db: f32,
    /// Random pitch per play, ± cents.
    pub pitch_cents: f32,
    /// Random gain per play, ± dB.
    pub gain_jitter_db: f32,
    /// Frequency carve: high-pass (Hz, 0 = off) and low-pass (Hz, 0 = off) for this role.
    pub hpf_hz: f32,
    pub lpf_hz: f32,
    /// Distance low-cut: far sources lose their body (Hz reached at ~45 m; 0 = off). Stops the
    /// sub punch of every distant gun/impact in a brawl from summing into a constant rumble.
    pub far_hpf_hz: f32,
    /// Minimum time between two plays of this sound (s); extra plays are dropped (0 = off).
    pub cooldown: f32,
    /// Occlusion strength multiplier (0 = ignores walls, e.g. body-conducted own sounds).
    pub occlusion: f32,
    /// Stereo width for stereo files (0 = mono sum, 1 = full).
    pub width: f32,
}

impl Default for CueParams {
    fn default() -> Self {
        Self {
            bus: Bus::Impacts,
            priority: 50.0,
            max_instances: 6,
            min_dist: 3.0,
            max_dist: 140.0,
            rolloff: 1.0,
            air: 1.0,
            send_db: -10.0,
            hdr: true,
            gain_db: 0.0,
            pitch_cents: 30.0,
            gain_jitter_db: 1.0,
            hpf_hz: 0.0,
            lpf_hz: 0.0,
            far_hpf_hz: 0.0,
            cooldown: 0.0,
            occlusion: 1.0,
            width: 1.0,
        }
    }
}

/// Partial cue (every field optional) as written in `cues.ron`.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct CueSpec {
    pub bus: Option<Bus>,
    pub priority: Option<f32>,
    pub max_instances: Option<u8>,
    pub min_dist: Option<f32>,
    pub max_dist: Option<f32>,
    pub rolloff: Option<f32>,
    pub air: Option<f32>,
    pub send_db: Option<f32>,
    pub hdr: Option<bool>,
    pub gain_db: Option<f32>,
    pub pitch_cents: Option<f32>,
    pub gain_jitter_db: Option<f32>,
    pub hpf_hz: Option<f32>,
    pub lpf_hz: Option<f32>,
    pub far_hpf_hz: Option<f32>,
    pub cooldown: Option<f32>,
    pub occlusion: Option<f32>,
    pub width: Option<f32>,
}

impl CueSpec {
    fn apply(&self, p: &mut CueParams) {
        macro_rules! set {
            ($($f:ident),*) => { $( if let Some(v) = self.$f { p.$f = v; } )* };
        }
        set!(
            bus,
            priority,
            max_instances,
            min_dist,
            max_dist,
            rolloff,
            air,
            send_db,
            hdr,
            gain_db,
            pitch_cents,
            gain_jitter_db,
            hpf_hz,
            lpf_hz,
            far_hpf_hz,
            cooldown,
            occlusion,
            width
        );
    }
}

/// `assets/audio/cues.ron`.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct CueTable {
    pub defaults: CueSpec,
    pub categories: HashMap<String, CueSpec>,
    pub sounds: HashMap<String, CueSpec>,
}

impl CueTable {
    pub fn from_ron(text: &str) -> Result<Self, String> {
        ron::from_str(text).map_err(|e| e.to_string())
    }
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        let t = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::from_ron(&t)
    }
    /// Resolve one sound.
    pub fn resolve(&self, name: &str, category: &str) -> CueParams {
        let mut p = CueParams::default();
        self.defaults.apply(&mut p);
        if let Some(c) = self.categories.get(category) {
            c.apply(&mut p);
        }
        if let Some(s) = self.sounds.get(name) {
            s.apply(&mut p);
        }
        sanitize(&mut p);
        p
    }
    /// Resolve every sound of a bank (indexed by `SoundId`).
    pub fn resolve_bank(&self, bank: &SoundBank) -> Vec<CueParams> {
        bank.sounds.iter().map(|s| self.resolve(&s.name, &s.category)).collect()
    }
}

fn sanitize(p: &mut CueParams) {
    let f = |x: f32, lo: f32, hi: f32, d: f32| if x.is_finite() { x.clamp(lo, hi) } else { d };
    p.priority = f(p.priority, 0.0, 100.0, 50.0);
    p.max_instances = p.max_instances.max(1);
    p.min_dist = f(p.min_dist, 0.1, 1000.0, 3.0);
    p.max_dist = f(p.max_dist, p.min_dist + 0.1, 5000.0, 140.0);
    p.rolloff = f(p.rolloff, 0.0, 4.0, 1.0);
    p.air = f(p.air, 0.0, 10.0, 1.0);
    p.send_db = f(p.send_db, -120.0, 12.0, -10.0);
    p.gain_db = f(p.gain_db, -60.0, 24.0, 0.0);
    p.pitch_cents = f(p.pitch_cents, 0.0, 1200.0, 0.0);
    p.gain_jitter_db = f(p.gain_jitter_db, 0.0, 12.0, 0.0);
    p.hpf_hz = f(p.hpf_hz, 0.0, 20000.0, 0.0);
    p.lpf_hz = f(p.lpf_hz, 0.0, 24000.0, 0.0);
    p.far_hpf_hz = f(p.far_hpf_hz, 0.0, 2000.0, 0.0);
    p.cooldown = f(p.cooldown, 0.0, 10.0, 0.0);
    p.occlusion = f(p.occlusion, 0.0, 2.0, 1.0);
    p.width = f(p.width, 0.0, 1.0, 1.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolution_order() {
        let t = CueTable::from_ron(
            r#"(
                defaults: (priority: Some(40.0)),
                categories: { "movement": (bus: Some(Foley), priority: Some(20.0)) },
                sounds: { "footstep": (max_instances: Some(3)) },
            )"#,
        )
        .unwrap();
        let p = t.resolve("footstep", "movement");
        assert_eq!(p.bus, Bus::Foley);
        assert_eq!(p.priority, 20.0);
        assert_eq!(p.max_instances, 3);
        let q = t.resolve("other", "none");
        assert_eq!(q.priority, 40.0);
        assert_eq!(q.bus, Bus::Impacts);
    }
}
