//! Menu shell (bevy_ui): Title → Practice / Host / Join / Settings / Quit, plus the in-game Esc
//! overlay. Keyboard first: ↑/↓ (W/S) select, Enter/Space activate, ←/→ (A/D) adjust, Esc back.
//! Mouse hover selects, click activates (click / right-click step values up / down).

use crate::audio::PlaySfx;
use crate::settings::{ComfortSettings, Settings, FOV_MAX, FOV_MIN};
use crate::arena::{ArenaWorld, CurrentMap, MapKind};
use crate::state::{AfterLoading, AppState, GameMode, InGameMenu};
use bd_core::corps::Corps;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::ButtonState;
use bevy::prelude::*;
use bevy::ui::{ColorStop, LinearGradient};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuPage {
    Title,
    Join,
    Settings,
    Pause,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Item {
    Practice,
    Tutorial,
    Host,
    Join,
    Settings,
    Quit,
    Back,
    Resume,
    QuitToTitle,
    CodeField,
    Connect,
    Sensitivity,
    Fov,
    InvertY,
    Comfort,
    DynamicFov,
    Shake,
    FovKick,
    LandingDip,
    Flashes,
    Vignette,
    Vsync,
    Fullscreen,
    Aa,
    Volume,
}

impl Item {
    fn is_value(self) -> bool {
        matches!(
            self,
            Item::Sensitivity
                | Item::Fov
                | Item::InvertY
                | Item::Comfort
                | Item::DynamicFov
                | Item::Shake
                | Item::FovKick
                | Item::LandingDip
                | Item::Flashes
                | Item::Vignette
                | Item::Vsync
                | Item::Fullscreen
                | Item::Aa
                | Item::Volume
        )
    }
}

fn page_items(page: MenuPage) -> &'static [Item] {
    match page {
        MenuPage::Title => &[Item::Practice, Item::Tutorial, Item::Host, Item::Join, Item::Settings, Item::Quit],
        MenuPage::Join => &[Item::CodeField, Item::Connect, Item::Back],
        MenuPage::Settings => &[
            Item::Sensitivity,
            Item::Fov,
            Item::InvertY,
            Item::Comfort,
            Item::DynamicFov,
            Item::Shake,
            Item::FovKick,
            Item::LandingDip,
            Item::Flashes,
            Item::Vignette,
            Item::Vsync,
            Item::Fullscreen,
            Item::Aa,
            Item::Volume,
            Item::Back,
        ],
        MenuPage::Pause => &[Item::Resume, Item::Settings, Item::QuitToTitle],
    }
}

#[derive(Resource, Debug)]
pub struct MenuState {
    pub page: MenuPage,
    /// Page to return to from Settings.
    settings_parent: MenuPage,
    cursor: usize,
    pub join_code: String,
    rebuild: bool,
    caret_t: f32,
}

impl Default for MenuState {
    fn default() -> Self {
        Self { page: MenuPage::Title, settings_parent: MenuPage::Title, cursor: 0, join_code: String::new(), rebuild: true, caret_t: 0.0 }
    }
}

impl MenuState {
    fn goto(&mut self, page: MenuPage) {
        if page == MenuPage::Settings {
            self.settings_parent = if self.page == MenuPage::Pause { MenuPage::Pause } else { MenuPage::Title };
        }
        self.page = page;
        self.cursor = 0;
        self.rebuild = true;
    }
}

#[derive(Component)]
struct MenuRoot;
#[derive(Component)]
struct MenuRow(usize);
#[derive(Component)]
struct RowBar(usize);
#[derive(Component)]
struct RowLabel(usize);
#[derive(Component)]
struct RowValue(usize);

const ACCENT: Color = Color::srgb(0.25, 1.0, 0.42);

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuState>()
            .add_systems(OnEnter(AppState::Menu), enter_menu)
            .add_systems(OnExit(AppState::Menu), |mut m: ResMut<MenuState>| m.rebuild = true)
            .add_systems(
                Update,
                (toggle_pause, menu_input, menu_mouse, rebuild_menu, style_rows)
                    .chain()
                    .run_if(not(in_state(AppState::Loading))),
            );
    }
}

