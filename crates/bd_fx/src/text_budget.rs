//! World-text budget & placement policy for comic words ([`crate::SpawnOnomatopoeia`]) and move
//! titles / shouts ([`crate::Shout`]). Tunables: `assets/fx/text_budget.ron` (compiled in).
//!
//! Owner rule: presentation stays dramatic (titles and comic words are part of the charm) but nobody
//! ever loses sight of the action or their crosshair — the game is fast and competitive.
//!
//! * **Placement** ([`place_word`]): a word never sits closer than `min_cam_dist` to the camera (it is
//!   pushed out along its view ray, so it keeps its spot on screen); its letters are clamped to
//!   `min_letter_h..max_letter_h` of the screen height and the word to `max_word_w` of the width; it
//!   never overlaps the protected crosshair cone (others' words go up, yours to the side); words
//!   you are involved in are smaller (`own_scale`), peripheral (`own_min_deg`) and brief (`own_hold`).
//! * **Budget** ([`priority`], [`pick_eviction`], [`LightGate`]): each word has a priority (impact +
//!   involvement + accent); at most `max_near_*` words live near the camera — a new word replaces the
//!   least important one (newer wins ties) or is skipped; rapid light hits from one source say a
//!   word only every `light_every`-th hit (crits / finishers / KOs always do).
//! * **Titles** ([`stack_offsets`]): a caster's titles stack in one short column (newest on the base
//!   line, older ones shrunk above it, at most `stack_max`) instead of overlapping.
//! * **Word choice** ([`TextBudget::hit_word`]): a hit's word by form / weapon class / element / weight
//!   (SHING for blades, THOOM for heavies, KRAK for lightning…).

use std::collections::HashMap;

use bevy::prelude::*;
use serde::Deserialize;

const DEFAULT_SRC: &str = include_str!("../../../assets/fx/text_budget.ron");

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Placement {
    pub min_cam_dist: f32,
    pub skip_cam_dist: f32,
    pub crosshair_deg: f32,
    pub margin_deg: f32,
    pub max_letter_h: f32,
    pub min_letter_h: f32,
    pub max_word_w: f32,
    pub own_scale: f32,
    pub own_hold: f32,
    pub own_min_deg: f32,
    /// Own BIG hits (heavy / crit / finisher): this size multiplier instead of `own_scale`, and the
    /// full hold — still peripheral.
    pub own_big_scale: f32,
}

