//! Pure trigger rules: who sees a requested frame, the frequency cap (WCAG 2.3.1: ≤ 3 flashes in any
//! second, whatever the settings), the Reduce-flashes pref, pre-emption and the weight → hold
//! mapping. No ECS here, so it's unit-tested directly.

use super::tuning::ImpactRules;
use bd_core::PlayerId;

/// Settings → Reduce flashes (mirrors `settings::FlashReduction`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Reduce {
    #[default]
    Off,
    Reduced,
    None,
}

/// Why a frame fires: priority + cap class + default style.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum ImpactTrigger {
    /// A move's juice `impact_frame` key.
    Move,
    /// `PowerEvent::Crit` (Black Flash).
    Crit,
    /// A KO blow.
    Ko,
    /// A Signature-tier construct's hit (Hollow Purple landing).
    Signature,
    /// A perfect counter / parry.
    Counter,
    /// A fully charged heavy-weapon hit (WEAPON FEEL).
    HeavyWeapon,
    /// A beam clash detonates.
    BeamClash,
    /// A domain's sure-hit lands.
    DomainSureHit,
    #[default]
    Custom,
}

impl ImpactTrigger {
    pub fn priority(self) -> u8 {
        match self {
            ImpactTrigger::Ko => 6,
            ImpactTrigger::Signature | ImpactTrigger::BeamClash => 5,
            ImpactTrigger::Crit | ImpactTrigger::Move => 4,
            ImpactTrigger::Counter | ImpactTrigger::DomainSureHit => 3,
            ImpactTrigger::HeavyWeapon => 2,
            ImpactTrigger::Custom => 1,
        }
    }
}

