//! Pure derivation of [`PowerHud`] from replicated sim state (`PowerSnap` + `ReadySnap` + the
//! recipient's `OwnHud`), plus the auto-derived look of every power (colour + glyph + chime pitch).
//! No Bevy systems here: unit-tested directly.

use bd_core::mechanics::Element;
use bd_sim::pickups::GateKind;
use bd_sim::powers::{power_flags, PowerDef, PowerKind, PowerSnap, PowerTable};
use bd_sim::readiness::{OwnHud, ReadySnap, HELD_MAX, HUD_TIMERS};

/// Display slots (stable per power while it is held).
pub const SLOTS: usize = HELD_MAX;

/// Glyph ids (the `power_icon.wgsl` SDF set). Colour always comes paired with a shape.
pub mod glyph {
    pub const CIRCLE: u8 = 0;
    pub const DIAMOND: u8 = 1;
    pub const TRIANGLE: u8 = 2;
    pub const STAR4: u8 = 3;
    pub const HEXAGON: u8 = 4;
    pub const CLAW: u8 = 5;
    pub const CRESCENT: u8 = 6;
    pub const CROSS: u8 = 7;
    pub const CHEVRON_UP: u8 = 8;
    pub const CHEVRON_DOWN: u8 = 9;
    pub const DROP: u8 = 10;
    pub const BOLT: u8 = 11;
    pub const RING: u8 = 12;
    pub const X: u8 = 13;
    /// Fallback order when a player's held powers would share a glyph.
    pub const SPARE: [u8; 6] = [HEXAGON, CROSS, CRESCENT, RING, BOLT, X];
}

/// E-minor pentatonic ratios above the chime's root (E): E G A B D E' — the hard-light key, so a
/// player's powers ring as a little chord and are learned by ear.
pub const CHIME_STEPS: [f32; 6] = [1.0, 1.189_207, 1.334_84, 1.498_307, 1.781_797, 2.0];

/// How a held power looks and sounds: derived from its existing data, no authoring.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    /// Identity colour (linear RGB, vivid): stable across its tiers.
    pub color: [f32; 3],
    /// Form colour (the Ascend aura of its current tier / transform; = `color` without one).
    pub form_color: [f32; 3],
    pub glyph: u8,
    /// Ready-chime playback rate (a pentatonic step).
    pub pitch: f32,
}

impl Default for Look {
    fn default() -> Self {
        Self { color: [1.0; 3], form_color: [1.0; 3], glyph: glyph::CIRCLE, pitch: 1.0 }
    }
}

fn srgb_to_linear(c: [f32; 3]) -> [f32; 3] {
    c.map(|x| {
        let x = x.clamp(0.0, 1.0);
        if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }
    })
}

/// Lift a colour so even Earth brown / Shadow purple read on a dark HUD: max channel 1, a touch
/// of white.
pub fn vivid(c: [f32; 3]) -> [f32; 3] {
    let m = c[0].max(c[1]).max(c[2]).max(1e-3);
    c.map(|x| (x / m) * 0.85 + 0.15)
}

