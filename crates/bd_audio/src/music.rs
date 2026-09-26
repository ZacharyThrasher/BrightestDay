//! The soundtrack: bd_music's adaptive chip player on the Music bus, and the game-side director
//! that picks tracks and fires stingers.
//!
//! * [`ChipMusic`] adapts [`bd_music::MusicPlayer`] to [`MusicSource`] (the mixer owns it on the
//!   audio thread; no allocation there).
//! * [`MusicDirector`] (game thread): which track for the scene / map / state (explore ↔ combat
//!   with hysteresis, final stock, sudden death, party, results), stingers (KO throttled to one per
//!   2 s, streak, match point, last standing, victory / defeat) and the Style setting.
//!   Intensity and tempo come from the mix snapshots (`music_intensity` / `music_tempo`).

use std::sync::Arc;

use bd_music::{MusicLibrary, MusicPlayer, StingerKind, TrackId};
use serde::{Deserialize, Serialize};

use crate::director::Scene;
use crate::dsp::Rng;
use crate::engine::AudioEngine;
use crate::mixer::{MusicCommand, MusicSource};

/// [`MusicSource`] over bd_music's player.
pub struct ChipMusic {
    player: MusicPlayer,
}

impl ChipMusic {
    pub fn new(lib: Arc<MusicLibrary>, sample_rate: u32) -> Self {
        Self { player: MusicPlayer::new(lib, sample_rate) }
    }
}

impl MusicSource for ChipMusic {
    fn render(&mut self, out: &mut [f32]) {
        self.player.render(out);
    }
    fn command(&mut self, cmd: MusicCommand) {
        let c = match cmd {
            MusicCommand::Play { track, skip_intro } => bd_music::MusicCommand::Play { track: TrackId(track), skip_intro },
            MusicCommand::Stop => bd_music::MusicCommand::Stop,
            MusicCommand::Intensity(x) => bd_music::MusicCommand::SetIntensity(x),
            MusicCommand::Tempo { multiplier, ramp_secs } => bd_music::MusicCommand::SetTempo { multiplier, ramp_secs },
            MusicCommand::Stinger(k) => match StingerKind::ALL.get(k as usize) {
                Some(&k) => bd_music::MusicCommand::Stinger(k),
                None => return,
            },
        };
        self.player.command(c);
    }
    fn sub_bass(&self) -> bool {
        // REMIX HOOK: when bd_music's variant API lands, return the current track's `sub_bass`
        // flag (e.g. `self.player.status().track` → `library().track(id).sub_bass`).
        false
    }
}

/// Settings → Music style.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MusicStyle {
    /// The chip arrangements.
    #[default]
    Classic,
    /// The remix variants (808 / sub-bass) where a track has one.
    Remixes,
    /// Pick per track change.
    Shuffle,
}

impl MusicStyle {
    pub fn next(self, dir: i32) -> Self {
        let all = [MusicStyle::Classic, MusicStyle::Remixes, MusicStyle::Shuffle];
        let i = all.iter().position(|s| *s == self).unwrap_or(0) as i32;
        all[(i + dir).rem_euclid(3) as usize]
    }
    pub fn label(self) -> &'static str {
        match self {
            MusicStyle::Classic => "CLASSIC",
            MusicStyle::Remixes => "REMIXES",
            MusicStyle::Shuffle => "SHUFFLE",
        }
    }
}

/// What the soundtrack follows.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MusicInputs {
    pub scene: Scene,
    /// Map key (`archipelago`, `miami`, `orbit`, `tutorial`, …).
    pub map: &'static str,
    /// Combat weight 0..1 (the mix director's).
    pub combat: f32,
    /// Someone is on their last stock / the last minute.
    pub final_stock: bool,
    /// Overtime.
    pub sudden_death: bool,
    /// Party mode match / card pick.
    pub party: bool,
    /// Match over: Some(true) = local player won.
    pub results: Option<bool>,
}