/// Title page on every entry; the first entry honours `--menu <page>`.
fn enter_menu(cli: Res<crate::cli::CliArgs>, mut m: ResMut<MenuState>, mut first_done: Local<bool>) {
    m.goto(MenuPage::Title);
    if std::mem::replace(&mut *first_done, true) {
        return;
    }
    match cli.menu_page.as_deref() {
        Some("settings") => m.goto(MenuPage::Settings),
        Some("join") => m.goto(MenuPage::Join),
        _ => {}
    }
}

/// Starts a game on a map: switches maps through the loading screen when needed.
#[derive(bevy::ecs::system::SystemParam)]
struct GameStarter<'w> {
    mode: ResMut<'w, GameMode>,
    current: ResMut<'w, CurrentMap>,
    after: ResMut<'w, AfterLoading>,
    arena: Option<Res<'w, ArenaWorld>>,
}

impl GameStarter<'_> {
    fn start(&mut self, start: Option<(MapKind, GameMode)>, next: &mut NextState<AppState>) {
        let Some((map, mode)) = start else { return };
        *self.mode = mode;
        let loaded = self.arena.as_ref().map(|a| a.info.kind);
        if loaded == Some(map) {
            next.set(AppState::InGame);
        } else {
            self.current.0 = map;
            self.after.0 = AppState::InGame;
            next.set(AppState::Loading);
        }
    }
}

fn menu_open(state: &AppState, pause: &InGameMenu) -> bool {
    *state == AppState::Menu || pause.0
}

fn toggle_pause(
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<AppState>>,
    mut pause: ResMut<InGameMenu>,
    mut menu: ResMut<MenuState>,
) {
    if *state.get() != AppState::InGame || !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    if !pause.0 {
        pause.0 = true;
        menu.goto(MenuPage::Pause);
    } else if menu.page == MenuPage::Pause {
        pause.0 = false;
        menu.rebuild = true;
    }
    // Esc inside Settings-from-pause is handled by menu_input (back).
}

fn step_setting(item: Item, s: &mut Settings, dir: i32, fine: bool) {
    let d = dir as f32;
    match item {
        Item::Sensitivity => {
            let step = if fine { 0.01 } else { 0.05 };
            s.sensitivity = ((s.sensitivity + d * step) * 100.0).round() / 100.0;
            s.sensitivity = s.sensitivity.clamp(0.05, 20.0);
        }
        Item::Fov => s.fov = (s.fov + d * if fine { 1.0 } else { 5.0 }).clamp(FOV_MIN, FOV_MAX).round(),
        Item::InvertY => s.invert_y = !s.invert_y,
        Item::Comfort => {
            s.comfort = if s.comfort.is_comfort_preset() { ComfortSettings::default() } else { ComfortSettings::COMFORT };
        }
        Item::DynamicFov => s.comfort.speed_fov = !s.comfort.speed_fov,
        Item::Shake => s.comfort.shake = step4(s.comfort.shake, d, 2.0),
        Item::FovKick => s.comfort.fov_kick = step4(s.comfort.fov_kick, d, 2.0),
        Item::LandingDip => s.comfort.landing_dip = step4(s.comfort.landing_dip, d, 2.0),
        Item::Flashes => s.comfort.flashes = step4(s.comfort.flashes, d, 1.0),
        Item::Vignette => s.comfort.vignette = step4(s.comfort.vignette, d, 1.0),
        Item::Vsync => s.vsync = !s.vsync,
        Item::Fullscreen => s.fullscreen = !s.fullscreen,
        Item::Aa => s.antialiasing = if dir > 0 { s.antialiasing.next() } else { s.antialiasing.prev() },
        Item::Volume => s.master_volume = ((s.master_volume + d * 0.05) * 20.0).round().clamp(0.0, 20.0) / 20.0,
        _ => {}
    }
}

fn pct(x: f32) -> String {
    if x <= 0.0 { "OFF".into() } else { format!("{:.0}%", x * 100.0) }
}

/// Step a 0..max slider in 25 % increments.
fn step4(x: f32, d: f32, max: f32) -> f32 {
    ((x + d * 0.25) * 4.0).round().clamp(0.0, max * 4.0) / 4.0
}

