//! The generic transform: a move's ops × a list of modifiers → the twisted move.
//!
//! Two passes, both generic:
//! 1. **Typed conversions keyed by op kind** ([`convert`]) — only where a behaviour needs a
//!    different verb: homing lunges / strikes become a `Pounce` (the homing lunge), a homing blast a
//!    seeking `Salvo`; a delayed blast is planted (`Salvo` with a delay), a delayed strike marks
//!    (`DelayedHit`); a reversed blast implodes (a pull `Field` first), a reversed strike yanks;
//!    exploding strikes / blinks leave a blast, exploding leaps slam; chained hits arc
//!    (`DelayedHit` round you); lingering hits leave a `Zone`; spinning strikes go all round;
//!    piercing rams go through, drills stop grinding, waves cut; casts gain the mechanics.
//! 2. **Field roles** ([`rescale`]) over the op's own serialised form: every number whose field has
//!    a role (`assets/move_modifiers.ron` `roles`: `radius` = Size, `damage` = Damage, `Ram.speed`
//!    = Speed, `GazeStun.secs` = CcSecs…) is multiplied, every `element` swapped, every elemental
//!    `status` swapped. Ops added by other workers tomorrow are twisted with no code here.

use bd_core::mechanics::{Element, Mechanic, Recipe, Temperament};

use super::{ModKind, ModTable, Role};
use crate::powers::{At, MoveDef, MoveJuice, MoveOp, RecipeDef};
use crate::status::StatusKind;

/// Every modifier of a side folded together (multipliers multiplied and clamped to the caps).
#[derive(Clone, Debug, PartialEq)]
pub struct Gathered {
    pub size: f32,
    pub reach: f32,
    pub damage: f32,
    pub knock: f32,
    pub speed: f32,
    pub secs: f32,
    pub cone: f32,
    pub arc: f32,
    pub cooldown: f32,
    pub count: u8,
    pub repeat: u8,
    pub interval: f32,
    pub spread: f32,
    pub mech: Vec<Mechanic>,
    pub element: Option<Element>,
    pub fx: Option<String>,
    pub color: Option<[f32; 3]>,
    pub reverse: bool,
    pub delay: f32,
    pub linger: f32,
    pub chain: u8,
    pub silent: bool,
    pub invisible: bool,
    pub tier: i8,
    pub sparks: i8,
    pub punch: f32,
    pub shake: f32,
    pub boom: bool,
    /// Title words in reading order (count, size, speed, adverb, behaviour, element, style).
    pub titles: Vec<String>,
    /// Any modifier at all.
    pub any: bool,
}

impl Gathered {
    pub fn none() -> Self {
        Gathered {
            size: 1.0,
            reach: 1.0,
            damage: 1.0,
            knock: 1.0,
            speed: 1.0,
            secs: 1.0,
            cone: 1.0,
            arc: 1.0,
            cooldown: 1.0,
            count: 1,
            repeat: 1,
            interval: 0.0,
            spread: 0.0,
            mech: vec![],
            element: None,
            fx: None,
            color: None,
            reverse: false,
            delay: 0.0,
            linger: 0.0,
            chain: 0,
            silent: false,
            invisible: false,
            tier: 0,
            sparks: 0,
            punch: 0.0,
            shake: 0.0,
            boom: false,
            titles: vec![],
            any: false,
        }
    }

    fn has(&self, m: Mechanic) -> bool {
        self.mech.contains(&m)
    }
}

fn kind_order(k: ModKind) -> u8 {
    match k {
        ModKind::Count => 0,
        ModKind::Size => 1,
        ModKind::Speed => 2,
        ModKind::Adverb => 3,
        ModKind::Behaviour => 4,
        ModKind::Element => 5,
        ModKind::Style => 6,
    }
}

