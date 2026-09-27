//! World side of portals, force fields and links (COMBAT.md §5f), and Shrink/Grow plumbing.
//! The player portal/field steps are free functions so client prediction runs the exact same code.

use bd_core::input::PlayerInput;
use bd_core::mechanics::{FormCategory, Mechanic};
use bd_core::{ConstructId, DT, PlayerId, Quat, Tick, Vec3};

use super::World;
use crate::constructs::AnimState;
use crate::events::{GameEvent, Traveller};
use crate::movement::{MoveParams, PlayerState};
use crate::space::*;

/// Pass a player through a portal if the hull centre crossed a disc this tick. Rotates position,
/// velocity, hooks and the view (the input's yaw/pitch). Returns (portal id, rotation).
pub(crate) fn player_portals(
    st: &mut PlayerState,
    input: &mut PlayerInput,
    cooldown: &mut f32,
    params: &MoveParams,
    pre_center: Vec3,
    pre_vel: Vec3,
    portals: &[PortalPair],
) -> Option<(u16, Quat)> {
    if *cooldown > 0.0 || portals.is_empty() {
        return None;
    }
    let hull = st.hull(params);
    let center = hull.center();
    let half = hull.half();
    for pp in portals {
        let Some(t) = pp.crossing_body(pre_center, center, half) else { continue };
        let exit = *pp.exit(&t);
        let entry_n = pp.ends[t.entry].normal;
        // The surface behind a portal stopped the body this tick: carry the pre-collision speed.
        let v = if pre_vel.dot(entry_n) < st.vel.dot(entry_n) { pre_vel } else { st.vel };
        let new_center = clear_of_face(t.point, half, &exit);
        st.pos = new_center - Vec3::Y * half.y;
        st.vel = t.rot * v;
        st.grounded = false;
        st.release_tethers();
        let look = t.rot * input.look_dir();
        input.yaw = (-look.x).atan2(-look.z);
        input.pitch = look.y.clamp(-1.0, 1.0).asin().clamp(-1.56, 1.56);
        *cooldown = PORTAL_COOLDOWN;
        return Some((pp.id, t.rot));
    }
    None
}

/// Force fields on a player (the owner is immune; tethered ×0.3; big bodies resist).
pub(crate) fn player_fields(st: &mut PlayerState, id: PlayerId, tethered: bool, size: f32, fields: &[ForceField], dt: f32) {
    if fields.is_empty() {
        return;
    }
    let mut a = Vec3::ZERO;
    for f in fields {
        if f.owner != id {
            a += f.accel_at(st.pos + Vec3::Y * 0.9);
        }
    }
    if a == Vec3::ZERO {
        return;
    }
    let mut k = if tethered { FIELD_RESIST } else { 1.0 };
    k /= size.max(0.5).powi(2);
    let a = a * k;
    let mut dv = a * dt;
    // A strong field lifts grounded players off their feet (friction would otherwise pin them).
    if st.grounded && a.length() > 8.0 {
        dv.y = dv.y.max(1.5);
        st.grounded = false;
        st.sliding = false;
    }
    st.vel += dv;
}

impl World {
    pub fn create_portal(&mut self, owner: PlayerId, a: PortalEnd, b: PortalEnd, secs: f32, construct: Option<ConstructId>) -> Option<u16> {
        if !(a.pos.is_finite() && b.pos.is_finite()) || self.portals.len() >= 32 {
            return None;
        }
        // One pair per owner: a new pair replaces the old one.
        let mut closed = Vec::new();
        self.portals.retain(|p| {
            let keep = p.owner != owner;
            if !keep {
                closed.push(p.id);
            }
            keep
        });
        for id in closed {
            self.events.push(GameEvent::PortalClosed { id });
        }
        let id = self.next_space_id();
        let expires = self.tick + (secs.clamp(0.1, 120.0) / DT) as Tick;
        let pair = PortalPair { id, owner, ends: [a, b], expires, construct };
        self.portals.push(pair);
        self.events.push(GameEvent::PortalOpened { id, owner, ends: pair.ends });
        Some(id)
    }