fn hue_rgb(h: f32) -> [f32; 3] {
    // HSV(h, 0.75, 1) → RGB.
    let h = h.rem_euclid(1.0) * 6.0;
    let f = h.fract();
    let (s, v) = (0.75, 1.0);
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    match h as u32 {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}

fn fnv(s: &str) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in s.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

fn element_rgb(e: Element) -> [f32; 3] {
    bd_fx::palette::element_color(e)
}

/// A power's identity colour: its graft element, else its own aura (Ascend), else its
/// transform's aura, else its first tier's, else its pickup's element, else a hue from its id.
pub fn identity_color(def: &PowerDef) -> [f32; 3] {
    let from_element = def.graft.as_ref().and_then(|g| g.element).filter(|e| *e != Element::Light).map(element_rgb);
    let from_ascend = def
        .ascend
        .as_ref()
        .and_then(|a| a.color)
        .or_else(|| def.transform.as_ref().and_then(|t| t.ascend.as_ref()).and_then(|a| a.color))
        .or_else(|| def.tiers.iter().find_map(|t| t.ascend.as_ref().and_then(|a| a.color)))
        .map(srgb_to_linear);
    let from_pickup = def.pickup.as_ref().map(|p| p.element).filter(|e| *e != Element::Light).map(element_rgb);
    vivid(from_element.or(from_ascend).or(from_pickup).unwrap_or_else(|| hue_rgb(fnv(&def.id) as f32 / u32::MAX as f32)))
}

/// The aura colour of the form `def` is in at `tier` (transformed or not).
pub fn form_color(def: &PowerDef, tier: u8, transformed: bool, identity: [f32; 3]) -> [f32; 3] {
    let tf = transformed.then(|| def.transform.as_ref().and_then(|t| t.ascend.as_ref()).and_then(|a| a.color)).flatten();
    tf.or_else(|| def.ascend_at(tier).and_then(|a| a.color)).map(|c| vivid(srgb_to_linear(c))).unwrap_or(identity)
}

/// The base glyph of a power: by what KIND of thing it is (never its name).
pub fn kind_glyph(def: &PowerDef) -> u8 {
    match def.kind {
        PowerKind::Paramecia => glyph::CIRCLE,
        PowerKind::Logia => glyph::DROP,
        PowerKind::Zoan | PowerKind::MythicalZoan => glyph::CLAW,
        PowerKind::Stand => glyph::STAR4,
        PowerKind::Origin => glyph::DIAMOND,
        PowerKind::Technique => glyph::TRIANGLE,
        // never held (a kit's moves are one-shots), but keep the match total
        PowerKind::Kit => glyph::TRIANGLE,
    }
}

/// Look of power `id` (at `tier`), avoiding the glyphs / pitches already used by the player's
/// other held powers (`taken_glyphs`, `taken_steps` bitmasks).
pub fn power_look(id: u16, tier: u8, transformed: bool, taken_glyphs: u32, taken_steps: u32) -> Look {
    let Some(def) = PowerTable::get().def(id) else { return Look::default() };
    let color = identity_color(def);
    let mut g = kind_glyph(def);
    if taken_glyphs & (1 << g) != 0 {
        g = glyph::SPARE.iter().copied().find(|s| taken_glyphs & (1 << s) == 0).unwrap_or(g);
    }
    let mut step = (fnv(&def.id) % CHIME_STEPS.len() as u32) as usize;
    for _ in 0..CHIME_STEPS.len() {
        if taken_steps & (1 << step) == 0 {
            break;
        }
        step = (step + 1) % CHIME_STEPS.len();
    }
    Look { color, form_color: form_color(def, tier, transformed, color), glyph: g, pitch: CHIME_STEPS[step] }
}

/// Index of `pitch` in [`CHIME_STEPS`].
fn step_of(pitch: f32) -> usize {
    CHIME_STEPS.iter().position(|s| (s - pitch).abs() < 1e-4).unwrap_or(0)
}

/// Look of a map pickup (the carrier's glow colour + a glyph): Giant = a warm egg, Awakening = an
/// aura ring, Ultimate = a crackling star. (Shared with the pickup presenter, when it lands.)
pub fn pickup_look(kind: GateKind) -> ([f32; 3], u8) {
    match kind {
        GateKind::Giant => ([1.0, 0.36, 0.22], glyph::HEXAGON),
        GateKind::Awakening => ([1.0, 0.86, 0.35], glyph::RING),
        GateKind::Ultimate => ([0.72, 0.5, 1.0], glyph::STAR4),
    }
}

/// Status pips: (colour, glyph) per [`OwnHud`] timer slot (StatusKind order, guard, size).
/// `grown` picks the size pip's direction.
pub fn status_look(k: usize, grown: bool) -> ([f32; 3], u8) {
    use bd_sim::readiness::{GUARD_TIMER, SIZE_TIMER};
    match k {
        0 => ([1.0, 0.45, 0.1], glyph::TRIANGLE),   // Burn
        1 => ([0.5, 1.0, 0.25], glyph::X),          // Poison
        2 => ([0.55, 0.85, 1.0], glyph::STAR4),     // Chill
        3 => ([1.0, 0.92, 0.2], glyph::BOLT),       // Stun
        4 => ([0.78, 0.55, 1.0], glyph::CRESCENT),  // Float
        5 => ([0.6, 0.42, 0.85], glyph::RING),      // Shadow
        6 => ([0.3, 0.6, 1.0], glyph::DROP),        // Wet
        GUARD_TIMER => ([0.95, 0.97, 1.0], glyph::HEXAGON),
        SIZE_TIMER if grown => ([1.0, 0.72, 0.3], glyph::CHEVRON_UP),
        SIZE_TIMER => ([0.4, 1.0, 0.85], glyph::CHEVRON_DOWN),
        _ => ([1.0; 3], glyph::CIRCLE),
    }
}

/// One held power in a stable display slot.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Gem {
    pub power: Option<u16>,
    pub look: Look,
    pub tier: u8,
    pub transformed: bool,
    /// What G fires (the active power).
    pub active: bool,
    /// Replicated readiness 0 (just fired) … 1 (ready).
    pub ready: f32,
    /// Smoothed readiness for drawing (drains instantly, refills smoothly).
    pub shown: f32,
    pub is_ready: bool,
    /// Ready / gained flare 1 → 0.
    pub flash: f32,
    /// Form time left 0..1 (`None` = no form).
    pub form: Option<f32>,
}