/// Fold modifier ids into one [`Gathered`] (stacking multiplies; the caps clamp the result).
pub fn gather(t: &ModTable, mods: &[u8]) -> Gathered {
    let c = &t.caps;
    let mut g = Gathered::none();
    let mut titled: Vec<(u8, usize, String)> = Vec::new();
    let mut count: u32 = 1;
    let mut repeat: u32 = 1;
    for (k, id) in mods.iter().take(super::MAX_MODS).enumerate() {
        let Some(m) = t.get(*id) else { continue };
        g.any = true;
        g.size *= m.size;
        g.reach *= m.reach;
        g.damage *= m.damage;
        g.knock *= m.knockback;
        g.speed *= m.speed;
        g.secs *= m.secs;
        g.cone *= m.cone;
        g.arc *= m.arc;
        g.cooldown *= m.cooldown;
        count = count.saturating_mul(m.count.max(1) as u32);
        repeat = repeat.saturating_mul(m.repeat.max(1) as u32);
        if m.interval > 0.0 {
            g.interval = if g.interval > 0.0 { g.interval.min(m.interval) } else { m.interval };
        }
        if m.spread > 0.0 {
            g.spread = g.spread.max(m.spread);
        }
        for x in &m.mech {
            if !g.mech.contains(x) {
                g.mech.push(*x);
            }
        }
        if let Some(e) = m.element {
            g.element = Some(e);
            // an element's own look unless the modifier names a style
            if m.fx.is_empty() {
                g.fx = t.look(e).map(|l| l.fx.clone()).filter(|s| !s.is_empty());
            }
            g.color = m.color.or_else(|| t.look(e).map(|l| l.color));
        }
        if !m.fx.is_empty() {
            g.fx = Some(m.fx.clone());
            if m.color.is_some() {
                g.color = m.color;
            }
        }
        g.reverse ^= m.reverse;
        g.delay += m.delay;
        g.linger = g.linger.max(m.linger);
        g.chain = g.chain.saturating_add(m.chain);
        g.silent |= m.silent;
        g.invisible |= m.invisible;
        g.tier = g.tier.saturating_add(m.tier);
        g.sparks = g.sparks.saturating_add(m.sparks);
        g.punch += m.punch;
        g.shake += m.shake;
        g.boom |= m.boom;
        if !m.title.is_empty() && !titled.iter().any(|(_, _, s)| *s == m.title) {
            titled.push((kind_order(m.kind), k, m.title.clone()));
        }
    }
    titled.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    g.titles = titled.into_iter().map(|(_, _, s)| s).collect();
    let fin = |v: f32, lo: f32, hi: f32| if v.is_finite() { v.clamp(lo, hi) } else { 1.0 };
    g.size = fin(g.size, c.size_min, c.size_max);
    g.reach = fin(g.reach, c.size_min, c.reach_max);
    g.damage = fin(g.damage, c.damage_min, c.damage_max);
    g.knock = fin(g.knock, 0.1, c.knock_max);
    g.speed = fin(g.speed, c.speed_min, c.speed_max);
    g.secs = fin(g.secs, 0.3, c.secs_max);
    g.cone = fin(g.cone, 0.25, c.cone_max);
    g.arc = fin(g.arc, 0.25, c.arc_max);
    g.cooldown = fin(g.cooldown, c.cooldown_min, c.cooldown_max);
    g.count = count.clamp(1, c.count_max.max(1) as u32) as u8;
    // counts past the fan become a stream (x10 → 8 shots)
    let spill = if count > c.count_max as u32 { count.div_ceil(c.count_max.max(1) as u32) } else { 1 };
    g.repeat = repeat.saturating_mul(spill).clamp(1, c.repeat_max.max(1) as u32) as u8;
    g.delay = g.delay.clamp(0.0, c.delay_max);
    g.linger = g.linger.clamp(0.0, c.linger_max);
    g.chain = g.chain.min(c.chain_max);
    g.tier = g.tier.clamp(-c.tier_bump_max, c.tier_bump_max);
    g.punch = g.punch.clamp(0.0, 6.0);
    g.shake = g.shake.clamp(0.0, 0.3);
    g
}

/// A twisted move, ready to run (host) or present (client).
#[derive(Clone, Debug, PartialEq)]
pub struct Twisted {
    pub def: MoveDef,
    /// Yaw offsets (deg) of the simultaneous copies of each shot (≥ 1 entry; `[0]` = one copy).
    pub fan: Vec<f32>,
    /// Yaw offset (deg) of each sequential shot (≥ 1 entry).
    pub shots: Vec<f32>,
    /// Seconds between shots.
    pub interval: f32,
    /// Ops fired as a last stage once the move's active phase ends (a fused attack delivered at the
    /// end of a movement), and after how long.
    pub after: Option<(f32, Vec<MoveOp>)>,
    pub tier: i8,
    pub sparks: i8,
    pub element: Option<Element>,
    /// Size multiplier (presentation).
    pub size: f32,
    /// A two-element windup (fusion / Hollow-Purple grammar).
    pub merge: Option<(Element, Element)>,
    pub title: String,
    /// Palm beams: pairs of hands fired at once (the recipe's count).
    pub hands: u8,
    /// A fusion: which rule made it ("an imbued strike"…); "" for a plain twist.
    pub how: &'static str,
}

impl Twisted {
    /// An untouched move.
    pub fn plain(def: &MoveDef) -> Self {
        Twisted {
            def: def.clone(),
            fan: vec![0.0],
            shots: vec![0.0],
            interval: 0.0,
            after: None,
            tier: 0,
            sparks: 0,
            element: None,
            size: 1.0,
            merge: None,
            title: title_of(def),
            hands: 1,
            how: "",
        }
    }

    /// Copies × shots.
    pub fn total(&self) -> u32 {
        (self.fan.len() * self.shots.len()) as u32
    }
}

/// A move's title (its juice title, else its name) in capitals.
pub fn title_of(def: &MoveDef) -> String {
    def.juice.as_ref().and_then(|j| j.title.clone()).unwrap_or_else(|| def.name.to_uppercase())
}