fn on_off(b: bool) -> String {
    if b { "ON".into() } else { "OFF".into() }
}

fn item_text(item: Item, s: &Settings, m: &MenuState) -> (String, Option<String>) {
    let v = |x: String| Some(x);
    match item {
        Item::Practice => ("PRACTICE".into(), None),
        Item::Tutorial => ("TUTORIAL".into(), None),
        Item::Host => ("HOST".into(), None),
        Item::Join => ("JOIN".into(), None),
        Item::Settings => ("SETTINGS".into(), None),
        Item::Quit => ("QUIT".into(), None),
        Item::Back => ("BACK".into(), None),
        Item::Resume => ("RESUME".into(), None),
        Item::QuitToTitle => ("LEAVE MATCH".into(), None),
        Item::Connect => ("CONNECT".into(), None),
        Item::CodeField => {
            let caret = if (m.caret_t * 2.0) as i32 % 2 == 0 { "_" } else { " " };
            let code = if m.join_code.is_empty() { String::new() } else { m.join_code.to_uppercase() };
            ("CODE".into(), v(format!("{code}{caret}")))
        }
        Item::Sensitivity => ("SENSITIVITY".into(), v(format!("{:.2}", s.sensitivity))),
        Item::Fov => ("FIELD OF VIEW".into(), v(format!("{:.0}", s.fov))),
        Item::InvertY => ("INVERT Y".into(), v(on_off(s.invert_y))),
        Item::Comfort => ("COMFORT PRESET".into(), v(on_off(s.comfort.is_comfort_preset()))),
        Item::DynamicFov => ("  SPEED FOV".into(), v(on_off(s.comfort.speed_fov))),
        Item::Shake => ("  CAMERA SHAKE".into(), v(pct(s.comfort.shake))),
        Item::FovKick => ("  FOV KICKS".into(), v(pct(s.comfort.fov_kick))),
        Item::LandingDip => ("  LANDING DIP".into(), v(pct(s.comfort.landing_dip))),
        Item::Flashes => ("  SCREEN FLASHES".into(), v(pct(s.comfort.flashes))),
        Item::Vignette => ("  COMFORT VIGNETTE".into(), v(pct(s.comfort.vignette))),
        Item::Vsync => ("VSYNC".into(), v(on_off(s.vsync))),
        Item::Fullscreen => ("FULLSCREEN".into(), v(on_off(s.fullscreen))),
        Item::Aa => ("ANTI-ALIASING".into(), v(s.antialiasing.label().into())),
        Item::Volume => ("VOLUME".into(), v(format!("{:.0}%", s.master_volume * 100.0))),
    }
}

#[allow(clippy::too_many_arguments)]
fn activate(
    item: Item,
    menu: &mut MenuState,
    settings: &mut Settings,
    state: &AppState,
    next: &mut NextState<AppState>,
    pause: &mut InGameMenu,
    start: &mut Option<(MapKind, GameMode)>,
    exit: &mut MessageWriter<AppExit>,
) {
    match item {
        Item::Practice => *start = Some((MapKind::Arena, GameMode::Practice)),
        Item::Tutorial => *start = Some((MapKind::Tutorial, GameMode::Practice)),
        Item::Host => *start = Some((MapKind::Arena, GameMode::Host)),
        Item::Join => menu.goto(MenuPage::Join),
        Item::Settings => menu.goto(MenuPage::Settings),
        Item::Quit => {
            exit.write(AppExit::Success);
        }
        Item::Back => back(menu, state, pause),
        Item::Resume => {
            pause.0 = false;
            menu.rebuild = true;
        }
        Item::QuitToTitle => {
            pause.0 = false;
            next.set(AppState::Menu);
        }
        Item::CodeField | Item::Connect => {
            if !menu.join_code.trim().is_empty() {
                *start = Some((MapKind::Arena, GameMode::Join(menu.join_code.trim().to_string())));
            }
        }
        other if other.is_value() => {
            step_setting(other, settings, 1, false);
            menu.rebuild = true;
        }
        _ => {}
    }
}