impl Default for Placement {
    fn default() -> Self {
        Placement {
            min_cam_dist: 3.2,
            skip_cam_dist: 0.9,
            crosshair_deg: 7.0,
            margin_deg: 1.5,
            max_letter_h: 0.075,
            min_letter_h: 0.02,
            max_word_w: 0.3,
            own_scale: 0.6,
            own_hold: 0.6,
            own_min_deg: 11.0,
            own_big_scale: 0.85,
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Budget {
    pub near_cam_m: f32,
    pub max_near_full: usize,
    pub max_near_reduced: usize,
    pub cluster_m: f32,
    pub max_cluster: usize,
    pub light_impact: f32,
    pub light_every: u32,
    pub light_reset: f32,
    pub local_bonus: f32,
    pub accent_bonus: f32,
}

impl Default for Budget {
    fn default() -> Self {
        Budget {
            near_cam_m: 18.0,
            max_near_full: 3,
            max_near_reduced: 2,
            cluster_m: 4.0,
            max_cluster: 2,
            light_impact: 0.8,
            light_every: 3,
            light_reset: 1.0,
            local_bonus: 0.4,
            accent_bonus: 2.0,
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Titles {
    pub stack_max: usize,
    pub stack_shrink: f32,
    pub stack_hold: f32,
    pub stack_gap: f32,
    pub same_caster_m: f32,
    pub clear_crosshair: bool,
}

impl Default for Titles {
    fn default() -> Self {
        Titles { stack_max: 2, stack_shrink: 0.7, stack_hold: 0.35, stack_gap: 0.15, same_caster_m: 1.6, clear_crosshair: true }
    }
}

/// One `hit_words` rule (empty / `None` fields match anything).
#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct HitWordRule {
    pub form: String,
    pub class: String,
    pub element: String,
    pub heavy: Option<bool>,
    pub key: String,
}

/// The policy (`assets/fx/text_budget.ron`).
#[derive(Resource, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct TextBudget {
    pub placement: Placement,
    pub budget: Budget,
    pub titles: Titles,
    pub hit_words: Vec<HitWordRule>,
}

impl TextBudget {
    pub fn parse(src: &str) -> Result<Self, String> {
        ron::from_str(src).map_err(|e| e.to_string())
    }

    /// The shipped policy (falls back to the code defaults on a parse error).
    pub fn shipped() -> Self {
        Self::parse(DEFAULT_SRC).unwrap_or_else(|e| {
            warn!("bd_fx: text_budget.ron parse error: {e}");
            TextBudget::default()
        })
    }

    /// The onomatopoeia key for a hit (`form` / `class` / `element` lower-case keys, "" = unknown).
    pub fn hit_word(&self, form: &str, class: &str, element: &str, heavy: bool) -> Option<&str> {
        let m = |rule: &str, v: &str| rule.is_empty() || rule.eq_ignore_ascii_case(v);
        self.hit_words
            .iter()
            .find(|r| m(&r.form, form) && m(&r.class, class) && m(&r.element, element) && r.heavy.is_none_or(|h| h == heavy))
            .map(|r| r.key.as_str())
            .filter(|k| !k.is_empty() && *k != "none")
    }

    pub fn max_near(&self, reduced: bool) -> usize {
        if reduced { self.budget.max_near_reduced } else { self.budget.max_near_full }
    }
}

// ------------------------------------------------------------------------------------ placement

/// The camera a word is placed for.
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub pos: Vec3,
    pub right: Vec3,
    pub up: Vec3,
    pub fwd: Vec3,
    /// tan(vertical fov / 2).
    pub tan_half: f32,
    pub aspect: f32,
}

impl View {
    pub fn new(cam: &GlobalTransform, fov_y: f32, aspect: f32) -> Self {
        View {
            pos: cam.translation(),
            right: cam.right().as_vec3(),
            up: cam.up().as_vec3(),
            fwd: cam.forward().as_vec3(),
            tan_half: (fov_y.clamp(0.2, 3.0) * 0.5).tan(),
            aspect: aspect.clamp(0.3, 5.0),
        }
    }

    /// (depth, x, y) with x / y in tan space (x = lateral / depth).
    pub fn project(&self, p: Vec3) -> (f32, f32, f32) {
        let rel = p - self.pos;
        let d = rel.dot(self.fwd);
        let dd = d.max(1e-3);
        (d, rel.dot(self.right) / dd, rel.dot(self.up) / dd)
    }

    pub fn unproject(&self, d: f32, x: f32, y: f32) -> Vec3 {
        self.pos + self.fwd * d + self.right * (x * d) + self.up * (y * d)
    }
}

/// Where a word ends up this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub pos: Vec3,
    /// Multiplier on the word's own scale.
    pub scale: f32,
}

/// Push a rect (centre `x, y`, half extents `hx, hy`, tan space) out of the disc of radius `r` at the
/// origin. `side`: 0 = up (sideways when that leaves the screen, `top` = tan-space screen top),
/// ±1 = diagonally up and out to that side. Returns the new centre.
pub fn clear_disc(x: f32, y: f32, hx: f32, hy: f32, r: f32, side: i8, top: f32) -> (f32, f32) {
    let cx = (x.abs() - hx).max(0.0);
    let cy = (y.abs() - hy).max(0.0);
    if cx * cx + cy * cy >= r * r {
        return (x, y);
    }
    let sideways = |s: f32| {
        let cy = (y.abs() - hy).max(0.0);
        (s * (hx + (r * r - cy * cy).max(0.0).sqrt()), y)
    };
    if side != 0 {
        // Own words: diagonally up and out (the lower corners hold the first-person viewmodel):
        // the smallest push along (side, 1) that clears the disc (horizontal if it leaves the top).
        let dir = Vec2::new(side as f32, 1.0).normalize();
        let clear = |t: f32| {
            let (x2, y2) = (x + dir.x * t, y + dir.y * t);
            let cx = (x2.abs() - hx).max(0.0);
            let cy = (y2.abs() - hy).max(0.0);
            cx * cx + cy * cy >= r * r
        };
        let (mut lo, mut hi) = (0.0f32, 2.0 * (r + hx + hy) + x.abs() + y.abs());
        for _ in 0..24 {
            let mid = 0.5 * (lo + hi);
            if clear(mid) { hi = mid } else { lo = mid }
        }
        let (x2, y2) = (x + dir.x * hi, y + dir.y * hi);
        return if y2 + hy <= top { (x2, y2) } else { sideways(side as f32) };
    }
    // Up: the rect's bottom edge clears the disc at the rect's closest x.
    let up_y = hy + (r * r - cx * cx).max(0.0).sqrt();
    let up_y = up_y.max(y);
    if up_y + hy <= top {
        (x, up_y)
    } else {
        sideways(if x < 0.0 { -1.0 } else { 1.0 })
    }
}

/// The camera is inside the event (you are the blast / the victim up close): its word is dropped at
/// spawn. (Placement itself never drops a word — a first-person hit's word drifts toward the camera
/// and is pushed back out instead.)
pub fn inside_event(cam: Vec3, anchor: Vec3, p: &Placement) -> bool {
    cam.distance(anchor) < p.skip_cam_dist
}

/// Place a word whose root is at `centre` with world half extents `half` (at scale 1).
/// `own`: the viewer is involved (smaller, peripheral). `side`: sticky push side for own words
/// (0 = not chosen yet: picked from where the word is, then kept). Always `Some` today (kept an
/// `Option` so a future rule may drop a word).
pub fn place_word(v: &View, centre: Vec3, half: Vec2, own: bool, side: &mut i8, p: &Placement) -> Option<Placed> {
    place_word_k(v, centre, half, own, if own { p.own_scale } else { 1.0 }, side, p)
}

/// [`place_word`] with an explicit size multiplier for own words (`own_scale` / `own_big_scale`).
pub fn place_word_k(v: &View, centre: Vec3, half: Vec2, own: bool, own_k: f32, side: &mut i8, p: &Placement) -> Option<Placed> {
    let (mut d, mut x, mut y) = v.project(centre);
    if d <= 0.3 {
        // At / behind the eye (a first-person contact point, a word drifting into the camera): no
        // meaningful screen spot — start from the crosshair and let the cone push place it.
        (x, y) = (0.0, 0.0);
    }
    // Min distance: out along the view ray (same screen spot, farther away).
    if d < p.min_cam_dist {
        d = p.min_cam_dist;
    }
    // Size clamp (screen fractions).
    let h_frac = half.y.max(1e-4) / (d * v.tan_half);
    let own_k = if own { own_k.clamp(0.1, 1.0) } else { 1.0 };
    let max_h = p.max_letter_h * own_k;
    let mut s = h_frac.clamp(p.min_letter_h.min(max_h), max_h) / h_frac;
    let w_frac = 2.0 * half.x.max(1e-4) * s / (d * v.tan_half * v.aspect) * 0.5;
    let max_w = p.max_word_w * own_k.max(0.5);
    if w_frac > max_w {
        s *= max_w / w_frac;
    }
    // Crosshair cone.
    let (hx, hy) = (half.x * s / d, half.y * s / d);
    let deg = if own { p.own_min_deg.max(p.crosshair_deg) } else { p.crosshair_deg } + p.margin_deg;
    let r = deg.to_radians().tan();
    let top = v.tan_half * 0.95;
    let push_side = if own {
        if *side == 0 {
            // Away from where it is; dead centre → left (the first-person weapon swings on the right).
            *side = if x > 0.02 { 1 } else { -1 };
        }
        *side
    } else {
        0
    };
    (x, y) = clear_disc(x, y, hx, hy, r, push_side, top);
    Some(Placed { pos: v.unproject(d, x, y), scale: s })
}

// ------------------------------------------------------------------------------------ budget

/// A word's priority: impact + involvement + accent (crit / finisher / KO).
pub fn priority(b: &Budget, impact: f32, local: bool, accent: bool) -> f32 {
    impact.clamp(0.0, 3.0) + if local { b.local_bonus } else { 0.0 } + if accent { b.accent_bonus } else { 0.0 }
}

/// With `live` = (priority, age) of the words competing near the camera and room for `max`: `Ok(None)`
/// = room, `Ok(Some(i))` = evict `live[i]` (the least important, oldest on ties) to make room for a
/// word of priority `new`, `Err(())` = skip the new word (everything live matters more).
#[allow(clippy::result_unit_err)]
pub fn pick_eviction(live: &[(f32, f32)], max: usize, new: f32) -> Result<Option<usize>, ()> {
    if live.len() < max.max(1) {
        return Ok(None);
    }
    let weakest = live
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.0.total_cmp(&b.1.0).then(b.1.1.total_cmp(&a.1.1)))
        .map(|(i, w)| (i, w.0));
    match weakest {
        Some((i, p)) if new >= p => Ok(Some(i)),
        _ => Err(()),
    }
}

/// Every-Nth word for rapid light hits, per source.
#[derive(Default, Debug)]
pub struct LightGate {
    /// source → (light hits in the current string, time of the last one).
    seen: HashMap<u64, (u32, f32)>,
}

impl LightGate {
    /// Should this hit say a word? `source` 0 = anonymous (never thinned).
    pub fn admit(&mut self, b: &Budget, source: u64, now: f32, impact: f32, accent: bool) -> bool {
        if accent || source == 0 || impact >= b.light_impact || b.light_every <= 1 {
            return true;
        }
        if self.seen.len() > 64 {
            self.seen.retain(|_, v| now - v.1 < b.light_reset);
        }
        let e = self.seen.entry(source).or_insert((0, f32::NEG_INFINITY));
        if now - e.1 > b.light_reset {
            e.0 = 0;
        }
        e.1 = now;
        let n = e.0;
        e.0 += 1;
        n % b.light_every == 0
    }
}

// ------------------------------------------------------------------------------------ titles

/// Stacked title offsets: `items` = (serial, height) of one caster's live titles (any order). Writes
/// the upward offset of each (same order): the newest (highest serial) sits on the base line, each
/// older one above the newer ones (+ `gap` × the height below it). Heights are in whatever unit the
/// caller uses. `centred`: the caller places a title by its centre (local titles: the newest stays
/// centred on its spot, the rest sit on top of it); else by its bottom edge (world titles).
pub fn stack_offsets(items: &[(u64, f32)], gap: f32, centred: bool, out: &mut Vec<f32>) {
    out.clear();
    out.resize(items.len(), 0.0);
    let newest = items.iter().max_by_key(|x| x.0).map_or(0.0, |x| x.1);
    for (i, (serial, h)) in items.iter().enumerate() {
        // Everything newer than me stacks below me.
        let mut off = 0.0;
        for (s2, h2) in items.iter() {
            if s2 > serial {
                off += h2 * (1.0 + gap);
            }
        }
        if centred && off > 0.0 {
            off += (h - newest) * 0.5;
        }
        out[i] = off;
    }
}

/// Stack rank of `serial` among `items` (0 = newest).
pub fn stack_rank(items: &[(u64, f32)], serial: u64) -> usize {
    items.iter().filter(|(s, _)| *s > serial).count()
}

pub fn build(app: &mut App) {
    app.insert_resource(TextBudget::shipped());
}

// ------------------------------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> View {
        // Camera at the origin looking down -Z, 70° vertical fov, 16:9.
        View { pos: Vec3::ZERO, right: Vec3::X, up: Vec3::Y, fwd: Vec3::NEG_Z, tan_half: (35f32).to_radians().tan(), aspect: 16.0 / 9.0 }
    }

    fn word_rect(v: &View, p: &Placed, half: Vec2) -> (f32, f32, f32, f32) {
        let (d, x, y) = v.project(p.pos);
        (x, y, half.x * p.scale / d, half.y * p.scale / d)
    }

    fn disc_clear(x: f32, y: f32, hx: f32, hy: f32, r: f32) -> bool {
        let cx = (x.abs() - hx).max(0.0);
        let cy = (y.abs() - hy).max(0.0);
        cx * cx + cy * cy >= r * r * 0.999
    }

    #[test]
    fn shipped_policy_parses() {
        let t = TextBudget::parse(DEFAULT_SRC).expect("text_budget.ron parses");
        assert!(t.placement.min_cam_dist > 1.0);
        assert!(t.budget.max_near_reduced <= t.budget.max_near_full);
        assert!(!t.hit_words.is_empty());
        // Every hit-word key exists in the onomatopoeia table.
        let ono = crate::OnomatopoeiaTable::parse(include_str!("../../../assets/fx/onomatopoeia.ron")).unwrap();
        for r in &t.hit_words {
            assert!(ono.entries.contains_key(&r.key), "hit word key {:?} missing from onomatopoeia.ron", r.key);
        }
    }

    #[test]
    fn a_melee_word_in_your_face_is_pushed_out_shrunk_and_off_the_crosshair() {
        let v = view();
        let p = Placement::default();
        // A 5-letter word, 0.6 m tall, dead centre 1.5 m ahead (a first-person spear hit).
        let half = Vec2::new(1.3, 0.3);
        let mut side = 0;
        let placed = place_word(&v, Vec3::new(0.0, 0.0, -1.5), half, false, &mut side, &p).unwrap();
        let (d, _, _) = v.project(placed.pos);
        assert!(d >= p.min_cam_dist - 1e-3, "pushed out to the min distance (d = {d})");
        let (x, y, hx, hy) = word_rect(&v, &placed, half);
        // Letter height ≤ the cap, word width ≤ the cap (screen fractions).
        assert!(hy / v.tan_half <= p.max_letter_h + 1e-4, "letter height {}", hy / v.tan_half);
        assert!(hx / (v.tan_half * v.aspect) <= p.max_word_w + 1e-4);
        // Clear of the crosshair cone, and pushed UP (someone else's hit: over the target).
        let r = (p.crosshair_deg + p.margin_deg).to_radians().tan();
        assert!(disc_clear(x, y, hx, hy, r), "overlaps the crosshair cone");
        assert!(y > 0.0 && x.abs() < 1e-3);
    }

    #[test]
    fn own_words_are_smaller_and_go_to_the_side_and_stay_there() {
        let v = view();
        let p = Placement::default();
        let half = Vec2::new(0.8, 0.25);
        let mut side = 0;
        let other = place_word(&v, Vec3::new(0.0, 0.0, -6.0), half, false, &mut 0, &p).unwrap();
        let own = place_word(&v, Vec3::new(0.2, 0.0, -6.0), half, true, &mut side, &p).unwrap();
        let (x, y, hx, hy) = word_rect(&v, &own, half);
        let (_, _, _, hy_other) = word_rect(&v, &other, half);
        assert!(hy < hy_other, "own word smaller");
        assert_eq!(side, 1);
        assert!(x > 0.0 && y > 0.0, "up and out");
        // Dead centre → the left (away from the first-person weapon).
        let mut s2 = 0;
        let _ = place_word(&v, Vec3::new(0.0, 0.0, -6.0), half, true, &mut s2, &p).unwrap();
        assert_eq!(s2, -1);
        let r = (p.own_min_deg + p.margin_deg).to_radians().tan();
        assert!(disc_clear(x, y, hx, hy, r), "own word peripheral");
        // Sticky: the same word a frame later, now a hair left of centre, stays on the right.
        let own2 = place_word(&v, Vec3::new(-0.05, 0.0, -6.0), half, true, &mut side, &p).unwrap();
        assert!(v.project(own2.pos).1 > 0.0);
    }

    #[test]
    fn own_big_hits_keep_more_drama_than_own_light_hits() {
        let v = view();
        let p = Placement::default();
        let half = Vec2::new(0.8, 0.3);
        let at = Vec3::new(0.0, 0.2, -3.5);
        let light = place_word_k(&v, at, half, true, p.own_scale, &mut 0, &p).unwrap();
        let big = place_word_k(&v, at, half, true, p.own_big_scale, &mut 0, &p).unwrap();
        let other = place_word(&v, at, half, false, &mut 0, &p).unwrap();
        assert!(light.scale < big.scale && big.scale <= other.scale);
    }

    #[test]
    fn far_words_keep_a_readable_size_and_are_left_alone_off_axis() {
        let v = view();
        let p = Placement::default();
        let half = Vec2::new(0.9, 0.3);
        // 40 m away, well off the crosshair: only scaled up to the minimum letter height.
        let at = Vec3::new(12.0, 3.0, -40.0);
        let placed = place_word(&v, at, half, false, &mut 0, &p).unwrap();
        let (_, _, _, hy) = word_rect(&v, &placed, half);
        assert!(hy / v.tan_half >= p.min_letter_h - 1e-4);
        assert!(placed.pos.distance(at) < 1e-3, "no push needed");
    }

    #[test]
    fn inside_the_event_drops_the_word_and_words_at_the_top_edge_go_sideways() {
        let v = view();
        let p = Placement::default();
        assert!(inside_event(v.pos, Vec3::new(0.0, 0.0, -0.3), &p));
        assert!(!inside_event(v.pos, Vec3::new(0.0, 0.0, -1.2), &p));
        // A first-person hit's word that drifted to 0.3 m is pushed back out, not dropped.
        let near = place_word(&v, Vec3::new(0.0, 0.0, -0.3), Vec2::splat(0.3), true, &mut 0, &p).unwrap();
        assert!(v.project(near.pos).0 >= p.min_cam_dist - 1e-3);
        // Even at / behind the eye an own word lands on screen, off the crosshair.
        let at_eye = place_word(&v, Vec3::new(0.0, 0.1, 0.1), Vec2::new(0.6, 0.3), true, &mut 0, &p).unwrap();
        let (d, x, y) = v.project(at_eye.pos);
        assert!(d >= p.min_cam_dist - 1e-3 && x.abs() < v.tan_half * v.aspect && y.abs() < v.tan_half && (x != 0.0 || y != 0.0));
        // A tall rect over the crosshair that would leave the top of the screen goes sideways.
        let (x, y) = clear_disc(0.0, 0.0, 0.1, 0.6, 0.15, 0, 0.7);
        assert!(x.abs() > 0.1 && y == 0.0);
        // Already clear: untouched.
        assert_eq!(clear_disc(0.5, 0.5, 0.1, 0.1, 0.15, 0, 0.7), (0.5, 0.5));
    }

    #[test]
    fn global_cap_replaces_the_least_important_and_skips_weaker_words() {
        // Room.
        assert_eq!(pick_eviction(&[(1.0, 0.1)], 3, 0.2), Ok(None));
        // Full: the weakest (oldest on ties) goes when the new word is at least as important.
        let live = [(1.0, 0.1), (0.5, 0.3), (0.5, 0.2)];
        assert_eq!(pick_eviction(&live, 3, 0.5), Ok(Some(1)));
        assert_eq!(pick_eviction(&live, 3, 2.0), Ok(Some(1)));
        // Weaker than everything live: skipped.
        assert_eq!(pick_eviction(&live, 3, 0.3), Err(()));
        // A crit beats a plain heavy.
        let b = Budget::default();
        assert!(priority(&b, 0.5, false, true) > priority(&b, 1.5, false, false));
        assert!(priority(&b, 0.8, true, false) > priority(&b, 0.8, false, false));
    }

    #[test]
    fn rapid_light_hits_say_every_nth_word_but_crits_and_heavies_always() {
        let b = Budget { light_every: 3, light_reset: 1.0, light_impact: 0.8, ..Budget::default() };
        let mut g = LightGate::default();
        let shown: Vec<bool> = (0..7).map(|i| g.admit(&b, 7, i as f32 * 0.2, 0.5, false)).collect();
        assert_eq!(shown, [true, false, false, true, false, false, true]);
        // Another attacker has their own count.
        assert!(g.admit(&b, 9, 1.3, 0.5, false));
        // Crits / heavy hits are never thinned.
        assert!(g.admit(&b, 7, 1.4, 0.5, true));
        assert!(g.admit(&b, 7, 1.45, 1.2, false));
        // A pause resets the string: its first hit shows again.
        assert!(g.admit(&b, 7, 5.0, 0.5, false));
        // Anonymous sources are never thinned.
        assert!(g.admit(&b, 0, 5.1, 0.1, false) && g.admit(&b, 0, 5.2, 0.1, false));
    }

    #[test]
    fn chain_titles_stack_newest_on_the_base_line() {
        let items = [(5u64, 0.1), (7, 0.08), (6, 0.12)];
        let mut out = Vec::new();
        stack_offsets(&items, 0.25, false, &mut out);
        // 7 (newest) on the base line; 6 above it; 5 above both.
        assert_eq!(out[1], 0.0);
        assert!((out[2] - 0.08 * 1.25).abs() < 1e-6);
        assert!((out[0] - (0.08 + 0.12) * 1.25).abs() < 1e-6);
        assert_eq!(stack_rank(&items, 7), 0);
        assert_eq!(stack_rank(&items, 5), 2);
        // Stacked titles never overlap: each offset clears the ones below it.
        let mut sorted: Vec<(f32, f32)> = items.iter().zip(&out).map(|((_, h), o)| (*o, *h)).collect();
        sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
        for w in sorted.windows(2) {
            assert!(w[1].0 >= w[0].0 + w[0].1);
        }
        // Centre-placed (local titles): the rects [off - h/2, off + h/2] never overlap either — a
        // two-line older title above a one-line newer one clears it.
        let items = [(1u64, 0.2), (2, 0.08)];
        stack_offsets(&items, 0.15, true, &mut out);
        assert_eq!(out[1], 0.0);
        assert!(out[0] - 0.2 * 0.5 >= 0.08 * 0.5, "older bottom {} clears newer top {}", out[0] - 0.1, 0.04);
    }

    #[test]
    fn hit_words_follow_style_weight_and_element() {
        let t = TextBudget::parse(DEFAULT_SRC).unwrap();
        assert_eq!(t.hit_word("sword", "light", "none", false), Some("shing"));
        assert_eq!(t.hit_word("sword", "medium", "none", false), Some("shing"));
        assert_eq!(t.hit_word("sword", "heavy", "none", true), Some("thoom"));
        assert_eq!(t.hit_word("hammer", "heavy", "none", true), Some("thoom"));
        assert_eq!(t.hit_word("spear", "reach", "none", false), Some("thrust"));
        assert_eq!(t.hit_word("", "", "electric", false), Some("electric"));
        assert_eq!(t.hit_word("none", "", "none", true), Some("hit_heavy"));
        assert_eq!(t.hit_word("none", "", "none", false), Some("hit_light"));
    }
}