// ------------------------------------------------------------------------ op introspection

/// The variant name of an op ("Explode", "Cast"…), read from its serialised form (generic).
pub fn op_name(op: &MoveOp) -> String {
    match ron::to_string(op) {
        Ok(s) => s.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect(),
        Err(_) => String::new(),
    }
}

/// Numbers of the named fields in an op (first occurrence each), from its serialised form.
pub(crate) fn fields(op: &MoveOp, want: &[&str]) -> Vec<f32> {
    let s = ron::to_string(op).unwrap_or_default();
    want.iter()
        .map(|w| {
            let key = format!("{w}:");
            s.find(&key)
                .filter(|&p| p == 0 || !s.as_bytes()[p - 1].is_ascii_alphanumeric())
                .and_then(|p| {
                    let rest = &s[p + key.len()..];
                    let end = rest.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == 'e' || c == 'E')).unwrap_or(rest.len());
                    rest[..end].parse::<f32>().ok()
                })
                .unwrap_or(0.0)
        })
        .collect()
}

/// The element a move is flavoured with, read generically: its first non-Light `element` field
/// (any op, recipes too), else what its status says (Burn → Fire, Chill → Ice…), else Light.
pub fn element_of(def: &MoveDef) -> Element {
    let mut from_status = None;
    for op in &def.ops {
        let s = ron::to_string(op).unwrap_or_default();
        for (key, status) in [("element:", false), ("status:", true), ("merge:Some((", false)] {
            let mut from = 0;
            while let Some(p) = s[from..].find(key) {
                let at = from + p + key.len();
                let rest = s[at..].trim_start_matches("Some(");
                let ident: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
                if status {
                    let e = match ident.as_str() {
                        "Burn" => Some(Element::Fire),
                        "Chill" => Some(Element::Ice),
                        "Poison" => Some(Element::Poison),
                        "Wet" => Some(Element::Water),
                        "Float" => Some(Element::Psychic),
                        "Shadow" => Some(Element::Shadow),
                        "Stun" => Some(Element::Electric),
                        _ => None,
                    };
                    from_status = from_status.or(e);
                } else if let Some(e) = parse_element(&ident).filter(|e| *e != Element::Light) {
                    return e;
                }
                from = at;
            }
        }
    }
    from_status.unwrap_or(Element::Light)
}

/// The strongest hit in a move (max `damage` of any op) and its knockback.
pub fn max_hit(def: &MoveDef) -> (f32, f32) {
    let mut d = 0.0f32;
    let mut k = 0.0f32;
    for op in &def.ops {
        let f = fields(op, &["damage", "knockback"]);
        if f[0] > d {
            d = f[0];
            k = f[1];
        }
    }
    (d, k)
}

/// Does the op have an intrinsic count (a salvo's strikes, a barrage's hits, a volley recipe)?
fn intrinsic_count(t: &ModTable, op: &MoveOp) -> bool {
    let name = op_name(op);
    if let MoveOp::Cast { recipe, .. } = op {
        return recipe.count > 1 || recipe.mech.contains(&Mechanic::Swarm);
    }
    ["count", "hits"].iter().any(|f| t.role(&name, f) == Role::Count)
}

/// A palm-beam cast (the Hands archetype, fired as a signature beam: the kamehameha family).
fn palm_beam(op: &MoveOp) -> bool {
    matches!(op, MoveOp::Cast { recipe, .. } if recipe.form == bd_core::mechanics::Form::Hands)
}

// ------------------------------------------------------------------------ the text rescaler

/// Multipliers applied by role in [`rescale`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scales {
    pub size: f32,
    pub reach: f32,
    pub damage: f32,
    pub knock: f32,
    pub speed: f32,
    pub secs: f32,
    pub cone: f32,
    pub arc: f32,
    pub count: u32,
    pub element: Option<Element>,
    pub status: Option<StatusKind>,
}

impl Scales {
    pub fn of(g: &Gathered, t: &ModTable) -> Self {
        Scales {
            size: g.size,
            reach: g.reach,
            damage: g.damage,
            knock: g.knock,
            speed: g.speed,
            secs: g.secs,
            cone: g.cone,
            arc: g.arc,
            count: 1,
            element: g.element,
            status: g.element.and_then(|e| t.look(e)).and_then(|l| l.status),
        }
    }

    fn identity(&self) -> bool {
        [self.size, self.reach, self.damage, self.knock, self.speed, self.secs, self.cone, self.arc].iter().all(|v| (v - 1.0).abs() < 1e-6)
            && self.count <= 1
            && self.element.is_none()
    }
}

