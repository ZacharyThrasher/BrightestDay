//! BORROWED MOVES + CHAINS on the strip (TOOLBOX PRESENTATION; wordless — docs/toolbox/PRESENTATION.md).
//!
//! * **Chips**: the last few borrowed moves you fired (say any move of any power: a one-shot) sit to
//!   the right of the power icons — small discs in the move's colour with its STYLE glyph (rubber,
//!   rigid, smoke, speedforce… `move_fx.ron` `glyph`), a clockwise cooldown sweep, a white flare
//!   when it fires and when it is ready again. Repeats wear a chip down (it shrinks a step per
//!   staleness step: repeating costs more sparks — cycle!). Unused chips fade after a while.
//! * **Chain pips**: a line with separators ("bamf -> hulk smash -> pistol") shows one pip per link;
//!   each pops as its move fires, the next one to fire breathes.
//! * **Tab** (the scoreboard key, like the power cards): the move's name floats over its chip.
//!
//! Inputs: [`MoveFxFired`] with `borrowed` (the local player's, from the sim's `PowerEvent::Fired`),
//! the own `ToolboxSnap` (host cooldowns, repeats, the pending chain — docs/toolbox/PRESENTATION_API.md),
//! `PowerEvent::ChainStopped`, and [`MoveChainFx`] (dev / synthetic chains). Nodes are spawned once; a frame writes a material only when
//! its quantised value changed.

use super::strip::{IconMaterial, IconParams, StripRow};
use crate::lobby::prefs::{Action, KeyBinds, UiPrefs};
use crate::present::powers::movefx::{self, MoveChainFx, MoveFxFired};
use crate::sim_bridge::{ActiveWorld, GameEvents, ViewKeys};
use bd_sim::GameEvent;
use bd_sim::powers::PowerEvent;
use bd_sim::powers::chain::ToolboxSnap;
use crate::state::{AppState, InGameMenu};
use bd_sim::powers::MoveDef;
use bevy::picking::Pickable;
use bevy::prelude::*;

pub const CHIPS: usize = 4;
pub const PIPS: usize = 8;
const CHIP: f32 = 26.0;
const PIP: f32 = 11.0;
/// A chip nobody used for this long (and ready) fades away.
const IDLE_FADE: f32 = 9.0;

#[derive(Clone, Copy, Debug)]
pub struct Chip {
    pub mv: &'static MoveDef,
    /// Packed TOOLBOX `MoveRef` + 1 (0 = unknown: counted locally).
    pub key: u32,
    pub color: [f32; 3],
    pub glyph: u8,
    pub cd: f32,
    pub left: f32,
    pub flash: f32,
    pub stale: u8,
    pub idle: f32,
}

/// The chips + chain state (other presenters / tests read it).
#[derive(Resource, Default, Debug)]
pub struct BorrowedHud {
    pub chips: [Option<Chip>; CHIPS],
    /// Links in the pending chain (0 = none) and how many have fired.
    pub chain_len: u8,
    pub chain_fired: u8,
    /// Pop timers per pip (1 → 0 after it fires).
    pub pop: [f32; PIPS],
    pub chain_done: f32,
}

impl BorrowedHud {
    /// A borrowed move fired: refresh / add its chip (the least recently used one makes room).
    pub fn fired(&mut self, mv: &'static MoveDef, key: u32, color: [f32; 3], glyph: u8, cd: f32) {
        if let Some(c) = self.chips.iter_mut().flatten().find(|c| std::ptr::eq(c.mv, mv)) {
            c.left = cd;
            c.cd = cd.max(0.05);
            c.flash = 1.0;
            c.stale = c.stale.saturating_add(1).min(4);
            c.idle = 0.0;
            return;
        }
        let chip = Chip { mv, key, color, glyph, cd: cd.max(0.05), left: cd, flash: 1.0, stale: 0, idle: 0.0 };
        let slot = self.chips.iter().position(|c| c.is_none()).unwrap_or_else(|| {
            // Evict the longest idle.
            let mut best = 0;
            for (k, c) in self.chips.iter().enumerate() {
                if let (Some(c), Some(b)) = (c, self.chips[best]) && c.idle > b.idle {
                    best = k;
                }
            }
            best
        });
        self.chips[slot] = Some(chip);
    }