    pub fn create_field(&mut self, owner: PlayerId, pos: Vec3, radius: f32, strength: f32, secs: f32, construct: Option<ConstructId>) -> Option<u16> {
        if !pos.is_finite() || !radius.is_finite() || !strength.is_finite() || self.fields.len() >= 32 {
            return None;
        }
        let id = self.next_space_id();
        let expires = self.tick + (secs.clamp(0.1, 120.0) / DT) as Tick;
        let field = ForceField { id, owner, pos, radius: radius.clamp(1.0, 40.0), strength: strength.clamp(-80.0, 80.0), expires, construct };
        self.fields.push(field);
        self.events.push(GameEvent::FieldCreated { field });
        Some(id)
    }

    fn end_valid(&self, e: &LinkEnd) -> bool {
        match *e {
            LinkEnd::Player(p) => self.player(p).is_some_and(|p| p.alive),
            LinkEnd::Construct(c) => self.constructs.get(c).is_some_and(|c| c.gone.is_none()),
            LinkEnd::Point(p) => p.is_finite(),
        }
    }

    pub fn create_link(&mut self, owner: Option<PlayerId>, a: LinkEnd, b: LinkEnd, max_len: f32, secs: f32) -> Option<u16> {
        if a == b || !self.end_valid(&a) || !self.end_valid(&b) || self.links.len() >= 32 {
            return None;
        }
        let id = self.next_space_id();
        let expires = self.tick + (secs.clamp(0.1, 60.0) / DT) as Tick;
        // Starts with some slack: at least the current distance (it tightens as they separate).
        let cur = match (self.link_end_state(&a), self.link_end_state(&b)) {
            (Some(x), Some(y)) => (y.0 - x.0).length(),
            _ => 0.0,
        };
        let link = Link { id, owner, a, b, max_len: max_len.clamp(1.0, 30.0).max(cur.min(30.0)), expires, hp: LINK_HP };
        self.links.push(link);
        self.events.push(GameEvent::LinkCreated { link });
        Some(id)
    }

    /// (anchor position, velocity, inverse mass) of a link end.
    fn link_end_state(&self, e: &LinkEnd) -> Option<(Vec3, Vec3, f32)> {
        let base = &self.cfg.move_params;
        match *e {
            LinkEnd::Player(id) => {
                let p = self.player(id).filter(|p| p.alive)?;
                let inv = if p.seat.is_some_and(|s| !s.suit) { 0.0 } else { 1.0 / (PLAYER_MASS * p.size.scale.powi(3)) };
                Some((p.hull(base).center(), p.mv.vel, inv))
            }
            LinkEnd::Construct(cid) => {
                let c = self.constructs.get(cid).filter(|c| c.gone.is_none())?;
                let fixed = matches!(c.category, FormCategory::Structure) || c.attached();
                Some((c.center(), c.vel, if fixed { 0.0 } else { 1.0 / c.mass().max(1.0) }))
            }
            LinkEnd::Point(p) => Some((p, Vec3::ZERO, 0.0)),
        }
    }

    fn link_apply(&mut self, e: &LinkEnd, dv: Vec3) {
        match *e {
            LinkEnd::Player(id) => {
                if let Some(p) = self.player_mut(id) {
                    p.mv.vel += dv;
                    if dv.y > 0.5 {
                        p.mv.grounded = false;
                    }
                }
            }
            LinkEnd::Construct(cid) => {
                if let Some(c) = self.constructs.get_mut(cid) {
                    c.vel += dv;
                }
            }
            LinkEnd::Point(_) => {}
        }
    }