fn parse_element(s: &str) -> Option<Element> {
    Some(match s {
        "Light" => Element::Light,
        "Fire" => Element::Fire,
        "Electric" => Element::Electric,
        "Ice" => Element::Ice,
        "Water" => Element::Water,
        "Wind" => Element::Wind,
        "Earth" => Element::Earth,
        "Poison" => Element::Poison,
        "Psychic" => Element::Psychic,
        "Shadow" => Element::Shadow,
        "Atomic" => Element::Atomic,
        _ => return None,
    })
}

/// Statuses an element swap may re-flavour (never crowd control: Stun is left alone).
fn elemental_status(s: &str) -> bool {
    matches!(s, "Burn" | "Poison" | "Chill" | "Wet" | "Float" | "Shadow")
}

fn apply_number(role: Role, op: &str, lit: &str, s: &Scales) -> Option<String> {
    let int = !lit.contains(['.', 'e', 'E']);
    let v: f64 = lit.parse().ok()?;
    let m = match role {
        Role::Size => s.size,
        Role::Reach => s.reach,
        Role::Damage => s.damage,
        Role::Knock => s.knock,
        Role::Speed => s.speed,
        Role::Secs => s.secs,
        Role::CcSecs => s.secs.min(1.0),
        Role::Cone => s.cone,
        Role::Arc => s.arc,
        Role::Count => {
            if op == "Cast" && v <= 1.0 {
                1.0
            } else {
                s.count as f32
            }
        }
        Role::Fixed => 1.0,
    } as f64;
    if (m - 1.0).abs() < 1e-6 {
        return None;
    }
    let mut nv = v * m;
    if role == Role::Arc {
        nv = nv.min(179.0);
    }
    Some(if int { format!("{}", nv.round().clamp(0.0, 255.0) as u32) } else { format!("{:?}", nv as f32) })
}

/// Rewrite one op's serialised form by field roles (see the module docs). Pure and deterministic;
/// anything it can't read round-trips unchanged.
pub fn rescale(t: &ModTable, src: &str, s: &Scales) -> String {
    let b = src.as_bytes();
    let mut out = String::with_capacity(src.len() + 16);
    let mut stack: Vec<&str> = Vec::new();
    let mut field: Option<&str> = None;
    let mut i = 0;
    let current = |stack: &[&str]| -> String {
        stack.iter().rev().find(|x| !x.is_empty() && **x != "Some" && x.starts_with(|c: char| c.is_ascii_uppercase())).map_or(String::new(), |x| x.to_string())
    };
    while i < b.len() {
        let c = b[i];
        if c == b'"' {
            let start = i;
            i += 1;
            while i < b.len() {
                if b[i] == b'\\' {
                    i += 2;
                    continue;
                }
                if b[i] == b'"' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            let end = i.min(b.len());
            out.push_str(&src[start..end]);
            field = None;
            continue;
        }
        if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            let ident = &src[start..i];
            if i < b.len() && b[i] == b':' {
                field = Some(ident);
                out.push_str(ident);
                continue;
            }
            if i < b.len() && b[i] == b'(' {
                stack.push(ident);
                out.push_str(ident);
                out.push('(');
                i += 1;
                continue;
            }
            let rep = match field {
                Some("element") => s.element.filter(|_| parse_element(ident).is_some()).map(|e| format!("{e:?}")),
                Some("status") if elemental_status(ident) => s.status.map(|x| format!("{x:?}")),
                _ => None,
            };
            out.push_str(rep.as_deref().unwrap_or(ident));
            field = None;
            continue;
        }
        if c == b'(' {
            stack.push("");
            out.push('(');
            field = None;
            i += 1;
            continue;
        }
        if c == b')' {
            stack.pop();
            out.push(')');
            field = None;
            i += 1;
            continue;
        }
        if c == b'-' || c.is_ascii_digit() {
            let start = i;
            i += 1;
            while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.' || b[i] == b'e' || b[i] == b'E' || ((b[i] == b'-' || b[i] == b'+') && matches!(b[i - 1], b'e' | b'E'))) {
                i += 1;
            }
            let lit = &src[start..i];
            let rep = field.and_then(|f| {
                let op = current(&stack);
                apply_number(t.role(&op, f), &op, lit, s)
            });
            out.push_str(rep.as_deref().unwrap_or(lit));
            field = None;
            continue;
        }
        // anything else (punctuation; never non-ASCII outside strings in RON output) verbatim
        let ch = src[i..].chars().next().unwrap_or(' ');
        out.push(ch);
        i += ch.len_utf8().max(1);
    }
    out
}

/// One op through the role rescaler (unchanged if nothing applies or anything fails).
pub fn scale_op(t: &ModTable, op: &MoveOp, s: &Scales) -> MoveOp {
    if s.identity() {
        return op.clone();
    }
    let Ok(text) = ron::to_string(op) else { return op.clone() };
    let out = rescale(t, &text, s);
    if out == text {
        return op.clone();
    }
    ron::from_str::<MoveOp>(&out).unwrap_or_else(|_| op.clone())
}

// ------------------------------------------------------------------------ typed conversions