    pub fn chain(&mut self, index: u8, len: u8) {
        let len = len.min(PIPS as u8);
        if len == 0 {
            self.chain_len = 0;
            return;
        }
        if index == 0 || len != self.chain_len {
            self.pop = [0.0; PIPS];
        }
        self.chain_len = len;
        self.chain_fired = (index + 1).min(len);
        if let Some(p) = self.pop.get_mut(index as usize) {
            *p = 1.0;
        }
        self.chain_done = 0.0;
    }

    /// The host's word (the own `ToolboxSnap`): cooldowns left, repeats, the pending chain.
    pub fn sync(&mut self, snap: &ToolboxSnap) {
        for c in self.chips.iter_mut().flatten() {
            if c.key == 0 {
                continue;
            }
            if let Some(&(_, left, uses)) = snap.cds.iter().find(|x| x.0 == c.key) {
                c.left = left as f32 / 255.0 * c.cd;
                c.stale = uses.min(4);
            }
        }
        let len = snap.fired.saturating_add(snap.pending);
        if snap.chain != 0 && len > 1 && (self.chain_len == 0 || snap.fired > self.chain_fired || len != self.chain_len) {
            // A link fired (or a new chain): pop the pip that just went.
            if snap.fired > 0 && snap.fired > self.chain_fired {
                self.chain(snap.fired - 1, len);
            } else if self.chain_len == 0 || len != self.chain_len {
                self.chain_len = len.min(PIPS as u8);
                self.chain_fired = snap.fired;
                self.pop = [0.0; PIPS];
                self.chain_done = 0.0;
            }
        }
    }

    pub fn tick(&mut self, dt: f32) {
        for c in self.chips.iter_mut() {
            let Some(ch) = c else { continue };
            let was = ch.left > 0.0;
            ch.left = (ch.left - dt).max(0.0);
            ch.flash = (ch.flash - dt * 3.0).max(0.0);
            if was && ch.left == 0.0 {
                ch.flash = 1.0;
            }
            // Wear heals slowly (the sim's staleness decays the same way: keep cycling).
            ch.idle += dt;
            if ch.idle > 4.0 && ch.stale > 0 && (ch.idle % 4.0) < dt {
                ch.stale -= 1;
            }
            if ch.left == 0.0 && ch.idle > IDLE_FADE + 1.0 {
                *c = None;
            }
        }
        for p in self.pop.iter_mut() {
            *p = (*p - dt * 4.0).max(0.0);
        }
        if self.chain_len > 0 && self.chain_fired >= self.chain_len {
            self.chain_done += dt;
            if self.chain_done > 0.5 {
                self.chain_len = 0;
            }
        }
    }
}

#[derive(Resource)]
struct Nodes {
    chips: [(Entity, Entity, Entity, Handle<IconMaterial>); CHIPS],
    pips: [(Entity, Handle<IconMaterial>); PIPS],
    names: [Option<&'static str>; CHIPS],
}

#[derive(Component)]
struct ChipName;

pub fn build(app: &mut App) {
    app.init_resource::<BorrowedHud>()
        .add_message::<MoveFxFired>()
        .add_message::<MoveChainFx>()
        .add_systems(Update, (attach, feed, draw).chain().in_set(crate::present::PresentSet::Sync));
}

/// Spawn the chips / pips once, into the strip's icon row (after the pickup).
fn attach(mut commands: Commands, rows: Query<Entity, Added<StripRow>>, mut mats: ResMut<Assets<IconMaterial>>) {
    for row in &rows {
        let mut mk = |mode: f32, seed: f32| mats.add(IconMaterial { params: IconParams { b: Vec4::new(mode, 0.0, 0.0, 0.0), c: Vec4::new(1.0, 1.0, 1.0, seed), ..default() } });
        let chips = std::array::from_fn(|k| {
            let m = mk(1.0, 3.1 + k as f32);
            let icon = commands.spawn((Node { width: px(CHIP), height: px(CHIP), ..default() }, MaterialNode(m.clone()), Pickable::IGNORE)).id();
            let name = commands
                .spawn((
                    ChipName,
                    Node { display: Display::None, position_type: PositionType::Absolute, bottom: px(CHIP + 4.0), left: px(-47.0), width: px(120), justify_content: JustifyContent::Center, ..default() },
                    Text::new(""),
                    TextFont::from_font_size(12.0),
                    TextColor(Color::srgba(1.0, 1.0, 1.0, 0.92)),
                    TextLayout::justify(Justify::Center),
                    Pickable::IGNORE,
                ))
                .id();
            let holder = commands.spawn((Node { display: Display::None, width: px(CHIP), height: px(CHIP), flex_shrink: 0.0, align_items: AlignItems::Center, justify_content: JustifyContent::Center, ..default() }, Pickable::IGNORE)).id();
            commands.entity(holder).add_children(&[icon, name]);
            (holder, icon, name, m)
        });
        let pips = std::array::from_fn(|k| {
            let m = mk(1.0, 0.7 * k as f32);
            let e = commands.spawn((Node { display: Display::None, width: px(PIP), height: px(PIP), flex_shrink: 0.0, ..default() }, MaterialNode(m.clone()), Pickable::IGNORE)).id();
            (e, m)
        });
        let chip_row = commands.spawn((Name::new("BorrowedChips"), Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(4), margin: UiRect::left(px(6)), ..default() }, Pickable::IGNORE)).id();
        let pip_row = commands.spawn((Name::new("ChainPips"), Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(2), margin: UiRect::left(px(4)), ..default() }, Pickable::IGNORE)).id();
        for c in &chips {
            commands.entity(chip_row).add_child(c.0);
        }
        for p in &pips {
            commands.entity(pip_row).add_child(p.0);
        }
        commands.entity(row).add_children(&[chip_row, pip_row]);
        commands.insert_resource(Nodes { chips, pips, names: [None; CHIPS] });
    }
}

