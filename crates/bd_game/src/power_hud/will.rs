//! The willpower gauge model (pure, unit-tested): willpower in whole SPARKS
//! (`bd_sim::rules::MatchRules::willpower_segments`; every cast costs whole sparks), the typed
//! phrase's cost bite, sparks breaking off when spent, the glint + tick when one fills, the idle
//! fade, and the ring hand's one-pulse-per-spark. Drawn by [`super::gauge`] (crosshair) and read by
//! the ring hand (`viewmodel`) and the strip's fallback arc — one source ([`super::PowerHudState`]).

use crate::readability::{Bite, GaugeTuning, cost_bite, whole};

/// Sound when a spark fills (a soft hard-light click, pitched up).
pub const SPARK_TICK: &str = "assemble_click";
/// Seconds per ring-hand pulse (one per spark spent) and the gap between them.
pub const PULSE_SECS: f32 = 0.09;
pub const PULSE_GAP: f32 = 0.05;
/// A drop of at least this many sparks in one frame is a cast (it breaks off together).
const CAST_DROP: f32 = 0.35;

/// What happened this frame (sounds).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WillEdges {
    /// A spark finished filling (tick + glint).
    pub filled: bool,
    /// Sparks spent this frame (they break off).
    pub spent: u32,
    /// Reached a full gauge.
    pub full: bool,
}

/// Sparks breaking off the gauge: the range along the gauge and its age 0 → 1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Break {
    pub lo: f32,
    pub hi: f32,
    pub age: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WillGauge {
    /// Sparks in the whole pool (0 = no local player / infinite willpower: nothing to show).
    pub pool: u32,
    /// Willpower per spark.
    pub spark: f32,
    /// Sparks filled (display value).
    pub have: f32,
    /// The typed phrase's bite (sparks).
    pub bite: Bite,
    pub charging: bool,
    /// Under `low_frac` of the pool or one spark: dims and sputters.
    pub low: bool,
    /// The spark that just filled (index) and its glint age 0 → 1.
    pub glint: Option<(u32, f32)>,
    /// The latest sparks breaking off.
    pub brk: Option<Break>,
    /// Full-gauge flash 1 → 0.
    pub full_flash: f32,
    /// Opacity 0..1 of the gauge (idle baseline … active).
    pub alpha: f32,
    /// Seconds since anything changed.
    pub idle: f32,
    /// Seconds since the last spend and how many sparks it was (ring-hand pulses).
    pub since_spent: f32,
    pub spent_n: u32,
    raw: Option<f32>,
}