fn add_mech(r: &mut RecipeDef, m: Mechanic) {
    if !r.mech.contains(&m) {
        r.mech.push(m);
    }
}

fn is_lunge(op: &MoveOp) -> Option<f32> {
    match *op {
        MoveOp::Impulse { fwd, .. } if fwd >= 6.0 => Some(fwd),
        MoveOp::Ram { speed, .. } => Some(speed),
        MoveOp::Drill { speed, .. } => Some(speed),
        _ => None,
    }
}

/// (damage, knockback, element, up) of a strike op.
fn strike_of(op: &MoveOp) -> Option<(f32, f32, Element, f32)> {
    match *op {
        MoveOp::Shove { damage, knockback, element, up, .. } => Some((damage, knockback, element, up)),
        MoveOp::Finisher { damage, knockback, element, up, .. } => Some((damage, knockback, element, up)),
        MoveOp::Ram { damage, knockback, element, up, .. } => Some((damage, knockback, element, up)),
        MoveOp::Drill { damage, knockback, element, up, .. } => Some((damage, knockback, element, up)),
        _ => None,
    }
}

fn is_melee(op: &MoveOp) -> bool {
    matches!(
        op,
        MoveOp::Shove { .. }
            | MoveOp::Ram { .. }
            | MoveOp::Pounce { .. }
            | MoveOp::Barrage { .. }
            | MoveOp::Finisher { .. }
            | MoveOp::Drill { .. }
            | MoveOp::Grab { .. }
            | MoveOp::Rush { .. }
    )
}

fn is_blink(op: &MoveOp) -> bool {
    matches!(op, MoveOp::Blink { .. } | MoveOp::BlinkCarry { .. } | MoveOp::WarpTo { .. } | MoveOp::Transpose { .. })
}

/// Where a move lands: the first blast's placement, a cast's aim point, a strike's front.
fn impact(ops: &[MoveOp]) -> (At, f32, f32) {
    for op in ops {
        match *op {
            MoveOp::Explode { at, dist, radius, .. } => return (at, dist, radius),
            MoveOp::Salvo { at, dist, radius, .. } => return (at, dist, radius),
            MoveOp::SlamOnLand { radius, .. } => return (At::Me, 0.0, radius),
            _ => {}
        }
    }
    if ops.iter().any(|o| matches!(o, MoveOp::Cast { .. } | MoveOp::Seeker { .. } | MoveOp::BounceBeam { .. } | MoveOp::Wave { .. })) {
        return (At::Aim, 0.0, 3.5);
    }
    if ops.iter().any(is_melee) {
        return (At::Front, 3.0, 3.0);
    }
    (At::Me, 0.0, 3.5)
}