/// How the local viewer sees a frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum View {
    /// Attacker or victim: the whole screen.
    Full,
    /// A bystander who can see it: the treatment in a world-anchored bubble around the point.
    Bubble,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Decision {
    Play(View),
    /// Not shown (the reason, for tests / logs).
    Drop(&'static str),
}

/// What the rules need to know about the viewer and the request.
#[derive(Clone, Copy, Debug)]
pub struct Ask {
    pub now: f64,
    pub trigger: ImpactTrigger,
    pub attacker: Option<PlayerId>,
    pub victim: Option<PlayerId>,
    pub local: Option<PlayerId>,
    /// Distance from the camera to the point (m) and whether it is in front of the camera.
    pub dist: f32,
    pub visible: bool,
    pub reduce: Reduce,
}

/// The cap's memory (fixed size: no allocation).
#[derive(Clone, Debug, Default)]
pub struct Cap {
    /// Start times of the last frames shown (ring).
    recent: [f64; 8],
    n: usize,
    head: usize,
    /// Last frame per attacker: (attacker, time) ring.
    by_attacker: [(Option<PlayerId>, f64); 8],
    ba_head: usize,
    /// The frame playing now: (priority, ends at).
    playing: Option<(u8, f64)>,
}

impl Cap {
    fn count_since(&self, t: f64) -> usize {
        (0..self.n).filter(|&i| self.recent[i] > t).count()
    }
    fn last(&self) -> Option<f64> {
        (0..self.n).map(|i| self.recent[i]).fold(None, |a, b| Some(a.map_or(b, |a: f64| a.max(b))))
    }
    fn last_by(&self, id: PlayerId) -> Option<f64> {
        self.by_attacker.iter().filter(|e| e.0 == Some(id)).map(|e| e.1).fold(None, |a, b| Some(a.map_or(b, |a: f64| a.max(b))))
    }

    /// Record a shown frame (starting `now`, playing until `ends`).
    pub fn record(&mut self, now: f64, ends: f64, attacker: Option<PlayerId>, priority: u8) {
        self.recent[self.head] = now;
        self.head = (self.head + 1) % self.recent.len();
        self.n = (self.n + 1).min(self.recent.len());
        if attacker.is_some() {
            self.by_attacker[self.ba_head] = (attacker, now);
            self.ba_head = (self.ba_head + 1) % self.by_attacker.len();
        }
        self.playing = Some((priority, ends));
    }

    /// Frames shown in the 1 s before `now` (inclusive of `now`).
    pub fn in_last_second(&self, now: f64) -> usize {
        self.count_since(now - 1.0)
    }
}

/// Hard ceiling of frames per second regardless of data (WCAG 2.3.1 general flash threshold).
pub const WCAG_MAX_PER_SEC: usize = 3;

/// Decide whether (and how) the local viewer sees a requested frame.
pub fn decide(rules: &ImpactRules, cap: &Cap, ask: &Ask) -> Decision {
    if ask.reduce == Reduce::None {
        return Decision::Drop("reduce flashes: none");
    }
    let involved = ask.local.is_some() && (ask.local == ask.attacker || ask.local == ask.victim);
    let view = if involved {
        View::Full
    } else if ask.visible && ask.dist <= rules.bubble_range {
        View::Bubble
    } else {
        return Decision::Drop("not involved / not in view");
    };
    // Flash cap: never more than 3 (data may lower it; Reduced allows 1 per `reduced_gap`).
    let per_sec = (rules.max_per_sec as usize).clamp(1, WCAG_MAX_PER_SEC);
    if cap.in_last_second(ask.now) >= per_sec {
        return Decision::Drop("cap: flashes per second");
    }
    let gap = if ask.reduce == Reduce::Reduced { rules.reduced_gap.max(rules.min_gap) } else { rules.min_gap };
    if cap.last().is_some_and(|t| ask.now - t < gap as f64) {
        return Decision::Drop("cap: min gap");
    }
    if ask.trigger != ImpactTrigger::Ko
        && let Some(a) = ask.attacker
        && cap.last_by(a).is_some_and(|t| ask.now - t < rules.same_attacker_gap as f64)
    {
        return Decision::Drop("cap: same attacker");
    }
    // A frame still playing is only replaced by a more important one.
    if let Some((p, ends)) = cap.playing
        && ends > ask.now
        && ask.trigger.priority() <= p
    {
        return Decision::Drop("a more important frame is playing");
    }
    Decision::Play(view)
}

/// Hold (s) for a hit `weight` (0..1+): `hold_frames` lerped, × the style's multiplier.
pub fn hold_secs(rules: &ImpactRules, weight: f32, style_mult: f32) -> f32 {
    let w = weight.clamp(0.0, 1.0);
    let (lo, hi) = rules.hold_frames;
    let frames = (lo + (hi - lo) * w).round().clamp(1.0, 8.0);
    frames * rules.frame_secs.clamp(1.0 / 120.0, 0.1) * style_mult.clamp(0.5, 2.0)
}

/// Envelope of the treatment at `t` s into a frame: (strength 0..1, zoom 0..1 of the style's
/// zoom). Full: a hard cut in, held, then a fast ease out during `release`. Reduced: a fade in over
/// `fade_in`, and a slow ease out (no hard luminance steps).
pub fn envelope(t: f32, hold: f32, release: f32, reduced: bool, fade_in: f32) -> (f32, f32) {
    if t < 0.0 {
        return (0.0, 0.0);
    }
    if t < hold {
        let a = if reduced { smooth((t / fade_in.max(1e-3)).min(1.0)) } else { 1.0 };
        // Zoom creeps in across the hold (the camera "leans" into the hit).
        let z = 0.7 + 0.3 * (t / hold.max(1e-3));
        return (a, z);
    }
    let u = ((t - hold) / release.max(1e-3)).clamp(0.0, 1.0);
    if u >= 1.0 {
        return (0.0, 0.0);
    }
    let s = if reduced { 1.0 - smooth(u) } else { (1.0 - u) * (1.0 - u) * (1.0 - u) };
    // The zoom snaps back faster than the tone (the punch out).
    let z = (1.0 - u).powi(4);
    (s, z)
}

fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: PlayerId = PlayerId(1);
    const B: PlayerId = PlayerId(2);
    const C: PlayerId = PlayerId(3);

    fn ask(now: f64, trigger: ImpactTrigger, attacker: PlayerId, local: PlayerId) -> Ask {
        Ask { now, trigger, attacker: Some(attacker), victim: Some(B), local: Some(local), dist: 5.0, visible: true, reduce: Reduce::Off }
    }

    fn play(rules: &ImpactRules, cap: &mut Cap, a: &Ask) -> Decision {
        let d = decide(rules, cap, a);
        if let Decision::Play(_) = d {
            cap.record(a.now, a.now + 0.3, a.attacker, a.trigger.priority());
        }
        d
    }

    #[test]
    fn attacker_and_victim_get_full_screen_bystanders_a_bubble_far_away_nothing() {
        let r = ImpactRules::default();
        let cap = Cap::default();
        assert_eq!(decide(&r, &cap, &ask(0.0, ImpactTrigger::Crit, A, A)), Decision::Play(View::Full));
        assert_eq!(decide(&r, &cap, &ask(0.0, ImpactTrigger::Crit, A, B)), Decision::Play(View::Full));
        assert_eq!(decide(&r, &cap, &ask(0.0, ImpactTrigger::Crit, A, C)), Decision::Play(View::Bubble));
        let mut far = ask(0.0, ImpactTrigger::Crit, A, C);
        far.dist = r.bubble_range + 1.0;
        assert!(matches!(decide(&r, &cap, &far), Decision::Drop(_)));
        let mut behind = ask(0.0, ImpactTrigger::Crit, A, C);
        behind.visible = false;
        assert!(matches!(decide(&r, &cap, &behind), Decision::Drop(_)));
        // A spectator (no local player) sees bubbles.
        let mut spec = ask(0.0, ImpactTrigger::Crit, A, C);
        spec.local = None;
        assert_eq!(decide(&r, &cap, &spec), Decision::Play(View::Bubble));
    }

    #[test]
    fn never_more_than_three_per_second_whatever_the_data_says() {
        let mut r = ImpactRules { max_per_sec: 30, min_gap: 0.0, same_attacker_gap: 0.0, ..Default::default() };
        let mut cap = Cap::default();
        // 60 Hz of KO blows from different attackers for 5 s.
        let mut shown = Vec::new();
        for i in 0..300 {
            let now = i as f64 / 60.0;
            let who = PlayerId((i % 7) as u8 + 1);
            if let Decision::Play(_) = play(&r, &mut cap, &ask(now, ImpactTrigger::Ko, who, who)) {
                shown.push(now);
            }
        }
        for (k, t) in shown.iter().enumerate() {
            let in_window = shown[..=k].iter().filter(|s| **s > t - 1.0).count();
            assert!(in_window <= WCAG_MAX_PER_SEC, "{in_window} flashes in the second before {t}");
        }
        assert!(shown.len() >= 12, "the cap still lets frames through: {}", shown.len());
        // The default data is stricter still (min gap).
        r = ImpactRules::default();
        let mut cap = Cap::default();
        let mut n = 0;
        for i in 0..60 {
            if let Decision::Play(_) = play(&r, &mut cap, &ask(i as f64 / 60.0, ImpactTrigger::Ko, A, A)) {
                n += 1;
            }
        }
        assert!(n <= 3, "{n}");
    }

    #[test]
    fn rapid_hits_from_one_attacker_do_not_strobe() {
        let r = ImpactRules::default();
        let mut cap = Cap::default();
        let mut n = 0;
        // A crit every 100 ms for 2 s from the same attacker.
        for i in 0..20 {
            if let Decision::Play(_) = play(&r, &mut cap, &ask(i as f64 * 0.1, ImpactTrigger::Crit, A, A)) {
                n += 1;
            }
        }
        let max = (2.0 / r.same_attacker_gap).ceil() as usize + 1;
        assert!(n <= max && n >= 2, "{n} frames in 2 s");
        // ...but a KO blow still gets through the per-attacker gap (inside the global cap).
        let mut cap = Cap::default();
        assert!(matches!(play(&r, &mut cap, &ask(0.0, ImpactTrigger::Crit, A, A)), Decision::Play(_)));
        assert!(matches!(play(&r, &mut cap, &ask(0.5, ImpactTrigger::Ko, A, A)), Decision::Play(_)));
    }

    #[test]
    fn reduce_flashes_pref() {
        let r = ImpactRules::default();
        let mut cap = Cap::default();
        let mut a = ask(0.0, ImpactTrigger::Ko, A, A);
        a.reduce = Reduce::None;
        assert!(matches!(decide(&r, &cap, &a), Decision::Drop(_)), "None: never");
        a.reduce = Reduce::Reduced;
        assert!(matches!(play(&r, &mut cap, &a), Decision::Play(_)));
        // Reduced: one per `reduced_gap`.
        let mut b = ask(0.6, ImpactTrigger::Ko, C, C);
        b.reduce = Reduce::Reduced;
        assert!(matches!(decide(&r, &cap, &b), Decision::Drop(_)));
        b.now = r.reduced_gap as f64 + 0.01;
        assert!(matches!(decide(&r, &cap, &b), Decision::Play(_)));
        // Reduced envelope: no hard cut (starts at 0, ramps), slow fade out.
        let (s0, _) = envelope(0.0, 0.15, 0.4, true, 0.1);
        let (s1, _) = envelope(0.12, 0.15, 0.4, true, 0.1);
        assert!(s0 < 0.05 && s1 > 0.95);
        let (full0, _) = envelope(0.0, 0.15, 0.14, false, 0.1);
        assert_eq!(full0, 1.0);
        // Per-frame change of the reduced envelope stays small at 60 fps (no step).
        let mut prev = 0.0;
        for i in 0..60 {
            let (s, _) = envelope(i as f32 / 60.0, 0.15, 0.4, true, 0.1);
            assert!((s - prev).abs() < 0.5, "step {} → {}", prev, s);
            prev = s;
        }
    }

    #[test]
    fn a_higher_priority_frame_preempts_a_lower_one_inside_the_cap() {
        let r = ImpactRules { min_gap: 0.0, same_attacker_gap: 0.0, ..Default::default() };
        let mut cap = Cap::default();
        cap.record(0.0, 1.0, Some(A), ImpactTrigger::HeavyWeapon.priority());
        assert!(matches!(decide(&r, &cap, &ask(0.05, ImpactTrigger::Custom, C, C)), Decision::Drop(_)));
        assert!(matches!(decide(&r, &cap, &ask(0.05, ImpactTrigger::Ko, C, C)), Decision::Play(_)));
    }

    #[test]
    fn weight_maps_to_two_to_five_frames() {
        let r = ImpactRules::default();
        let f = |w: f32| hold_secs(&r, w, 1.0) / r.frame_secs;
        assert!((f(0.0) - 2.0).abs() < 1e-3);
        assert!((f(1.0) - 5.0).abs() < 1e-3);
        assert!((f(9.0) - 5.0).abs() < 1e-3);
        assert!(f(0.5) > 2.9 && f(0.5) < 4.1);
    }

    #[test]
    fn the_envelope_ends() {
        for reduced in [false, true] {
            let (s, z) = envelope(0.15 + 0.41, 0.15, 0.4, reduced, 0.1);
            assert_eq!((s, z), (0.0, 0.0));
        }
    }
}