fn back(menu: &mut MenuState, state: &AppState, pause: &mut InGameMenu) {
    match menu.page {
        MenuPage::Settings => {
            let parent = if *state == AppState::InGame { MenuPage::Pause } else { menu.settings_parent };
            menu.goto(parent);
        }
        MenuPage::Join => menu.goto(MenuPage::Title),
        MenuPage::Pause => {
            pause.0 = false;
            menu.rebuild = true;
        }
        MenuPage::Title => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn menu_input(
    time: Res<Time<Real>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut typed: MessageReader<KeyboardInput>,
    state: Res<State<AppState>>,
    mut pause: ResMut<InGameMenu>,
    mut menu: ResMut<MenuState>,
    mut settings: ResMut<Settings>,
    mut next: ResMut<NextState<AppState>>,
    mut starter: GameStarter,
    mut exit: MessageWriter<AppExit>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    if !menu_open(state.get(), &pause) {
        typed.clear();
        return;
    }
    let menu = &mut *menu;
    let items = page_items(menu.page);
    let n = items.len();
    menu.caret_t += time.delta_secs();
    let current = items[menu.cursor.min(n - 1)];

    // Text entry on the Join page.
    if menu.page == MenuPage::Join {
        let before = menu.join_code.len();
        for ev in typed.read() {
            if ev.state != ButtonState::Pressed {
                continue;
            }
            match &ev.logical_key {
                Key::Backspace => {
                    menu.join_code.pop();
                }
                Key::Character(s) => {
                    for c in s.chars() {
                        if (c.is_ascii_alphanumeric() || "-._:".contains(c)) && menu.join_code.len() < 64 {
                            menu.join_code.push(c.to_ascii_lowercase());
                        }
                    }
                }
                _ => {}
            }
        }
        if menu.join_code.len() != before {
            menu.cursor = 0;
            menu.rebuild = true;
        }
    } else {
        typed.clear();
    }
    let typing = menu.page == MenuPage::Join && current == Item::CodeField;

    let up = keys.just_pressed(KeyCode::ArrowUp) || (!typing && keys.just_pressed(KeyCode::KeyW));
    let down = keys.just_pressed(KeyCode::ArrowDown)
        || (!typing && keys.just_pressed(KeyCode::KeyS))
        || keys.just_pressed(KeyCode::Tab);
    let left = keys.just_pressed(KeyCode::ArrowLeft) || (!typing && keys.just_pressed(KeyCode::KeyA));
    let right = keys.just_pressed(KeyCode::ArrowRight) || (!typing && keys.just_pressed(KeyCode::KeyD));
    let alt = keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]);
    let enter = !alt && (keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter) || (!typing && keys.just_pressed(KeyCode::Space)));
    let fine = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);

    if up {
        menu.cursor = (menu.cursor + n - 1) % n;
        sfx.write(PlaySfx::new("ui_move"));
    }
    if down {
        menu.cursor = (menu.cursor + 1) % n;
        sfx.write(PlaySfx::new("ui_move"));
    }
    if (left || right) && current.is_value() {
        step_setting(current, &mut settings, if right { 1 } else { -1 }, fine);
        menu.rebuild = true;
        sfx.write(PlaySfx::new("ui_move"));
    }
    if enter {
        let mut start = None;
        activate(current, menu, &mut settings, state.get(), &mut next, &mut pause, &mut start, &mut exit);
        starter.start(start, &mut next);
        sfx.write(PlaySfx::new("ui_select"));
    }
    // Esc = back (the pause toggle itself is handled in toggle_pause).
    if keys.just_pressed(KeyCode::Escape) && menu.page != MenuPage::Pause {
        back(menu, state.get(), &mut pause);
        sfx.write(PlaySfx::new("ui_back"));
    }
    // Keep the blinking caret alive.
    if menu.page == MenuPage::Join && ((menu.caret_t * 2.0) as i32) != (((menu.caret_t - time.delta_secs()) * 2.0) as i32) {
        menu.rebuild = true;
    }
}