    /// Inextensible chains with slack: remove separating velocity past the length and pull back
    /// the excess (soft positional correction through velocity, so movement's swept collision
    /// keeps players out of walls). Mass-weighted; points are immovable.
    pub(crate) fn solve_links(&mut self, dt: f32) {
        for k in 0..self.links.len() {
            let l = self.links[k];
            let (Some(a), Some(b)) = (self.link_end_state(&l.a), self.link_end_state(&l.b)) else { continue };
            let d = b.0 - a.0;
            let dist = d.length();
            if dist <= l.max_len || dist < 1e-4 {
                continue;
            }
            if dist > l.max_len * 4.0 + 10.0 {
                // Torn apart (a portal jump, a respawn): the chain snaps.
                self.links[k].hp = 0.0;
                continue;
            }
            let w = a.2 + b.2;
            if w <= 0.0 {
                continue;
            }
            let n = d / dist;
            let rel = (b.1 - a.1).dot(n);
            let excess = dist - l.max_len;
            let bias = (excess / dt * 0.2).min(12.0);
            let j = (rel.max(0.0) + bias) / w;
            self.link_apply(&l.a, n * (j * a.2));
            self.link_apply(&l.b, -n * (j * b.2));
        }
    }

    /// Housekeeping for portals / fields / links, plus constructs through portals and in fields,
    /// and blades cutting links.
    pub(crate) fn step_space(&mut self, dt: f32) {
        let tick = self.tick;
        // Fields follow their construct.
        for f in &mut self.fields {
            if let Some(cid) = f.construct {
                match self.constructs.get(cid).filter(|c| c.gone.is_none()) {
                    Some(c) => f.pos = c.center(),
                    None => f.expires = tick,
                }
            }
        }
        // Constructs: fields and portals (free-moving bodies only).
        let fields = std::mem::take(&mut self.fields);
        let portals = std::mem::take(&mut self.portals);
        let mut teleports: Vec<(ConstructId, u16, Quat, Vec3)> = Vec::new();
        for c in self.constructs.iter_mut() {
            if c.attached() || !c.active() || c.category == FormCategory::Structure || c.pilot.is_some() {
                continue;
            }
            if !fields.is_empty() {
                let resist = if c.has(Mechanic::Heavy) { FIELD_RESIST } else { 1.0 };
                for f in &fields {
                    if f.owner != c.owner && f.construct != Some(c.id) {
                        c.vel += f.accel_at(c.center()) * (resist * dt);
                    }
                }
            }
            if c.portal_cd <= 0.0 && !portals.is_empty() {
                for pp in &portals {
                    if let Some(t) = pp.crossing_body(c.prev_pos, c.pos, c.half) {
                        let exit = *pp.exit(&t);
                        c.pos = clear_of_face(t.point, c.half, &exit);
                        c.vel = t.rot * c.vel;
                        c.rot = (t.rot * c.rot).normalize();
                        c.prev_pos = c.pos;
                        c.portal_cd = PORTAL_COOLDOWN;
                        teleports.push((c.id, pp.id, t.rot, c.pos));
                        break;
                    }
                }
            }
        }
        self.fields = fields;
        self.portals = portals;
        for (cid, portal, rot, pos) in teleports {
            self.events.push(GameEvent::Teleported { who: Traveller::Construct(cid), portal, rot, pos });
        }
        // Blades in mid-swing cut links they touch.
        self.cut_links_with_blades();
        // Expiry.
        let mut i = 0;
        while i < self.portals.len() {
            let p = &self.portals[i];
            if p.expires <= tick || p.ends.iter().any(|e| e.hp <= 0.0) {
                let id = p.id;
                self.portals.remove(i);
                self.events.push(GameEvent::PortalClosed { id });
            } else {
                i += 1;
            }
        }
        let mut i = 0;
        while i < self.fields.len() {
            if self.fields[i].expires <= tick {
                let id = self.fields[i].id;
                self.fields.remove(i);
                self.events.push(GameEvent::FieldEnded { id });
            } else {
                i += 1;
            }
        }
        let mut i = 0;
        while i < self.links.len() {
            let l = self.links[i];
            let valid = self.end_valid(&l.a) && self.end_valid(&l.b);
            if l.expires <= tick || l.hp <= 0.0 || !valid {
                self.links.remove(i);
                self.events.push(GameEvent::LinkBroken { id: l.id, cut: l.hp <= 0.0 });
            } else {
                i += 1;
            }
        }
    }