/// The typed pass (see the module docs). `el` = the move's element after the swap.
pub fn convert(ops: Vec<MoveOp>, g: &Gathered, el: Element) -> Vec<MoveOp> {
    let mut ops = ops;
    // ---- casts & seekers carry the mechanics
    for op in ops.iter_mut() {
        match op {
            MoveOp::Cast { recipe, merge, .. } => {
                for m in &g.mech {
                    add_mech(recipe, *m);
                }
                if g.chain > 0 {
                    add_mech(recipe, Mechanic::Ricochet);
                }
                if g.delay > 0.0 {
                    add_mech(recipe, Mechanic::Sticky);
                    add_mech(recipe, Mechanic::Explodes);
                }
                if g.invisible {
                    recipe.temperament = Temperament::Sneaky;
                    recipe.intensity = recipe.intensity.max(0.6);
                }
                if let (Some(e), Some(m)) = (g.element, merge.as_mut()) {
                    *m = (e, e);
                }
                recipe.mech.truncate(8);
            }
            MoveOp::Seeker { hits, chain, .. } => {
                if g.has(Mechanic::Ricochet) || g.chain > 0 {
                    *hits = hits.saturating_add(2 + g.chain);
                    *chain = chain.max(12.0);
                }
            }
            MoveOp::BounceBeam { bounces, .. } => {
                if g.has(Mechanic::Ricochet) || g.chain > 0 {
                    *bounces = bounces.saturating_add(3 + g.chain);
                }
            }
            MoveOp::Ram { through, .. } if g.has(Mechanic::Pierce) => *through = true,
            MoveOp::Drill { grind, .. } if g.has(Mechanic::Pierce) => *grind = 0.0,
            MoveOp::Wave { thin, .. } if g.has(Mechanic::Pierce) => *thin = true,
            MoveOp::Shove { range, angle, .. } if g.has(Mechanic::Pierce) => {
                *range *= 1.35;
                *angle = (*angle * 0.6).max(8.0);
            }
            _ => {}
        }
        if g.has(Mechanic::Spin) {
            match op {
                MoveOp::Shove { angle, .. } | MoveOp::Barrage { angle, .. } | MoveOp::Finisher { angle, .. } => *angle = 179.0,
                _ => {}
            }
        }
    }
    let (dmg, kb) = {
        let mut d = 0.0f32;
        let mut k = 0.0f32;
        for op in &ops {
            let f = fields(op, &["damage", "knockback"]);
            if f[0] > d {
                d = f[0];
                k = f[1];
            }
        }
        (d, k)
    };
    // ---- homing: lunges and strikes home in, blasts seek
    if g.has(Mechanic::Homing) && !ops.iter().any(|o| matches!(o, MoveOp::Pounce { .. })) {
        let lunge = ops.iter().position(|o| is_lunge(o).is_some());
        let strike = ops.iter().position(|o| matches!(o, MoveOp::Shove { .. } | MoveOp::Finisher { .. }));
        if let Some(li) = lunge {
            let speed = is_lunge(&ops[li]).unwrap_or(24.0).clamp(22.0, 40.0);
            let (d, k, e, up) = strike.and_then(|s| strike_of(&ops[s])).or_else(|| strike_of(&ops[li])).unwrap_or((dmg, kb, el, 0.0));
            let impulse = matches!(ops[li], MoveOp::Impulse { .. });
            ops[li] = MoveOp::Pounce { range: 30.0, speed, cone: 22.0, damage: d, knockback: k, element: e, up, bounce: 3.0, secs: 0.9 };
            if impulse && let Some(si) = strike {
                ops.remove(si);
            }
        } else if let Some(si) = strike {
            let (d, k, e, up) = strike_of(&ops[si]).unwrap_or((dmg, kb, el, 0.0));
            let reach = fields(&ops[si], &["range"])[0];
            ops[si] = MoveOp::Pounce { range: (reach * 4.0).clamp(10.0, 24.0), speed: 26.0, cone: 20.0, damage: d, knockback: k, element: e, up, bounce: 3.0, secs: 0.6 };
        }
        for op in ops.iter_mut() {
            if let MoveOp::Explode { radius, damage, knockback, carve, element, at, dist } = *op
                && at != At::Me
            {
                *op = MoveOp::Salvo {
                    count: 1,
                    delay: 0.25,
                    interval: 0.0,
                    radius,
                    damage,
                    knockback,
                    element,
                    carve,
                    up: 0.0,
                    at: At::Aim,
                    dist,
                    spread: 10.0,
                    seek: true,
                    track: false,
                    cut: false,
                };
            }
        }
    }
    // ---- delayed: blasts are planted, strikes mark
    if g.delay > 0.0 {
        let d = g.delay;
        for op in ops.iter_mut() {
            *op = match op.clone() {
                MoveOp::Explode { radius, damage, knockback, carve, element, at, dist } => {
                    MoveOp::Salvo { count: 1, delay: d, interval: 0.0, radius, damage, knockback, element, carve, up: 0.0, at, dist, spread: 0.0, seek: false, track: false, cut: false }
                }
                MoveOp::Shove { range, damage, knockback, element, up, .. } | MoveOp::Finisher { range, damage, knockback, element, up, .. } => {
                    MoveOp::DelayedHit { range: range.max(6.0), cone: 18.0, radius: 0.0, secs: d, damage, knockback, up, element }
                }
                MoveOp::Salvo { count, delay, interval, radius, damage, knockback, element, carve, up, at, dist, spread, seek, track, cut } => {
                    MoveOp::Salvo { count, delay: delay + d, interval, radius, damage, knockback, element, carve, up, at, dist, spread, seek, track, cut }
                }
                MoveOp::DelayedHit { range, cone, radius, secs, damage, knockback, up, element } => {
                    MoveOp::DelayedHit { range, cone, radius, secs: secs + d, damage, knockback, up, element }
                }
                other => other,
            };
        }
    }
    // ---- reverse: pushes pull, blasts implode, dashes go back
    if g.reverse {
        let mut out = Vec::with_capacity(ops.len() + 2);
        for op in ops.drain(..) {
            match op {
                MoveOp::Field { strength, radius, secs, at, dist } => out.push(MoveOp::Field { strength: -strength, radius, secs, at, dist }),
                MoveOp::Explode { radius, damage, knockback, carve, element, at, dist } => {
                    out.push(MoveOp::Field { strength: 45.0, radius: radius * 1.6, secs: 0.45, at, dist });
                    out.push(MoveOp::Explode { radius, damage, knockback: knockback * 0.25, carve, element, at, dist });
                }
                MoveOp::Shove { range, angle, damage, knockback, element, .. } => {
                    out.push(MoveOp::Shove { range, angle, damage, knockback: knockback * 0.2, element, up: 0.0 });
                    out.push(MoveOp::Yank { range: range + 3.0, speed: 20.0, cone: (angle * 0.5).clamp(5.0, 20.0) });
                }
                MoveOp::Impulse { fwd, up } => out.push(MoveOp::Impulse { fwd: -fwd, up }),
                MoveOp::Yank { range, speed, .. } => out.push(MoveOp::Shove { range: range.min(12.0), angle: 30.0, damage: 0.0, knockback: speed.min(40.0), element: el, up: 0.2 }),
                // travelling pushers roll in and drag them back to you
                MoveOp::Wave { length, speed, width, carve, damage, knockback, up, element, thin } => {
                    out.push(MoveOp::Wave { length, speed, width, carve, damage, knockback: knockback * 0.2, up, element, thin });
                    out.push(MoveOp::Yank { range: length.min(30.0), speed: 22.0, cone: (width * 2.0).clamp(8.0, 20.0) });
                }
                MoveOp::Surge { width, dist, speed, push, damage, element } => {
                    out.push(MoveOp::Surge { width, dist, speed, push: push * 0.2, damage, element });
                    out.push(MoveOp::Yank { range: dist.min(30.0), speed: 22.0, cone: 18.0 });
                }
                other => out.push(other),
            }
        }
        ops = out;
    }
    // ---- exploding: strikes and blinks leave a blast, leaps slam
    if g.has(Mechanic::Explodes) && !ops.iter().any(|o| matches!(o, MoveOp::Explode { .. } | MoveOp::Salvo { .. } | MoveOp::SlamOnLand { .. })) {
        if ops.iter().any(is_melee) {
            ops.push(MoveOp::Explode { radius: 3.2, damage: (dmg * 0.5).max(6.0), knockback: kb.max(12.0), carve: 1.0, element: el, at: At::Front, dist: 3.0 });
        } else if ops.iter().any(|o| matches!(o, MoveOp::Leap { .. }) || matches!(o, MoveOp::Impulse { up, .. } if *up >= 5.0)) {
            ops.push(MoveOp::SlamOnLand { radius: 4.0, damage: 10.0, knockback: 14.0, carve: 1.2 });
        } else if ops.iter().any(is_blink) {
            ops.push(MoveOp::Explode { radius: 3.5, damage: 8.0, knockback: 14.0, carve: 0.8, element: el, at: At::Me, dist: 0.0 });
        }
    }
    // ---- chain: the hit arcs to everyone near you
    let arcs_already = ops.iter().any(|o| matches!(o, MoveOp::Cast { .. } | MoveOp::Seeker { .. } | MoveOp::BounceBeam { .. }));
    if g.chain > 0 && !arcs_already && dmg > 0.0 {
        let n = g.chain as f32;
        ops.push(MoveOp::DelayedHit { range: 0.0, cone: 0.0, radius: 7.0 + 1.5 * n, secs: 0.2, damage: (dmg * 0.35).max(3.0), knockback: 4.0, up: 0.0, element: el });
    }
    // ---- lingering: a zone of its element where it lands
    if g.linger > 0.0 {
        let (at, dist, radius) = impact(&ops);
        let status = match el {
            Element::Fire | Element::Atomic => Some(StatusKind::Burn),
            Element::Ice => Some(StatusKind::Chill),
            Element::Poison => Some(StatusKind::Poison),
            Element::Water => Some(StatusKind::Wet),
            Element::Shadow => Some(StatusKind::Shadow),
            _ => None,
        };
        let slow = if el == Element::Ice { 0.3 } else { 0.0 };
        ops.push(MoveOp::Zone { radius: (radius * 1.1).clamp(2.5, 9.0), secs: g.linger, status, slow, dps: 3.0, lockout: false, at, dist, follow: false });
    }
    ops
}

