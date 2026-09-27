//! **Overlay layers**: named percussion patterns a game cue toggles on top of whatever track is
//! playing — Gear Fifth's bouncy "drums of liberation" first; any future power (or map moment) can
//! name one ([`overlay_id`] → [`crate::MusicCommand::Overlay`]).
//!
//! * Locked to the playing track: every 16th of the main deck's grid (its tempo, tempo ramps and
//!   bar lines) steps the pattern, so the layer always grooves with the song, at any bpm.
//! * Fades in / out ([`OverlayDef::fade_in`] / [`OverlayDef::fade_out`]); voices ring out after
//!   off. Silent while no track plays (nothing to lock to).
//! * Mixed into the drive stem after the deck (so it survives calm intensity), pre output filters.
//! * Pre-allocated, no allocation / locks on the audio thread (fixed voice pools, `Copy` state).

use crate::synth::{CONTROL, Drum, DrumVoice};

/// Most overlay layers (the table is static; each has its own voices and fade).
pub const MAX_OVERLAYS: usize = 4;
/// Voices per overlay (round robin; a re-used voice is choked).
const VOICES: usize = 6;
/// Pattern length in 16ths (two bars of 4/4; shorter bars wrap early).
pub const PATTERN_STEPS: u8 = 32;

/// An overlay layer (index into the static table).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OverlayId(pub u8);

/// One drum hit of a pattern.
#[derive(Clone, Copy, Debug)]
pub struct OverlayHit {
    /// 16th within the two-bar pattern (0..[`PATTERN_STEPS`]).
    pub step: u8,
    pub drum: Drum,
    pub vel: f32,
    /// Semitones.
    pub pitch: f32,
    /// Stereo position -1..1.
    pub pan: f32,
}

/// A named overlay layer.
#[derive(Clone, Copy, Debug)]
pub struct OverlayDef {
    pub name: &'static str,
    pub hits: &'static [OverlayHit],
    pub gain: f32,
    /// Seconds to fade in / out.
    pub fade_in: f32,
    pub fade_out: f32,
}

const fn h(step: u8, drum: Drum, vel: f32, pitch: f32, pan: f32) -> OverlayHit {
    OverlayHit { step, drum, vel, pitch, pan }
}

/// Gear Fifth: bouncy, playful taiko-ish toms with a cowbell skip and a boing-y blip; bar two
/// answers with a tumbling tom fill.
const DRUMS: &[OverlayHit] = &[
    // Bar one: boom . . boom . . boom . | BOOM . ba . dum . ba dum
    h(0, Drum::TomLow, 1.0, -2.0, -0.15),
    h(2, Drum::Cowbell, 0.34, 0.0, 0.35),
    h(3, Drum::TomLow, 0.72, -2.0, 0.15),
    h(4, Drum::Clap, 0.5, 0.0, 0.0),
    h(6, Drum::TomLow, 0.86, 0.0, -0.1),
    h(7, Drum::Cowbell, 0.3, 5.0, 0.35),
    h(8, Drum::TomMid, 0.92, 0.0, 0.2),
    h(10, Drum::TomLow, 0.8, -2.0, -0.2),
    h(11, Drum::Cowbell, 0.34, 0.0, 0.35),
    h(12, Drum::Clap, 0.5, 0.0, 0.0),
    h(13, Drum::Blip, 0.26, 12.0, -0.3),
    h(14, Drum::TomMid, 0.72, 3.0, 0.25),
    h(15, Drum::Blip, 0.24, 19.0, 0.3),
    // Bar two: same bounce, then the fill.
    h(16, Drum::TomLow, 1.0, -2.0, -0.15),
    h(18, Drum::Cowbell, 0.34, 0.0, 0.35),
    h(19, Drum::TomLow, 0.72, -2.0, 0.15),
    h(20, Drum::Clap, 0.5, 0.0, 0.0),
    h(22, Drum::TomLow, 0.86, 0.0, -0.1),
    h(23, Drum::Cowbell, 0.3, 5.0, 0.35),
    h(24, Drum::TomMid, 0.9, 5.0, 0.3),
    h(25, Drum::TomMid, 0.7, 3.0, 0.15),
    h(26, Drum::TomMid, 0.78, 0.0, 0.0),
    h(27, Drum::TomLow, 0.74, 0.0, -0.15),
    h(28, Drum::Clap, 0.55, 0.0, 0.0),
    h(28, Drum::TomLow, 0.9, -2.0, -0.25),
    h(30, Drum::TomMid, 0.8, 7.0, 0.3),
    h(31, Drum::Blip, 0.3, 24.0, 0.0),
];