    fn cut_links_with_blades(&mut self) {
        if self.links.is_empty() {
            return;
        }
        for c in self.constructs.iter() {
            let blade = c.has(Mechanic::Blade) || matches!(c.recipe.form, bd_core::mechanics::Form::Sword | bd_core::mechanics::Form::Axe);
            if !blade || c.anim_state != AnimState::Active {
                continue;
            }
            let a = c.pos;
            let b = c.pos + c.rot * Vec3::new(0.0, 0.0, -2.0 * c.half.z);
            for k in 0..self.links.len() {
                let l = self.links[k];
                let (Some(x), Some(y)) = (link_pos(self, &l.a), link_pos(self, &l.b)) else { continue };
                if seg_seg_dist(a, b, x, y) < 0.5 {
                    self.links[k].hp = 0.0;
                }
            }
        }
    }

    /// Explosions damage portal rings and links in range.
    pub(crate) fn damage_space_in_radius(&mut self, pos: Vec3, r: f32, dmg: f32) {
        for p in &mut self.portals {
            for e in &mut p.ends {
                if (e.pos - pos).length() <= r + e.radius {
                    e.hp -= dmg;
                }
            }
        }
        for k in 0..self.links.len() {
            let l = self.links[k];
            let (Some(x), Some(y)) = (link_pos(self, &l.a), link_pos(self, &l.b)) else { continue };
            if seg_seg_dist(pos, pos, x, y) <= r {
                self.links[k].hp -= dmg;
            }
        }
    }

    /// Damage one portal ring (shots / melee from the component system).
    pub fn damage_portal(&mut self, id: u16, end: usize, dmg: f32) {
        if let Some(p) = self.portals.iter_mut().find(|p| p.id == id) {
            if let Some(e) = p.ends.get_mut(end) {
                e.hp -= dmg.max(0.0);
            }
        }
    }

    /// Damage / cut a link (Blade cuts outright).
    pub fn damage_link(&mut self, id: u16, dmg: f32, blade: bool) {
        if let Some(l) = self.links.iter_mut().find(|l| l.id == id) {
            l.hp = if blade { 0.0 } else { l.hp - dmg.max(0.0) };
        }
    }

    pub(crate) fn remove_links_of_player(&mut self, id: PlayerId) {
        for l in &mut self.links {
            if l.a == LinkEnd::Player(id) || l.b == LinkEnd::Player(id) {
                l.hp = 0.0;
            }
        }
    }

    /// Drop references to a leaving player.
    pub(crate) fn remove_space_refs_player(&mut self, id: PlayerId) {
        let mut ev = Vec::new();
        self.portals.retain(|p| {
            let keep = p.owner != id;
            if !keep {
                ev.push(GameEvent::PortalClosed { id: p.id });
            }
            keep
        });
        self.fields.retain(|f| {
            let keep = f.owner != id;
            if !keep {
                ev.push(GameEvent::FieldEnded { id: f.id });
            }
            keep
        });
        self.links.retain(|l| {
            let keep = l.owner != Some(id) && l.a != LinkEnd::Player(id) && l.b != LinkEnd::Player(id);
            if !keep {
                ev.push(GameEvent::LinkBroken { id: l.id, cut: false });
            }
            keep
        });
        self.events.extend(ev);
    }

    /// After constructs were reaped: fields / links / portals must not point at dead constructs or
    /// dead players.
    pub(crate) fn cleanup_space_refs(&mut self) {
        let mut i = 0;
        while i < self.fields.len() {
            let f = self.fields[i];
            if f.construct.is_some_and(|c| self.constructs.get(c).is_none()) {
                self.fields.remove(i);
                self.events.push(GameEvent::FieldEnded { id: f.id });
            } else {
                i += 1;
            }
        }
        for p in &mut self.portals {
            if p.construct.is_some_and(|c| self.constructs.get(c).is_none()) {
                // Portals outlive the construct that opened them (they have their own 12 s).
                p.construct = None;
            }
        }
        let mut i = 0;
        while i < self.links.len() {
            let l = self.links[i];
            if !self.end_valid(&l.a) || !self.end_valid(&l.b) {
                self.links.remove(i);
                self.events.push(GameEvent::LinkBroken { id: l.id, cut: false });
            } else {
                i += 1;
            }
        }
    }
}