// ------------------------------------------------------------------------ the whole move

/// Twist a move by a list of modifier ids (pure; see the module docs). No modifiers → the move as is.
pub fn twist(t: &ModTable, def: &MoveDef, mods: &[u8]) -> Twisted {
    let g = gather(t, mods);
    twist_gathered(t, def, &g)
}

/// [`twist`] with the modifiers already folded.
pub fn twist_gathered(t: &ModTable, def: &MoveDef, g: &Gathered) -> Twisted {
    let mut tw = Twisted::plain(def);
    if !g.any {
        return tw;
    }
    let c = &t.caps;
    let el = g.element.unwrap_or_else(|| element_of(def));
    // ---- copies: palm beams fire more pairs of hands; intrinsic counts multiply; instant area /
    //      strike ops fan out in one tick; anything else fires as quick sequential shots.
    let n = g.count.max(1);
    let mut s = Scales::of(g, t);
    let mut fan = 1u8;
    let mut seq = 1u8;
    let mut hands = 1u8;
    if n > 1 {
        if def.ops.iter().any(palm_beam) {
            hands = n;
        } else if def.ops.iter().any(|o| intrinsic_count(t, o)) {
            s.count = n as u32;
        } else if def.ops.iter().all(|o| t.fan_safe(&op_name(o))) {
            fan = n;
        } else {
            seq = n;
        }
    }
    let shots_n = (seq as u32 * g.repeat.max(1) as u32).clamp(1, c.repeat_max.max(1) as u32) as usize;
    // damage per copy / per shot (a triple is not three full moves)
    if fan > 1 || hands > 1 || s.count > 1 {
        s.damage *= c.copy_damage;
    }
    if shots_n > 1 {
        s.damage /= (shots_n as f32).powf(c.repeat_falloff.clamp(0.0, 1.0));
    }
    s.damage = s.damage.clamp(0.05, c.damage_max);
    // ---- ops: typed conversions, then roles
    let converted = convert(def.ops.clone(), g, el);
    let mut ops: Vec<MoveOp> = converted.iter().map(|o| scale_op(t, o, &s)).collect();
    if hands > 1 {
        for op in ops.iter_mut() {
            if let MoveOp::Cast { recipe, .. } = op
                && recipe.form == bd_core::mechanics::Form::Hands
            {
                recipe.count = hands;
            }
        }
    }
    ops.truncate(12);
    tw.def.ops = ops;
    // ---- yaw pattern: 0, +s, −s, +2s, −2s…
    let spread = if g.spread > 0.0 { g.spread } else { c.spread };
    let pattern = |k: usize| -> f32 {
        if k == 0 {
            0.0
        } else {
            let step = k.div_ceil(2) as f32;
            if k % 2 == 1 { spread * step } else { -spread * step }
        }
    };
    tw.fan = (0..fan as usize).map(pattern).collect();
    tw.shots = (0..shots_n).map(|k| if seq > 1 { pattern(k % seq as usize) } else { 0.0 }).collect();
    tw.interval = if g.interval > 0.0 { g.interval } else { c.interval };
    tw.hands = hands;
    // ---- the move's own numbers and look
    tw.def.cooldown = (def.cooldown * g.cooldown).clamp(0.05, 120.0);
    tw.tier = g.tier;
    tw.sparks = g.sparks;
    tw.element = g.element;
    tw.size = g.size;
    tw.merge = def.ops.iter().find_map(|o| match o {
        MoveOp::Cast { merge: Some((a, b)), .. } => Some(g.element.map_or((*a, *b), |e| (e, e))),
        _ => None,
    });
    let base_title = title_of(def);
    let mut words = g.titles.clone();
    words.push(base_title.clone());
    tw.title = words.join(" ");
    if !g.titles.is_empty() {
        let prefix = g.titles.iter().map(|w| w.to_lowercase()).collect::<Vec<_>>().join(" ");
        tw.def.name = format!("{prefix} {}", def.name);
    }
    if let Some(fx) = &g.fx {
        tw.def.fx = fx.clone();
    }
    tw.def.juice = Some(twist_juice(def.juice.as_ref(), g, &tw.title));
    tw
}