/// The overlay table. Names are what `powers.ron` `ascend.music` (and any other cue) say.
pub const OVERLAYS: &[OverlayDef] = &[OverlayDef { name: "drums", hits: DRUMS, gain: 0.5, fade_in: 1.2, fade_out: 1.6 }];

/// The overlay called `name` (case-insensitive).
pub fn overlay_id(name: &str) -> Option<OverlayId> {
    OVERLAYS.iter().take(MAX_OVERLAYS).position(|o| o.name.eq_ignore_ascii_case(name.trim())).map(|i| OverlayId(i as u8))
}

pub fn overlay_name(id: OverlayId) -> Option<&'static str> {
    OVERLAYS.get(usize::from(id.0)).map(|o| o.name)
}

/// One layer's playback state (audio thread).
#[derive(Clone, Copy)]
struct Layer {
    on: bool,
    gain: f32,
    voices: [DrumVoice; VOICES],
    pans: [f32; VOICES],
    next: usize,
}

/// Every overlay layer of a player (owned by [`crate::MusicPlayer`]).
pub(crate) struct Overlays {
    sr: f32,
    layers: [Layer; MAX_OVERLAYS],
    mono: [f32; CONTROL],
}

impl Overlays {
    pub(crate) fn new(sr: f32) -> Self {
        let l = Layer { on: false, gain: 0.0, voices: [DrumVoice::new(sr); VOICES], pans: [0.0; VOICES], next: 0 };
        Self { sr, layers: [l; MAX_OVERLAYS], mono: [0.0; CONTROL] }
    }

    pub(crate) fn set(&mut self, id: OverlayId, on: bool) {
        if usize::from(id.0) < OVERLAYS.len().min(MAX_OVERLAYS) {
            self.layers[usize::from(id.0)].on = on;
        }
    }

    /// Bit `i` = overlay `i` is on.
    pub(crate) fn mask(&self) -> u8 {
        self.layers.iter().enumerate().fold(0, |m, (i, l)| if l.on { m | 1 << i } else { m })
    }

    fn busy(&self) -> bool {
        self.layers.iter().any(|l| l.on || l.gain > 0.0 || l.voices.iter().any(|v| v.is_active()))
    }