fn feed(
    time: Res<Time<Real>>,
    mut fired: MessageReader<MoveFxFired>,
    mut chain: MessageReader<MoveChainFx>,
    active: Option<Res<ActiveWorld>>,
    table: Option<Res<movefx::table::MoveFx>>,
    keys: Res<ViewKeys>,
    mut events: MessageReader<GameEvents>,
    mut hud: ResMut<BorrowedHud>,
) {
    let local = active.as_ref().and_then(|a| a.local);
    hud.tick(time.delta_secs().min(0.1));
    for ev in events.read() {
        // A chain cut short (out of sparks, KO, stunned): the pips go at once.
        if let GameEvent::Power(PowerEvent::ChainStopped { player, .. }) = &ev.event
            && Some(*player) == local
        {
            hud.chain_len = 0;
        }
    }
    for f in fired.read() {
        if !f.borrowed || Some(f.player) != local {
            continue;
        }
        let corps = active.as_ref().and_then(|a| a.local_player()).map_or([0.3, 1.0, 0.45], |p| p.corps.color());
        let (color, glyph) = match table.as_deref() {
            Some(t) => {
                let look = movefx::resolve_look(t, f.mv, f.power, corps);
                (look.main, t.def(look.style).glyph)
            }
            None => (corps, 0),
        };
        hud.fired(f.mv, f.key.map_or(0, |k| k.pack() + 1), super::derive::vivid(color), glyph, f.mv.cooldown);
    }
    if keys.valid
        && let Some(tb) = keys.curr.own.as_ref().and_then(|o| o.toolbox)
    {
        hud.sync(&tb);
    }
    for c in chain.read() {
        if Some(c.player) == local {
            hud.chain(c.index, c.len);
        }
    }
}

fn q(x: f32) -> f32 {
    (x * 96.0).round() / 96.0
}