/// Game-thread soundtrack director.
pub struct MusicDirector {
    lib: Arc<MusicLibrary>,
    pub style: MusicStyle,
    current: Option<&'static str>,
    fighting: bool,
    calm_for: f32,
    since_ko: f32,
    rng: Rng,
    ended: bool,
}

impl MusicDirector {
    pub fn new(lib: Arc<MusicLibrary>, style: MusicStyle, seed: u64) -> Self {
        Self { lib, style, current: None, fighting: false, calm_for: 0.0, since_ko: 99.0, rng: Rng::new(seed ^ 0x6d75), ended: false }
    }

    pub fn current(&self) -> Option<&'static str> {
        self.current
    }

    /// The track a state wants.
    fn want(&mut self, inp: &MusicInputs, dt: f32) -> &'static str {
        match inp.scene {
            Scene::Menu => return "menu",
            Scene::Lobby => return if inp.party { "party" } else { "lobby" },
            Scene::InGame => {}
        }
        if let Some(_won) = inp.results {
            return if inp.party { "party_results" } else { "results" };
        }
        if inp.sudden_death {
            return "sudden_death";
        }
        if inp.final_stock {
            return "final_stock";
        }
        if inp.party {
            return "party";
        }
        // Explore ↔ combat with hysteresis: fight starts at 0.5, ends after 8 s under 0.15.
        if inp.combat > 0.5 {
            self.fighting = true;
            self.calm_for = 0.0;
        } else if inp.combat < 0.15 {
            self.calm_for += dt;
            if self.calm_for > 8.0 {
                self.fighting = false;
            }
        }
        match (inp.map, self.fighting) {
            ("tutorial", _) => "tutorial",
            ("miami", _) => "miami",
            ("orbit", false) => "orbit_float",
            ("orbit", true) => "orbit_combat",
            (_, false) => "archipelago_explore",
            (_, true) => "archipelago_combat",
        }
    }

    /// Resolve a track under the Style setting (remix variants when present).
    fn resolve(&mut self, name: &str) -> Option<TrackId> {
        let remix = match self.style {
            MusicStyle::Classic => false,
            MusicStyle::Remixes => true,
            MusicStyle::Shuffle => self.rng.f32() < 0.5,
        };
        if remix {
            // REMIX HOOK: variant naming is up to bd_music; `<name>_remix` is tried first.
            let mut buf = [0u8; 64];
            let n = name.len().min(56);
            buf[..n].copy_from_slice(&name.as_bytes()[..n]);
            buf[n..n + 6].copy_from_slice(b"_remix");
            if let Ok(s) = std::str::from_utf8(&buf[..n + 6])
                && let Some(id) = self.lib.track_id(s)
            {
                return Some(id);
            }
        }
        self.lib.track_id(name)
    }

    /// Per frame.
    pub fn update(&mut self, dt: f32, inp: &MusicInputs, engine: &mut AudioEngine) {
        self.since_ko += dt.max(0.0);
        if inp.results.is_some() && inp.scene == Scene::InGame {
            if !self.ended {
                self.ended = true;
                let k = if inp.results == Some(true) { StingerKind::Victory } else { StingerKind::Defeat };
                self.stinger(k, engine);
            }
        } else {
            self.ended = false;
        }
        let want = self.want(inp, dt);
        if self.current != Some(want) {
            // Victory/defeat end the track by themselves; results music follows once it has.
            if let Some(id) = self.resolve(want) {
                engine.music(MusicCommand::Play { track: id.0, skip_intro: false });
            }
            self.current = Some(want);
        }
    }

    /// Fire a stinger (KOs throttled to one per 2 s).
    pub fn stinger(&mut self, k: StingerKind, engine: &mut AudioEngine) {
        if k == StingerKind::Ko {
            if self.since_ko < 2.0 {
                return;
            }
            self.since_ko = 0.0;
        }
        engine.music(MusicCommand::Stinger(k.index() as u8));
    }

    /// Stop the music (loading, alt-tab).
    pub fn stop(&mut self, engine: &mut AudioEngine) {
        engine.music(MusicCommand::Stop);
        self.current = None;
    }
}