impl WillGauge {
    /// `wp` = the local player's willpower (`None` = none / dead), `max` = the pool, `spark` =
    /// willpower per spark (0 = infinite willpower), `cost` = the typed phrase's cost (willpower;
    /// the sim's quantised price), `charging` = holding charge.
    #[allow(clippy::too_many_arguments)]
    pub fn update(&mut self, wp: Option<f32>, max: f32, spark: f32, cost: Option<f32>, charging: bool, dt: f32, t: &GaugeTuning) -> WillEdges {
        let mut e = WillEdges::default();
        let dt = if dt.is_finite() { dt.clamp(0.0, 0.25) } else { 0.0 };
        let (Some(wp), true) = (wp.filter(|w| w.is_finite()), spark > 0.0 && spark.is_finite() && max > 0.0) else {
            *self = WillGauge { alpha: (self.alpha - dt * 4.0).max(0.0), ..WillGauge::default() };
            return e;
        };
        self.spark = spark;
        self.pool = (max / spark).round().max(1.0) as u32;
        let pool = self.pool as f32;
        let now = (wp / spark).clamp(0.0, pool);
        let first = self.raw.is_none();
        let before = self.raw.unwrap_or(now);
        self.raw = Some(now);
        if first {
            self.have = now;
        }
        // Spent: a cast drops together; a continuous drain (upkeep, a growing energy ball) breaks
        // each spark off as it empties — chunk by chunk.
        let (wb, wn) = (whole(before), whole(now));
        if now < before - 1e-4 {
            let spent = if before - now >= CAST_DROP { ((before - now) + 0.5).floor().max(1.0) as u32 } else { wb.saturating_sub(wn) };
            if spent > 0 {
                let (lo, hi) = if before - now >= CAST_DROP { (now, before) } else { (wb.saturating_sub(spent) as f32, wb as f32) };
                self.brk = Some(Break { lo, hi, age: 0.0 });
                self.since_spent = 0.0;
                self.spent_n = spent;
                e.spent = spent;
            }
        } else if wn > wb {
            // A spark clicked in (the top one, when several fill at once: respawn, a big refill).
            self.glint = Some((wn - 1, 0.0));
            e.filled = true;
        }
        let full = now >= pool - 1e-3;
        if full && before < pool - 1e-3 {
            self.full_flash = 1.0;
            e.full = true;
        }
        // Display: drains instantly (the break-off shows the spend), refills smoothly.
        self.have = if now < self.have { now } else { self.have + (now - self.have) * (1.0 - (-14.0 * dt).exp()) };
        if (now - self.have).abs() < 1e-3 {
            self.have = now;
        }
        self.bite = cost_bite(now, cost.map_or(0.0, |c| c / spark));
        self.charging = charging;
        self.low = now < (t.low_frac * pool).max(1.0);
        // Animations.
        let brk_rate = 1.0 / t.break_secs.max(0.05);
        if let Some(b) = self.brk.as_mut() {
            b.age += dt * brk_rate;
        }
        if self.brk.is_some_and(|b| b.age >= 1.0) {
            self.brk = None;
        }
        let glint_rate = 1.0 / t.glint_secs.max(0.05);
        if let Some((_, a)) = self.glint.as_mut() {
            *a += dt * glint_rate;
        }
        if self.glint.is_some_and(|(_, a)| a >= 1.0) {
            self.glint = None;
        }
        self.full_flash = (self.full_flash - dt * 2.0).max(0.0);
        self.since_spent += dt;
        // Idle fade: full, still, nothing previewed or charging → the subtle baseline.
        let changing = (now - before).abs() > 1e-4 || self.bite.cost > 0.0 || charging || self.brk.is_some() || self.glint.is_some();
        self.idle = if changing || !full { 0.0 } else { self.idle + dt };
        let want = if self.idle > t.idle_after { t.idle_alpha } else { t.active_alpha };
        let k = 1.0 - (-dt * 3.0 / t.fade_secs.max(0.01)).exp();
        // Coming in is quick (the change is the news); settling is the slow part.
        self.alpha += (want - self.alpha) * if want > self.alpha { (k * 2.5).min(1.0) } else { k };
        e
    }

    /// The ring hand's pulse 0..1 right now: one short pulse per spark just spent.
    pub fn ring_pulse(&self) -> f32 {
        pulse_at(self.since_spent, self.spent_n)
    }
}