/// What happened this update (sounds, pops).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Edges {
    /// Display slots whose power came back off cooldown.
    pub ready: u8,
    /// Display slots that gained a power.
    pub gained: u8,
    /// Went from holding nothing to holding something (the awakening pop).
    pub appeared: bool,
}

/// The one derived HUD state: the ring gems, the motes' local copy, the HUD strip and (later) the
/// wordless-affordance ring gauge all read this. No names, no numbers.
#[derive(Clone, Debug, PartialEq)]
pub struct PowerHud {
    pub gems: [Gem; SLOTS],
    pub count: usize,
    /// Powers switched off (Water drain / seastone).
    pub drained: bool,
    /// Willpower 0..1 (replicated value / pool).
    pub willpower: f32,
    /// Cost of the phrase being typed, as a fraction of the pool (`None` = nothing typed / unknown).
    pub cost: Option<f32>,
    /// The map pickup you carry and its life left 0..1.
    pub pickup: Option<(GateKind, f32)>,
    /// Seconds left per [`OwnHud`] timer and the fraction of the longest seen since it started.
    pub timers: [f32; HUD_TIMERS],
    pub timer_frac: [f32; HUD_TIMERS],
    peaks: [f32; HUD_TIMERS],
    /// Shrink vs grow (the size pip's direction).
    pub grown: bool,
    /// Strip pop 1 → 0 after appearing.
    pub pop: f32,
    /// READABILITY: willpower in whole sparks (the crosshair gauge, the ring hand, the strip arc).
    pub will: super::will::WillGauge,
}

impl Default for PowerHud {
    fn default() -> Self {
        Self {
            gems: [Gem::default(); SLOTS],
            count: 0,
            drained: false,
            willpower: 0.0,
            cost: None,
            pickup: None,
            timers: [0.0; HUD_TIMERS],
            timer_frac: [0.0; HUD_TIMERS],
            peaks: [0.0; HUD_TIMERS],
            grown: false,
            pop: 0.0,
            will: Default::default(),
        }
    }
}

/// Seconds a ready flare lasts.
pub const FLASH_SECS: f32 = 0.45;
/// Seconds the strip's appear pop lasts.
pub const POP_SECS: f32 = 0.5;

/// A held power with its replicated readiness (index into `ReadySnap`).
fn held_list(ps: &PowerSnap) -> [Option<(u16, u8, bool, usize)>; SLOTS] {
    let mut out = [None; SLOTS];
    for (k, (id, tier, tf)) in ps.held().enumerate().take(SLOTS) {
        out[k] = Some((id, tier, tf, k));
    }
    out
}

impl PowerHud {
    /// Any timer (status / guard / size) running.
    pub fn any_timer(&self) -> bool {
        self.timers.iter().any(|t| *t > 0.0)
    }