fn link_pos(w: &World, e: &LinkEnd) -> Option<Vec3> {
    match *e {
        LinkEnd::Player(id) => w.player(id).filter(|p| p.alive).map(|p| p.hull(&w.cfg.move_params).center()),
        LinkEnd::Construct(c) => w.constructs.get(c).map(|c| c.center()),
        LinkEnd::Point(p) => Some(p),
    }
}

/// Distance between segments [a, b] and [c, d].
fn seg_seg_dist(a: Vec3, b: Vec3, c: Vec3, d: Vec3) -> f32 {
    let d1 = b - a;
    let d2 = d - c;
    let r = a - c;
    let aa = d1.dot(d1);
    let ee = d2.dot(d2);
    let f = d2.dot(r);
    let (s, t);
    if aa <= 1e-9 && ee <= 1e-9 {
        return r.length();
    }
    if aa <= 1e-9 {
        s = 0.0;
        t = (f / ee).clamp(0.0, 1.0);
    } else {
        let cc = d1.dot(r);
        if ee <= 1e-9 {
            t = 0.0;
            s = (-cc / aa).clamp(0.0, 1.0);
        } else {
            let bb = d1.dot(d2);
            let den = aa * ee - bb * bb;
            let s0 = if den > 1e-9 { ((bb * f - cc * ee) / den).clamp(0.0, 1.0) } else { 0.0 };
            let t0 = (bb * s0 + f) / ee;
            if t0 < 0.0 {
                t = 0.0;
                s = (-cc / aa).clamp(0.0, 1.0);
            } else if t0 > 1.0 {
                t = 1.0;
                s = ((bb - cc) / aa).clamp(0.0, 1.0);
            } else {
                t = t0;
                s = s0;
            }
        }
    }
    ((a + d1 * s) - (c + d2 * t)).length()
}

impl World {
    /// Set flammable terrain (leaves / wood) alight around `pos` (chemistry, COMBAT.md §9).
    pub fn ignite_terrain(&mut self, pos: Vec3, radius: f32, by: Option<PlayerId>) {
        let World { constructs, voxels, .. } = self;
        constructs.chemistry_mut().ignite_area(voxels, pos, radius, by);
    }

    /// Portal gun impact: place the owner's next ring (A, then B, alternating) on the surface at
    /// `point` facing `normal`. With both placed the pair opens; later shots move one end at a time
    /// (Portal-game style) and refresh the pair's lifetime.
    pub fn place_portal_end(&mut self, owner: PlayerId, point: Vec3, normal: Vec3, shot_dir: Vec3) {
        let n = normal.normalize_or_zero();
        let Some(i) = self.pidx(owner) else { return };
        if n == Vec3::ZERO || !point.is_finite() {
            return;
        }
        // Floor / ceiling rings are "up"-oriented along the shot; wall rings stand upright.
        let up = if n.y.abs() > 0.7 { Vec3::new(shot_dir.x, 0.0, shot_dir.z).normalize_or(Vec3::NEG_Z) } else { Vec3::Y };
        let end = fit_portal(&self.voxels, PortalEnd::new(point + n * 0.05, n, up, PORTAL_RADIUS));
        let slot = self.players[i].portal_slot.min(1);
        self.players[i].portal_slot = 1 - slot;
        self.events.push(GameEvent::PortalPlaced { owner, slot, end });
        let expires = self.tick + (PORTAL_LIFETIME / DT) as Tick;
        if let Some(pp) = self.portals.iter_mut().find(|p| p.owner == owner) {
            if let Some(e) = pp.ends.get_mut(slot as usize) {
                *e = end;
            }
            pp.expires = expires;
            let (id, ends) = (pp.id, pp.ends);
            self.events.push(GameEvent::PortalOpened { id, owner, ends });
            return;
        }
        match self.players[i].portal_pending.take() {
            Some(other) => {
                let (a, b) = if slot == 1 { (other, end) } else { (end, other) };
                self.create_portal(owner, a, b, PORTAL_LIFETIME, None);
            }
            None => self.players[i].portal_pending = Some(end),
        }
    }
}