/// `n` pulses of [`PULSE_SECS`] separated by [`PULSE_GAP`], `t` seconds after the spend.
pub fn pulse_at(t: f32, n: u32) -> f32 {
    if n == 0 || !(t >= 0.0) {
        return 0.0;
    }
    let period = PULSE_SECS + PULSE_GAP;
    let k = (t / period).floor();
    if k >= n.min(12) as f32 {
        return 0.0;
    }
    let x = (t - k * period) / PULSE_SECS;
    if x >= 1.0 { 0.0 } else { (x * std::f32::consts::PI).sin() }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    fn gauge() -> (WillGauge, GaugeTuning) {
        (WillGauge::default(), GaugeTuning::default())
    }

    #[test]
    fn sparks_count_and_bite() {
        let (mut g, t) = gauge();
        // 8 sparks of 12.5, 62.5 willpower = 5 sparks; a 2-spark phrase is affordable.
        g.update(Some(62.5), 100.0, 12.5, Some(25.0), false, DT, &t);
        assert_eq!(g.pool, 8);
        assert!((g.have - 5.0).abs() < 1e-4);
        assert!(g.bite.afford && g.bite.cost == 2.0 && g.bite.lo == 3.0 && g.bite.hi == 5.0);
        // A 7-spark phrase is not: 2 missing, drawn as ghosts.
        g.update(Some(62.5), 100.0, 12.5, Some(87.5), false, DT, &t);
        assert!(!g.bite.afford && (g.bite.missing - 2.0).abs() < 1e-4);
        // Double willpower: twice the sparks, same size.
        g.update(Some(150.0), 200.0, 12.5, None, false, DT, &t);
        assert_eq!(g.pool, 16);
        assert_eq!(g.bite, Bite::default());
        // Infinite willpower (no spark size) / dead: nothing.
        g.update(Some(50.0), 100.0, 0.0, None, false, DT, &t);
        assert_eq!(g.pool, 0);
        g.update(None, 100.0, 12.5, None, false, DT, &t);
        assert_eq!(g.pool, 0);
    }

    #[test]
    fn a_cast_breaks_its_sparks_off_and_pulses_the_ring_once_each() {
        let (mut g, t) = gauge();
        g.update(Some(75.0), 100.0, 12.5, None, false, DT, &t);
        // A 3-spark cast: 6 → 3.
        let e = g.update(Some(37.5), 100.0, 12.5, None, false, DT, &t);
        assert_eq!(e.spent, 3);
        let b = g.brk.expect("break-off");
        assert!((b.lo - 3.0).abs() < 1e-4 && (b.hi - 6.0).abs() < 1e-4, "{b:?}");
        assert!((g.have - 3.0).abs() < 1e-4, "the gauge drains instantly");
        // Quick: gone within break_secs.
        for _ in 0..((t.break_secs / DT) as usize + 2) {
            g.update(Some(37.5), 100.0, 12.5, None, false, DT, &t);
        }
        assert!(g.brk.is_none());
        // Three ring pulses, then quiet.
        let mut peaks = 0;
        let mut was = 0.0;
        for i in 0..60 {
            let p = pulse_at(i as f32 * 0.01, 3);
            if p > 0.5 && was <= 0.5 {
                peaks += 1;
            }
            was = p;
        }
        assert_eq!(peaks, 3);
        assert_eq!(pulse_at(10.0, 3), 0.0);
        assert_eq!(pulse_at(0.02, 0), 0.0);
    }

    #[test]
    fn a_continuous_drain_breaks_sparks_one_by_one_and_a_refill_clicks() {
        let (mut g, t) = gauge();
        let mut wp = 40.0; // 3.2 sparks
        g.update(Some(wp), 100.0, 12.5, None, false, DT, &t);
        let mut spent = 0;
        for _ in 0..240 {
            wp -= 4.0 * DT; // an energy ball growing: 4 willpower/s
            spent += g.update(Some(wp), 100.0, 12.5, None, false, DT, &t).spent;
        }
        // 16 willpower drained from 40: crossed the 3-spark and 2-spark boundaries.
        assert_eq!(spent, 2, "40 → 24 (3.2 → 1.92 sparks): the 3rd and 2nd sparks broke off one by one");
        let (mut g, t) = gauge();
        let mut wp = 24.0;
        let mut ticks = 0;
        g.update(Some(wp), 100.0, 12.5, None, false, DT, &t);
        for _ in 0..600 {
            wp += 3.0 * DT; // regen
            let e = g.update(Some(wp), 100.0, 12.5, None, false, DT, &t);
            ticks += e.filled as u32;
        }
        // 24 → 54: sparks 2, 3 and 4 click in (at 25, 37.5, 50).
        assert_eq!(ticks, 3);
        assert!(g.glint.is_none() || g.glint.unwrap().0 == 3);
    }

    #[test]
    fn fades_to_baseline_when_full_and_idle() {
        let (mut g, t) = gauge();
        for _ in 0..30 {
            g.update(Some(90.0), 100.0, 12.5, None, false, DT, &t);
        }
        assert!(g.alpha > t.active_alpha * 0.9, "not full: shown");
        let e = g.update(Some(100.0), 100.0, 12.5, None, false, DT, &t);
        assert!(e.full && g.full_flash > 0.9);
        for _ in 0..((t.idle_after + 3.0) / DT) as usize {
            g.update(Some(100.0), 100.0, 12.5, None, false, DT, &t);
        }
        assert!((g.alpha - t.idle_alpha).abs() < 0.05, "{}", g.alpha);
        // Typing a phrase brings it back.
        for _ in 0..10 {
            g.update(Some(100.0), 100.0, 12.5, Some(25.0), false, DT, &t);
        }
        assert!(g.alpha > 0.7);
        // Low: under one spark (or low_frac) it sputters.
        g.update(Some(10.0), 100.0, 12.5, None, false, DT, &t);
        assert!(g.low);
        g.update(Some(40.0), 100.0, 12.5, None, false, DT, &t);
        assert!(!g.low);
    }
}