#[allow(clippy::too_many_arguments)]
fn menu_mouse(
    rows: Query<(&Interaction, &MenuRow), Changed<Interaction>>,
    all_rows: Query<(&Interaction, &MenuRow)>,
    mouse: Res<ButtonInput<MouseButton>>,
    state: Res<State<AppState>>,
    mut pause: ResMut<InGameMenu>,
    mut menu: ResMut<MenuState>,
    mut settings: ResMut<Settings>,
    mut next: ResMut<NextState<AppState>>,
    mut starter: GameStarter,
    mut exit: MessageWriter<AppExit>,
) {
    if !menu_open(state.get(), &pause) {
        return;
    }
    let menu = &mut *menu;
    for (i, row) in &rows {
        if matches!(i, Interaction::Hovered | Interaction::Pressed) {
            menu.cursor = row.0;
        }
    }
    let items = page_items(menu.page);
    let hovered = all_rows.iter().find(|(i, _)| matches!(i, Interaction::Hovered | Interaction::Pressed)).map(|(_, r)| r.0);
    let Some(idx) = hovered else { return };
    let item = items[idx.min(items.len() - 1)];
    if mouse.just_pressed(MouseButton::Left) {
        let mut start = None;
        activate(item, menu, &mut settings, state.get(), &mut next, &mut pause, &mut start, &mut exit);
        starter.start(start, &mut next);
    } else if mouse.just_pressed(MouseButton::Right) && item.is_value() {
        step_setting(item, &mut settings, -1, false);
        menu.rebuild = true;
    }
}

fn rebuild_menu(
    mut commands: Commands,
    mut menu: ResMut<MenuState>,
    settings: Res<Settings>,
    state: Res<State<AppState>>,
    pause: Res<InGameMenu>,
    roots: Query<Entity, With<MenuRoot>>,
) {
    let open = menu_open(state.get(), &pause);
    if !menu.rebuild && (open == !roots.is_empty()) {
        return;
    }
    menu.rebuild = false;
    for e in &roots {
        commands.entity(e).despawn();
    }
    if !open {
        return;
    }
    let in_game = *state.get() == AppState::InGame;
    let page = menu.page;
    let items = page_items(page);
    let accent = Corps::Willpower.color();
    let glow = Color::srgb(accent[0], accent[1], accent[2]);

    let backdrop = if in_game {
        BackgroundGradient::from(LinearGradient::new(
            LinearGradient::TO_RIGHT,
            vec![
                ColorStop::new(Color::srgba(0.0, 0.01, 0.02, 0.85), percent(0)),
                ColorStop::new(Color::srgba(0.0, 0.01, 0.02, 0.55), percent(100)),
            ],
        ))
    } else {
        BackgroundGradient::from(LinearGradient::new(
            LinearGradient::TO_RIGHT,
            vec![
                ColorStop::new(Color::srgba(0.0, 0.01, 0.03, 0.82), percent(0)),
                ColorStop::new(Color::srgba(0.0, 0.01, 0.03, 0.45), percent(38)),
                ColorStop::new(Color::srgba(0.0, 0.0, 0.0, 0.0), percent(70)),
            ],
        ))
    };

    commands
        .spawn((
            Name::new("MenuRoot"),
            MenuRoot,
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(0),
                top: px(0),
                bottom: px(0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                padding: UiRect::left(percent(7)),
                row_gap: px(6),
                ..default()
            },
            backdrop,
            GlobalZIndex(80),
        ))
        .with_children(|root| {
            // Title block.
            let (title, sub) = match page {
                MenuPage::Title => ("BRIGHTEST DAY", None),
                MenuPage::Join => ("JOIN", Some("lobby code or ip:port")),
                MenuPage::Settings => ("SETTINGS", None),
                MenuPage::Pause => ("PAUSED", None),
            };
            root.spawn((
                Text::new(title),
                TextFont::from_font_size(if page == MenuPage::Title { 64.0 } else { 44.0 }),
                TextColor(Color::WHITE),
                TextShadow { offset: Vec2::new(0.0, 3.0), color: glow.with_alpha(0.35) },
                Node { margin: UiRect::bottom(px(if sub.is_some() { 2.0 } else { 26.0 })), ..default() },
            ));
            if let Some(sub) = sub {
                root.spawn((
                    Text::new(sub),
                    TextFont::from_font_size(15.0),
                    TextColor(Color::srgba(1.0, 1.0, 1.0, 0.45)),
                    Node { margin: UiRect::bottom(px(22)), ..default() },
                ));
            }
            // Accent line under the title.
            root.spawn((
                Node { width: px(64), height: px(3), margin: UiRect::bottom(px(22)), ..default() },
                BackgroundColor(glow),
            ));

            for (i, item) in items.iter().enumerate() {
                let (label, value) = item_text(*item, &settings, &menu);
                root.spawn((
                    MenuRow(i),
                    Button,
                    Node {
                        width: px(if page == MenuPage::Settings || page == MenuPage::Join { 520.0 } else { 360.0 }),
                        height: px(if page == MenuPage::Settings { 32.0 } else { 40.0 }),
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: px(14),
                        ..default()
                    },
                    BackgroundColor(Color::NONE),
                ))
                .with_children(|row| {
                    row.spawn((RowBar(i), Node { width: px(3), height: px(22), ..default() }, BackgroundColor(Color::NONE)));
                    row.spawn((
                        RowLabel(i),
                        Text::new(label),
                        TextFont::from_font_size(24.0),
                        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.55)),
                        Node { flex_grow: 1.0, ..default() },
                    ));
                    if let Some(v) = value {
                        row.spawn((
                            RowValue(i),
                            Text::new(v),
                            TextFont::from_font_size(22.0),
                            TextColor(Color::srgba(1.0, 1.0, 1.0, 0.55)),
                        ));
                    }
                });
            }
            if page == MenuPage::Settings {
                root.spawn((
                    Text::new("left/right adjust   shift = fine   esc back"),
                    TextFont::from_font_size(13.0),
                    TextColor(Color::srgba(1.0, 1.0, 1.0, 0.3)),
                    Node { margin: UiRect::top(px(18)), ..default() },
                ));
            }
        });
}