/// The move's juice, twisted: its title names the twist, bigger moves ripple wider, an element swap
/// recolours it, silent / invisible moves lose the fanfare.
pub fn twist_juice(j: Option<&MoveJuice>, g: &Gathered, title: &str) -> MoveJuice {
    let mut j = j.cloned().unwrap_or_default();
    if g.silent || g.invisible {
        j.title = None;
        j.boom = false;
        j.punch = 0.0;
        j.shake = 0.0;
        j.ripple = 0.0;
        if g.invisible {
            j.color = None;
        }
        return j;
    }
    j.title = Some(title.to_string());
    j.ripple *= g.size;
    j.debris *= g.size;
    j.punch = (j.punch * g.size.sqrt() + g.punch).clamp(0.0, 8.0);
    j.shake = (j.shake + g.shake).clamp(0.0, 0.4);
    j.boom |= g.boom || g.size >= 1.6;
    if let Some(c) = g.color {
        j.color = Some(c);
    }
    j
}

/// Twist a full construct recipe (a famous CAST the lexicon resolved: "spirit bomb", "hadouken").
pub fn twist_recipe(r: &mut Recipe, g: &Gathered) {
    if !g.any {
        return;
    }
    r.scale *= g.size;
    r.mass *= g.size.powi(2).clamp(0.2, 6.0);
    if let Some(e) = g.element {
        r.element = e;
    }
    let mut add = |m: Mechanic| {
        if !r.mechanics.contains(&m) {
            r.mechanics.push(m);
        }
    };
    for m in &g.mech {
        add(*m);
    }
    if g.chain > 0 {
        add(Mechanic::Ricochet);
    }
    if g.delay > 0.0 {
        add(Mechanic::Sticky);
        add(Mechanic::Explodes);
    }
    if g.invisible {
        r.temperament = Temperament::Sneaky;
        r.intensity = r.intensity.max(0.6);
    }
    let n = g.count.max(1);
    if n > 1 && r.form == bd_core::mechanics::Form::Hands {
        r.count = n;
    }
    *r = r.clone().normalized();
}

impl ModTable {
    /// Ops that may be copied several times in one tick (instant, no per-player state).
    pub fn fan_safe(&self, op: &str) -> bool {
        self.fan.iter().any(|o| o == op)
    }
}