    /// Render one control block: `steps` = (sample offset, 16th of the pattern) the main deck
    /// crossed this block, in order. Adds into the stereo drive stem.
    pub(crate) fn render(&mut self, steps: &[(usize, u8)], out_l: &mut [f32; CONTROL], out_r: &mut [f32; CONTROL]) {
        if !self.busy() {
            return;
        }
        let block = CONTROL as f32 / self.sr;
        for (k, layer) in self.layers.iter_mut().enumerate() {
            let Some(def) = OVERLAYS.get(k) else { break };
            // Fade (per block, linear).
            let g0 = layer.gain;
            layer.gain = if layer.on { (g0 + block / def.fade_in.max(0.01)).min(1.0) } else { (g0 - block / def.fade_out.max(0.01)).max(0.0) };
            let g1 = layer.gain;
            if g0 <= 0.0 && g1 <= 0.0 && !layer.voices.iter().any(|v| v.is_active()) {
                continue;
            }
            // Each voice renders into the mono scratch piecewise (hits land on their sample).
            let mut pos = 0usize;
            let mut outs = [[0.0f32; CONTROL]; 2];
            let seg = |from: usize, to: usize, layer: &mut Layer, mono: &mut [f32; CONTROL], outs: &mut [[f32; CONTROL]; 2]| {
                if to <= from {
                    return;
                }
                for (v, pan) in layer.voices.iter_mut().zip(layer.pans) {
                    if !v.is_active() {
                        continue;
                    }
                    mono[from..to].fill(0.0);
                    v.render_add(&mut mono[from..to], 1.0);
                    let (l, r) = ((1.0 - pan) * 0.5, (1.0 + pan) * 0.5);
                    let (l, r) = (l.sqrt(), r.sqrt());
                    for i in from..to {
                        outs[0][i] += mono[i] * l;
                        outs[1][i] += mono[i] * r;
                    }
                }
            };
            for &(off, step) in steps {
                let off = off.min(CONTROL);
                seg(pos, off, layer, &mut self.mono, &mut outs);
                pos = off;
                if layer.on || g1 > 0.0 {
                    for hit in def.hits.iter().filter(|x| x.step == step % PATTERN_STEPS) {
                        let i = layer.next % VOICES;
                        layer.next = (i + 1) % VOICES;
                        layer.voices[i].fade_out();
                        layer.voices[i].hit(hit.drum, hit.vel.clamp(0.0, 1.0), 0.12, hit.pitch);
                        layer.pans[i] = hit.pan.clamp(-1.0, 1.0);
                    }
                }
            }
            seg(pos, CONTROL, layer, &mut self.mono, &mut outs);
            // Gain ramps across the block.
            for i in 0..CONTROL {
                let g = (g0 + (g1 - g0) * (i as f32 + 1.0) / CONTROL as f32) * def.gain;
                out_l[i] += outs[0][i] * g;
                out_r[i] += outs[1][i] * g;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_resolve() {
        assert_eq!(overlay_id("drums"), Some(OverlayId(0)));
        assert_eq!(overlay_id(" Drums "), Some(OverlayId(0)));
        assert_eq!(overlay_id("kazoo"), None);
        assert_eq!(overlay_name(OverlayId(0)), Some("drums"));
        assert!(DRUMS.iter().all(|h| h.step < PATTERN_STEPS));
    }

    #[test]
    fn a_layer_plays_on_the_grid_fades_and_goes_quiet() {
        let sr = 48_000.0;
        let mut o = Overlays::new(sr);
        let (mut l, mut r) = ([0.0f32; CONTROL], [0.0f32; CONTROL]);
        // Off: silent, even with steps.
        o.render(&[(0, 0)], &mut l, &mut r);
        assert!(l.iter().chain(&r).all(|x| *x == 0.0));
        o.set(OverlayId(0), true);
        assert_eq!(o.mask(), 1);
        // 16ths at 120 bpm = 6000 samples: step through two bars.
        let mut energy = 0.0f32;
        let mut sample = 0usize;
        let mut step = 0u8;
        for _ in 0..(48_000 * 4 / CONTROL) {
            let (mut l, mut r) = ([0.0f32; CONTROL], [0.0f32; CONTROL]);
            let steps: Vec<(usize, u8)> = (0..CONTROL).filter(|i| (sample + i) % 6000 == 0).map(|i| (i, 0)).collect();
            let steps: Vec<(usize, u8)> = steps
                .into_iter()
                .map(|(i, _)| {
                    let s = step;
                    step = (step + 1) % PATTERN_STEPS;
                    (i, s)
                })
                .collect();
            o.render(&steps, &mut l, &mut r);
            assert!(l.iter().chain(&r).all(|x| x.is_finite() && x.abs() < 4.0));
            energy += l.iter().map(|x| x * x).sum::<f32>();
            sample += CONTROL;
        }
        assert!(energy > 1.0, "audible: {energy}");
        // Off: fades out and the voices ring out to silence (no new hits).
        o.set(OverlayId(0), false);
        let mut tail = 0.0f32;
        for k in 0..(48_000 * 3 / CONTROL) {
            let (mut l, mut r) = ([0.0f32; CONTROL], [0.0f32; CONTROL]);
            o.render(&[(0, (k % 32) as u8)], &mut l, &mut r);
            if k > 48_000 * 2 / CONTROL {
                tail += l.iter().map(|x| x * x).sum::<f32>();
            }
        }
        assert!(tail < 1e-6, "silent after the fade: {tail}");
        assert!(!o.busy());
    }
}