#[allow(clippy::type_complexity)]
fn style_rows(
    time: Res<Time<Real>>,
    menu: Res<MenuState>,
    mut rows: Query<(&MenuRow, &mut Node, &mut BackgroundColor), (Without<RowBar>, Without<RowLabel>)>,
    mut bars: Query<(&RowBar, &mut BackgroundColor, &mut Node), Without<MenuRow>>,
    mut labels: Query<(&RowLabel, &mut TextColor), Without<RowValue>>,
    mut values: Query<(&RowValue, &mut TextColor), Without<RowLabel>>,
) {
    let dt = time.delta_secs();
    let k = 1.0 - (-18.0 * dt).exp();
    for (row, mut node, mut bg) in &mut rows {
        let sel = row.0 == menu.cursor;
        let target = if sel { 14.0 } else { 0.0 };
        let cur = if let Val::Px(x) = node.padding.left { x } else { 0.0 };
        let next = cur + (target - cur) * k;
        if (next - cur).abs() > 0.01 {
            node.padding.left = px(next);
        }
        let want = if sel {
            selected_wash(ACCENT)
        } else {
            Color::NONE
        };
        if bg.0 != want {
            bg.0 = want;
        }
    }
    for (bar, mut bg, mut node) in &mut bars {
        let sel = bar.0 == menu.cursor;
        let want = if sel { ACCENT } else { Color::srgba(1.0, 1.0, 1.0, 0.12) };
        if bg.0 != want {
            bg.0 = want;
        }
        let h = if sel { px(26) } else { px(14) };
        if node.height != h {
            node.height = h;
        }
    }
    for (l, mut c) in &mut labels {
        let want = if l.0 == menu.cursor { Color::WHITE } else { Color::srgba(1.0, 1.0, 1.0, 0.55) };
        if c.0 != want {
            c.0 = want;
        }
    }
    for (v, mut c) in &mut values {
        let want = if v.0 == menu.cursor { ACCENT } else { Color::srgba(1.0, 1.0, 1.0, 0.45) };
        if c.0 != want {
            c.0 = want;
        }
    }
}

/// Selected-row wash: a faint tint of the accent.

fn selected_wash(accent: Color) -> Color {
    accent.with_alpha(0.07)
}