#[allow(clippy::too_many_arguments)]
fn draw(
    hud: Res<BorrowedHud>,
    nodes: Option<ResMut<Nodes>>,
    prefs: Res<UiPrefs>,
    state: Res<State<AppState>>,
    menu: Res<InGameMenu>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    binds: Res<KeyBinds>,
    focus: Res<crate::input::UiFocus>,
    active: Option<Res<ActiveWorld>>,
    mut mats: ResMut<Assets<IconMaterial>>,
    mut q_node: Query<&mut Node>,
    mut texts: Query<&mut Text, With<ChipName>>,
) {
    let Some(mut n) = nodes else { return };
    let alive = active.as_ref().and_then(|a| a.local_player()).is_some_and(|p| p.alive);
    let show = prefs.power_hud && *state.get() == AppState::InGame && !menu.0 && alive;
    let tab = show && ((!focus.typing && binds.pressed(Action::Scoreboard, &keys, &mouse)) || std::env::var_os("BD_TAB_TEST").is_some());
    let mut set_disp = |e: Entity, on: bool, size: Option<f32>| {
        let Ok(mut nd) = q_node.get_mut(e) else { return };
        let d = if on { Display::Flex } else { Display::None };
        if nd.display != d {
            nd.display = d;
        }
        if let Some(s) = size
            && nd.width != px(s)
        {
            nd.width = px(s);
            nd.height = px(s);
        }
    };
    for k in 0..CHIPS {
        let (holder, icon, name, ref h) = n.chips[k];
        let chip = hud.chips[k].filter(|_| show);
        set_disp(holder, chip.is_some(), None);
        set_disp(name, chip.is_some() && tab, None);
        let Some(c) = chip else { continue };
        let size = CHIP * (1.0 - 0.1 * c.stale as f32);
        set_disp(icon, true, Some(size));
        let fade = if c.left == 0.0 { (1.0 - (c.idle - IDLE_FADE).max(0.0)).clamp(0.0, 1.0) } else { 1.0 };
        let fill = if c.cd > 0.0 { 1.0 - c.left / c.cd } else { 1.0 };
        let p = IconParams {
            color: Vec4::new(c.color[0], c.color[1], c.color[2], q(fade)),
            a: Vec4::new(c.glyph as f32, q(fill), -1.0, q(c.flash)),
            b: Vec4::new(1.0, 0.0, 0.0, 0.0),
            c: Vec4::new(1.0, 1.0, 1.0, 3.1 + k as f32),
        };
        if mats.get(h).is_some_and(|m| m.params != p)
            && let Some(mut m) = mats.get_mut(h)
        {
            m.params = p;
        }
        if tab && n.names[k].is_none_or(|s| !std::ptr::eq(s, c.mv.name.as_str()))
            && let Ok(mut t) = texts.get_mut(name)
        {
            n.names[k] = Some(c.mv.name.as_str());
            t.0 = c.mv.name.to_uppercase();
        }
    }
    let len = hud.chain_len as usize;
    for k in 0..PIPS {
        let (e, ref h) = n.pips[k];
        let fired = k < hud.chain_fired as usize;
        let pop = hud.pop[k];
        let on = show && k < len && (!fired || pop > 0.0);
        let size = PIP * (1.0 + 0.6 * pop);
        set_disp(e, on, Some(size));
        if !on {
            continue;
        }
        let next = k == hud.chain_fired as usize;
        let col = if fired { [1.0, 1.0, 1.0] } else if next { [1.0, 0.9, 0.55] } else { [0.7, 0.72, 0.8] };
        let p = IconParams {
            color: Vec4::new(col[0], col[1], col[2], if fired { q(pop) } else if next { 1.0 } else { 0.7 }),
            a: Vec4::new(super::derive::glyph::CIRCLE as f32, if next { 0.0 } else { 1.0 }, -1.0, q(pop)),
            b: Vec4::new(1.0, next as u8 as f32, 0.0, 0.0),
            c: Vec4::new(1.0, 1.0, 1.0, k as f32 * 0.7),
        };
        if mats.get(h).is_some_and(|m| m.params != p)
            && let Some(mut m) = mats.get_mut(h)
        {
            m.params = p;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mv(name: &str) -> &'static MoveDef {
        Box::leak(Box::new(MoveDef { name: name.into(), cooldown: 2.0, ..default() }))
    }

    #[test]
    fn chips_refresh_wear_and_evict_the_idlest() {
        let mut h = BorrowedHud::default();
        let a = mv("a");
        h.fired(a, 0, [1.0; 3], 0, 2.0);
        h.fired(a, 0, [1.0; 3], 0, 2.0);
        assert_eq!(h.chips.iter().flatten().count(), 1, "the same move refreshes its chip");
        assert_eq!(h.chips[0].unwrap().stale, 1, "a repeat wears it");
        for n in ["b", "c", "d"] {
            h.tick(0.5);
            h.fired(mv(n), 0, [1.0; 3], 1, 1.0);
        }
        h.tick(0.1);
        h.fired(mv("e"), 0, [1.0; 3], 2, 1.0);
        assert!(h.chips.iter().flatten().all(|c| c.mv.name != "a"), "the idlest chip made room");
        // Cooldown sweeps back to ready.
        h.tick(5.0);
        assert!(h.chips.iter().flatten().all(|c| c.left == 0.0));
    }

    #[test]
    fn chain_pips_pop_in_order_then_clear() {
        let mut h = BorrowedHud::default();
        h.chain(0, 3);
        assert_eq!((h.chain_len, h.chain_fired), (3, 1));
        h.chain(1, 3);
        h.chain(2, 3);
        assert_eq!(h.chain_fired, 3);
        h.tick(0.3);
        assert_eq!(h.chain_len, 3, "the last pop plays out");
        h.tick(0.3);
        assert_eq!(h.chain_len, 0, "then the pips are gone");
    }
}