    /// Update from the local player's replicated state (`None` = no local player / dead: everything
    /// clears). `pool` = the willpower maximum. Call once per frame.
    pub fn update(&mut self, me: Option<(&PowerSnap, &ReadySnap, f32)>, pool: f32, own: Option<&OwnHud>, size: f32, dt: f32) -> Edges {
        let mut edges = Edges::default();
        let dt = if dt.is_finite() { dt.clamp(0.0, 0.25) } else { 0.0 };
        let before = self.count;
        let Some((ps, rs, wp)) = me else {
            for g in &mut self.gems {
                *g = Gem::default();
            }
            self.count = 0;
            self.drained = false;
            self.timers = [0.0; HUD_TIMERS];
            self.timer_frac = [0.0; HUD_TIMERS];
            self.peaks = [0.0; HUD_TIMERS];
            self.pop = 0.0;
            return edges;
        };
        self.willpower = if pool > 0.0 && wp.is_finite() { (wp / pool).clamp(0.0, 1.0) } else { 0.0 };
        self.drained = ps.has(power_flags::DRAINED) || ps.has(power_flags::SEASTONE);
        let held = held_list(ps);
        // Drop gems whose power is gone; keep the rest where they are.
        for g in &mut self.gems {
            if let Some(p) = g.power
                && !held.iter().flatten().any(|h| h.0 == p)
            {
                *g = Gem::default();
            }
        }
        // New powers take the first free slot (their look avoids the others' glyph / chime).
        for h in held.iter().flatten() {
            if self.gems.iter().any(|g| g.power == Some(h.0)) {
                continue;
            }
            let (mut tg, mut ts) = (0u32, 0u32);
            for g in self.gems.iter().filter(|g| g.power.is_some()) {
                tg |= 1 << g.look.glyph;
                ts |= 1 << step_of(g.look.pitch);
            }
            if let Some((k, g)) = self.gems.iter_mut().enumerate().find(|(_, g)| g.power.is_none()) {
                *g = Gem { power: Some(h.0), look: power_look(h.0, h.1, h.2, tg, ts), flash: 1.0, ready: 1.0, shown: 1.0, is_ready: true, ..Gem::default() };
                edges.gained |= 1 << k;
            }
        }
        // Readiness, forms, flares.
        let mut count = 0;
        for (k, g) in self.gems.iter_mut().enumerate() {
            g.flash = (g.flash - dt / FLASH_SECS).max(0.0);
            let Some(p) = g.power else { continue };
            let Some(&(_, tier, tf, ri)) = held.iter().flatten().find(|h| h.0 == p) else { continue };
            count += 1;
            if tier != g.tier || tf != g.transformed {
                g.tier = tier;
                g.transformed = tf;
                if let Some(def) = PowerTable::get().def(p) {
                    g.look.form_color = form_color(def, tier, tf, g.look.color);
                }
            }
            g.active = ri == 0;
            let ready = rs.ready(ri);
            let now_ready = rs.is_ready(ri);
            if now_ready && !g.is_ready && edges.gained & (1 << k) == 0 {
                g.flash = 1.0;
                edges.ready |= 1 << k;
            }
            g.is_ready = now_ready;
            g.ready = ready;
            // Drain instantly (it fired); refill smoothly between snapshots.
            g.shown = if ready < g.shown { ready } else { g.shown + (ready - g.shown) * (1.0 - (-18.0 * dt).exp()) };
            if now_ready {
                g.shown = 1.0;
            }
            g.form = rs.form(ri);
        }
        self.count = count;
        if before == 0 && count > 0 {
            edges.appeared = true;
            self.pop = 1.0;
        } else {
            self.pop = (self.pop - dt / POP_SECS).max(0.0);
        }
        // Timers: fraction of the longest seen since each started (a refresh restarts it).
        for k in 0..HUD_TIMERS {
            let s = own.map_or(0.0, |o| o.secs(k));
            if s <= 0.0 {
                self.peaks[k] = 0.0;
                self.timer_frac[k] = 0.0;
            } else {
                if s > self.timers[k] + 0.05 || self.peaks[k] <= 0.0 {
                    self.peaks[k] = s;
                }
                self.timer_frac[k] = (s / self.peaks[k]).clamp(0.0, 1.0);
            }
            self.timers[k] = s;
        }
        self.grown = size > 1.0;
        edges
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bd_sim::readiness::{GUARD_TIMER, SIZE_TIMER};

    fn pid(key: &str) -> u16 {
        PowerTable::get().index_of(key).unwrap_or_else(|| panic!("no power {key}"))
    }

    /// PowerSnap holding `ids` (the first is active).
    fn snap(ids: &[u16]) -> PowerSnap {
        let mut s = PowerSnap::default();
        if let Some(a) = ids.first() {
            s.id = *a + 1;
        }
        for (k, o) in ids.iter().skip(1).enumerate() {
            s.others[k] = *o + 1;
        }
        s
    }

    #[test]
    fn cooldown_fraction_and_ready_edge() {
        let (a, b) = (pid("star_platinum"), pid("mera_mera"));
        let ps = snap(&[a, b]);
        let mut hud = PowerHud::default();
        let e = hud.update(Some((&ps, &ReadySnap::default(), 50.0)), 100.0, None, 1.0, 1.0 / 60.0);
        assert!(e.appeared, "first power pops the strip");
        assert_eq!(e.gained, 0b11);
        assert_eq!(hud.count, 2);
        assert!((hud.willpower - 0.5).abs() < 1e-6);
        assert!(hud.gems[0].active && !hud.gems[1].active);
        // Fire the active power's move: 3/4 of its cooldown left.
        let mut rs = ReadySnap::default();
        rs.cd[0] = 191;
        let e = hud.update(Some((&ps, &rs, 45.0)), 100.0, None, 1.0, 1.0 / 60.0);
        assert_eq!(e.ready, 0);
        assert!(!hud.gems[0].is_ready && hud.gems[1].is_ready);
        assert!((hud.gems[0].ready - (1.0 - 191.0 / 255.0)).abs() < 1e-6, "cooldown fraction");
        assert!((hud.gems[0].shown - hud.gems[0].ready).abs() < 1e-6, "drains instantly");
        // Refilling: smooth, no edge.
        rs.cd[0] = 20;
        let e = hud.update(Some((&ps, &rs, 45.0)), 100.0, None, 1.0, 1.0 / 60.0);
        assert_eq!(e.ready, 0);
        assert!(hud.gems[0].shown > 1.0 - 191.0 / 255.0 && hud.gems[0].shown < hud.gems[0].ready);
        // Back: ONE ready edge + a flare, then none.
        rs.cd[0] = 0;
        let e = hud.update(Some((&ps, &rs, 45.0)), 100.0, None, 1.0, 1.0 / 60.0);
        assert_eq!(e.ready, 0b01);
        assert!(hud.gems[0].flash > 0.9 && hud.gems[0].shown == 1.0);
        let e = hud.update(Some((&ps, &rs, 45.0)), 100.0, None, 1.0, 1.0 / 60.0);
        assert_eq!(e.ready, 0);
        // The flare fades.
        for _ in 0..60 {
            hud.update(Some((&ps, &rs, 45.0)), 100.0, None, 1.0, 1.0 / 60.0);
        }
        assert_eq!(hud.gems[0].flash, 0.0);
    }

    #[test]
    fn form_duration_fraction() {
        let a = pid("star_platinum");
        let ps = snap(&[a]);
        let mut hud = PowerHud::default();
        let mut rs = ReadySnap::default();
        rs.form[0] = 255;
        hud.update(Some((&ps, &rs, 50.0)), 100.0, None, 1.0, 0.016);
        assert_eq!(hud.gems[0].form, Some(1.0));
        rs.form[0] = 64;
        hud.update(Some((&ps, &rs, 50.0)), 100.0, None, 1.0, 0.016);
        assert!((hud.gems[0].form.unwrap() - 64.0 / 255.0).abs() < 1e-6);
        rs.form[0] = 0;
        hud.update(Some((&ps, &rs, 50.0)), 100.0, None, 1.0, 0.016);
        assert_eq!(hud.gems[0].form, None);
    }

    #[test]
    fn slots_are_stable_when_the_active_power_changes() {
        let (a, b, c) = (pid("star_platinum"), pid("mera_mera"), pid("gomu_gomu"));
        let mut hud = PowerHud::default();
        let rs = ReadySnap::default();
        hud.update(Some((&snap(&[a, b, c]), &rs, 50.0)), 100.0, None, 1.0, 0.016);
        let before: Vec<_> = hud.gems.iter().map(|g| (g.power, g.look)).collect();
        // Cycle: b is active now, a moved to the back.
        let e = hud.update(Some((&snap(&[b, c, a]), &rs, 50.0)), 100.0, None, 1.0, 0.016);
        assert_eq!(e.gained, 0);
        let after: Vec<_> = hud.gems.iter().map(|g| (g.power, g.look)).collect();
        assert_eq!(before, after, "gems never jump around");
        assert!(hud.gems[1].active && !hud.gems[0].active);
        // Distinct glyphs and chimes among held powers.
        let g: Vec<u8> = hud.gems.iter().map(|g| g.look.glyph).collect();
        assert!(g[0] != g[1] && g[1] != g[2] && g[0] != g[2], "{g:?}");
        let p: Vec<usize> = hud.gems.iter().map(|g| step_of(g.look.pitch)).collect();
        assert!(p[0] != p[1] && p[1] != p[2] && p[0] != p[2], "{p:?}");
        // Losing one frees its slot; the others stay.
        hud.update(Some((&snap(&[b, a]), &rs, 50.0)), 100.0, None, 1.0, 0.016);
        assert_eq!(hud.count, 2);
        assert_eq!(hud.gems[2].power, None);
        assert_eq!(hud.gems[0].power, Some(a));
        // Death clears everything; the next power pops again.
        hud.update(None, 100.0, None, 1.0, 0.016);
        assert_eq!(hud.count, 0);
        let e = hud.update(Some((&snap(&[c]), &rs, 50.0)), 100.0, None, 1.0, 0.016);
        assert!(e.appeared && hud.pop == 1.0);
    }

    #[test]
    fn status_timers_sweep_from_their_start() {
        let mut hud = PowerHud::default();
        let ps = PowerSnap::default();
        let rs = ReadySnap::default();
        let mut own = OwnHud::default();
        own.timers[0] = 40; // burning, 4 s
        own.timers[SIZE_TIMER] = 60;
        hud.update(Some((&ps, &rs, 50.0)), 100.0, Some(&own), 0.5, 0.016);
        assert_eq!(hud.count, 0);
        assert!(hud.any_timer() && !hud.grown);
        assert_eq!(hud.timer_frac[0], 1.0);
        own.timers[0] = 10;
        hud.update(Some((&ps, &rs, 50.0)), 100.0, Some(&own), 0.5, 0.016);
        assert!((hud.timer_frac[0] - 0.25).abs() < 1e-5);
        // Refreshed: full again.
        own.timers[0] = 25;
        hud.update(Some((&ps, &rs, 50.0)), 100.0, Some(&own), 0.5, 0.016);
        assert_eq!(hud.timer_frac[0], 1.0);
        own.timers[0] = 0;
        hud.update(Some((&ps, &rs, 50.0)), 100.0, Some(&own), 0.5, 0.016);
        assert_eq!(hud.timer_frac[0], 0.0);
        assert!(hud.timers[GUARD_TIMER] == 0.0);
    }

    #[test]
    fn every_power_gets_a_vivid_colour_and_a_glyph() {
        let t = PowerTable::get();
        assert!(!t.powers.is_empty());
        for (i, d) in t.powers.iter().enumerate() {
            let l = power_look(i as u16, 0, false, 0, 0);
            assert!(l.color.iter().all(|c| c.is_finite() && (0.0..=1.0).contains(c)), "{}", d.id);
            assert!(l.color.iter().cloned().fold(0.0, f32::max) > 0.95, "{} is dull", d.id);
            assert!(l.glyph <= glyph::X);
            assert!(CHIME_STEPS.contains(&l.pitch));
        }
        // Pickups and statuses pair colour with distinct shapes.
        let g: Vec<u8> = GateKind::ALL.iter().map(|k| pickup_look(*k).1).collect();
        assert!(g[0] != g[1] && g[1] != g[2] && g[0] != g[2]);
        let s: Vec<u8> = (0..HUD_TIMERS).map(|k| status_look(k, false).1).collect();
        for i in 0..s.len() {
            for j in i + 1..s.len() {
                assert_ne!(s[i], s[j], "status pips {i} and {j} share a glyph");
            }
        }
        assert_ne!(status_look(SIZE_TIMER, true).1, status_look(SIZE_TIMER, false).1);
    }
}
