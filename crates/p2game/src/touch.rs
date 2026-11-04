//! Touch controls: on-screen buttons, a movement stick, look-by-drag, taps as clicks, and a layout
//! editor. Enabled on Android (and on desktop with `P2GAME_TOUCH=1`, for testing with a touch
//! screen or a simulated one).
//!
//! * Buttons and the stick do not invent input. They write the same Bevy messages a keyboard,
//!   mouse or wheel would (`KeyboardInput`, `MouseButtonInput`, `MouseWheel`) in `PreUpdate`,
//!   before `InputSystems`, so the player's bindings from the game's ini apply to them too.
//! * A touch that starts on empty screen and moves turns the view (`MouseMotion`). A short tap
//!   clicks where the finger was (menus, the map, the inventory) and moves the cursor there.
//! * The MOUSE button (top right, left of the gear) turns on mouse mode: a visible cursor that one
//!   finger drags, and a tap clicks where the cursor is. Menus and the inventory are used this way.
//! * The gear button (top right) opens the editor: tap a control to select it, drag it to move,
//!   drag its corner handle to resize, and change its action, opacity, or add or delete controls.
//!   The layout is saved to `touch_layout.txt` in the user folder (survives app updates).
//!
//! Positions are fractions of the window (0..1), so one layout fits every phone. Labels are
//! English: Bevy's built-in font has no Cyrillic.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Instant;

use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput, NativeKey};
use bevy::input::mouse::{MouseButtonInput, MouseMotion, MouseScrollUnit, MouseWheel};
use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::input::InputSystems;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

/// Touch layer on? Android always; desktop only with `P2GAME_TOUCH=1`.
pub fn enabled() -> bool {
    cfg!(target_os = "android") || std::env::var_os("P2GAME_TOUCH").is_some_and(|v| !v.is_empty() && v != "0")
}

pub struct TouchPlugin;

impl Plugin for TouchPlugin {
    fn build(&self, app: &mut App) {
        if !enabled() {
            return;
        }
        // The launcher's "Edit controls" starts the game with the editor already open.
        let editing = std::env::var_os("P2GAME_TOUCH_EDIT").is_some();
        app.insert_resource(TouchLayout { items: load_layout().unwrap_or_else(default_layout) })
            .insert_resource(TouchEditor { editing, panel: editing, selected: None, dirty: true, last_size: (0.0, 0.0) })
            .init_resource::<Fingers>()
            .init_resource::<Held>()
            .add_systems(Startup, spawn_cursor)
            .add_systems(PreUpdate, route.before(InputSystems))
            .add_systems(Update, (rebuild, paint).chain());
    }
}

// ----------------------------------------------------------------------------------------------
// What a button sends.

#[derive(Clone, Copy, Debug, PartialEq)]
enum Out {
    Key(KeyCode),
    Mouse(MouseButton),
    /// Mouse wheel steps (+1 = up, next weapon in POSTAL 2's default binds is the wheel).
    Wheel(f32),
}

struct ActionDef {
    id: &'static str,
    label: &'static str,
    out: Out,
}

/// Default POSTAL 2 binds (README "Keybinds"). `id` is what the layout file stores.
const ACTIONS: &[ActionDef] = &[
    ActionDef { id: "fire", label: "Fire", out: Out::Mouse(MouseButton::Left) },
    ActionDef { id: "altfire", label: "Alt fire", out: Out::Mouse(MouseButton::Right) },
    ActionDef { id: "reload", label: "Reload", out: Out::Mouse(MouseButton::Middle) },
    ActionDef { id: "jump", label: "Jump", out: Out::Key(KeyCode::Space) },
    ActionDef { id: "crouch", label: "Crouch", out: Out::Key(KeyCode::KeyC) },
    ActionDef { id: "walk", label: "Walk", out: Out::Key(KeyCode::ShiftLeft) },
    ActionDef { id: "use", label: "Use", out: Out::Key(KeyCode::KeyE) },
    ActionDef { id: "kick", label: "Kick", out: Out::Key(KeyCode::KeyQ) },
    ActionDef { id: "throw", label: "Throw", out: Out::Key(KeyCode::KeyZ) },
    ActionDef { id: "holster", label: "Holster", out: Out::Key(KeyCode::KeyF) },
    ActionDef { id: "inventory", label: "Inventory", out: Out::Key(KeyCode::Tab) },
    ActionDef { id: "map", label: "Map", out: Out::Key(KeyCode::KeyM) },
    ActionDef { id: "health", label: "Health", out: Out::Key(KeyCode::KeyT) },
    ActionDef { id: "menu", label: "Menu", out: Out::Key(KeyCode::Escape) },
    ActionDef { id: "enter", label: "Enter", out: Out::Key(KeyCode::Enter) },
    ActionDef { id: "wheelup", label: "Wheel +", out: Out::Wheel(1.0) },
    ActionDef { id: "wheeldown", label: "Wheel -", out: Out::Wheel(-1.0) },
    ActionDef { id: "previtem", label: "Prev item", out: Out::Key(KeyCode::BracketLeft) },
    ActionDef { id: "nextitem", label: "Next item", out: Out::Key(KeyCode::BracketRight) },
    ActionDef { id: "talk", label: "Talk", out: Out::Key(KeyCode::KeyB) },
    ActionDef { id: "dance", label: "Dance", out: Out::Key(KeyCode::KeyH) },
    ActionDef { id: "scream", label: "Scream", out: Out::Key(KeyCode::KeyJ) },
    ActionDef { id: "getdown", label: "Get down", out: Out::Key(KeyCode::KeyY) },
    ActionDef { id: "unzip", label: "Unzip", out: Out::Key(KeyCode::KeyR) },
];

fn action_index(id: &str) -> Option<usize> {
    ACTIONS.iter().position(|a| a.id == id)
}

// ----------------------------------------------------------------------------------------------
// Layout (saved) and the editor state.

#[derive(Clone, Debug, PartialEq)]
struct Control {
    /// The movement stick (WASD) instead of a button.
    stick: bool,
    /// Index into `ACTIONS` (buttons only).
    action: usize,
    /// Centre, as a fraction of the window width and height.
    x: f32,
    y: f32,
    /// Diameter as a fraction of the window's shorter side.
    size: f32,
    alpha: f32,
}

impl Control {
    fn label(&self) -> &'static str {
        if self.stick { "Move" } else { ACTIONS[self.action].label }
    }
}

fn default_layout() -> Vec<Control> {
    let button = |id: &str, x: f32, y: f32, size: f32| Control { stick: false, action: action_index(id).unwrap_or(0), x, y, size, alpha: 0.55 };
    vec![
        Control { stick: true, action: 0, x: 0.12, y: 0.66, size: 0.36, alpha: 0.45 },
        button("fire", 0.90, 0.72, 0.22),
        button("altfire", 0.75, 0.88, 0.14),
        button("reload", 0.60, 0.88, 0.12),
        button("jump", 0.90, 0.40, 0.16),
        button("crouch", 0.75, 0.56, 0.13),
        button("use", 0.60, 0.50, 0.13),
        button("holster", 0.78, 0.30, 0.11),
        button("menu", 0.06, 0.12, 0.10),
        button("inventory", 0.18, 0.12, 0.10),
        button("map", 0.30, 0.12, 0.10),
        button("health", 0.42, 0.12, 0.10),
    ]
}

#[derive(Resource)]
struct TouchLayout {
    items: Vec<Control>,
}

#[derive(Resource)]
struct TouchEditor {
    /// Controls can be moved and resized; touches do not reach the game.
    editing: bool,
    /// The editor panel (below the gear button) is open.
    panel: bool,
    selected: Option<usize>,
    /// The UI must be rebuilt next frame.
    dirty: bool,
    last_size: (f32, f32),
}

fn layout_path() -> Option<PathBuf> {
    uscript::config::default_user_dir().map(|d| d.join("touch_layout.txt"))
}

fn load_layout() -> Option<Vec<Control>> {
    let text = std::fs::read_to_string(layout_path()?).ok()?;
    let mut items = Vec::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() != 5 {
            continue;
        }
        let nums: Vec<f32> = f[1..].iter().filter_map(|s| s.parse().ok()).collect();
        if nums.len() != 4 {
            continue;
        }
        let (stick, action) = if f[0] == "stick" {
            (true, 0)
        } else {
            match action_index(f[0]) {
                Some(a) => (false, a),
                None => continue,
            }
        };
        items.push(Control {
            stick,
            action,
            x: nums[0].clamp(0.0, 1.0),
            y: nums[1].clamp(0.0, 1.0),
            size: nums[2].clamp(0.05, 0.7),
            alpha: nums[3].clamp(0.1, 1.0),
        });
    }
    (!items.is_empty()).then_some(items)
}

fn save_layout(layout: &TouchLayout) {
    let Some(path) = layout_path() else { return };
    let mut s = String::from("# POSTAL 2 Rust/Bevy touch layout: action x y size opacity (x/y: centre as a fraction of the screen)\n");
    for c in &layout.items {
        let id = if c.stick { "stick" } else { ACTIONS[c.action].id };
        s += &format!("{id} {:.4} {:.4} {:.4} {:.2}\n", c.x, c.y, c.size, c.alpha);
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(&path, s) {
        eprintln!("touch: cannot save {}: {e}", path.display());
    }
}

// ----------------------------------------------------------------------------------------------
// Geometry (logical pixels).

#[derive(Clone, Copy, Debug)]
struct Area {
    min: Vec2,
    max: Vec2,
}

impl Area {
    fn new(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Area { min: Vec2::new(x0, y0), max: Vec2::new(x1, y1) }
    }
    fn contains(&self, p: Vec2) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }
}

/// Centre and radius of a control in logical pixels.
fn geom(c: &Control, w: f32, h: f32) -> (Vec2, f32) {
    (Vec2::new(c.x * w, c.y * h), (c.size * w.min(h) * 0.5).max(14.0))
}

/// Topmost control under `p` (later controls are drawn above earlier ones).
fn hit_control(items: &[Control], w: f32, h: f32, p: Vec2) -> Option<usize> {
    (0..items.len()).rev().find(|&i| {
        let (c, r) = geom(&items[i], w, h);
        (p - c).length() <= r * 1.05
    })
}

/// The corner handle that resizes a control in the editor.
fn handle_pos(c: Vec2, r: f32) -> Vec2 {
    c + Vec2::new(r * 0.707, r * 0.707)
}

fn gear_area(w: f32) -> Area {
    Area::new(w - 64.0, 12.0, w - 12.0, 64.0)
}

/// The mouse-mode button, just left of the gear button.
fn mouse_area(w: f32) -> Area {
    Area::new(w - 150.0, 12.0, w - 72.0, 64.0)
}

/// The mouse-mode cursor: a ring drawn over everything, moved by `paint`.
#[derive(Component)]
struct TouchCursor;

fn spawn_cursor(mut commands: Commands) {
    commands.spawn((
        TouchCursor,
        Node {
            position_type: PositionType::Absolute,
            left: px(0.0),
            top: px(0.0),
            width: px(20.0),
            height: px(20.0),
            border: UiRect::all(px(2.0)),
            border_radius: BorderRadius::MAX,
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.25)),
        BorderColor::all(ACCENT),
        GlobalZIndex(Z + 2),
        Visibility::Hidden,
    ));
}

fn panel_width(w: f32) -> f32 {
    (w * 0.32).clamp(220.0, 300.0)
}

fn panel_bg(w: f32, h: f32) -> Area {
    let pw = panel_width(w);
    Area::new(w - pw - 18.0, 68.0, w - 6.0, (68.0_f32 + 330.0).min(h - 6.0))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PanelAct {
    Hint,
    Toggle,
    Add,
    SizeDown,
    SizeUp,
    AlphaDown,
    AlphaUp,
    PrevAct,
    ActName,
    NextAct,
    Delete,
    Reset,
    Done,
}

impl PanelAct {
    fn clickable(self) -> bool {
        !matches!(self, PanelAct::Hint | PanelAct::ActName)
    }
}

struct Row {
    act: PanelAct,
    label: String,
    area: Area,
    active: bool,
}

/// The editor panel's rows, top to bottom. Fractions are of the panel width.
fn panel_rows(ed: &TouchEditor, layout: &TouchLayout, w: f32, h: f32) -> Vec<Row> {
    let bg = panel_bg(w, h);
    let (x0, x1) = (bg.min.x + 8.0, bg.max.x - 8.0);
    let pw = x1 - x0;
    let mut out = Vec::new();
    let mut y = bg.min.y + 8.0;
    let sel = ed.selected.and_then(|i| layout.items.get(i));
    let hint = match sel {
        Some(c) => format!("Selected: {}", c.label()),
        None => "Tap a control to select it".to_string(),
    };
    out.push(Row { act: PanelAct::Hint, label: hint, area: Area::new(x0, y, x1, y + 26.0), active: false });
    y += 32.0;
    let mut line = |y: &mut f32, cells: &[(PanelAct, String, f32, f32, bool)]| {
        let h = 36.0;
        for (act, label, a, b, active) in cells {
            out.push(Row { act: *act, label: label.clone(), area: Area::new(x0 + pw * a, *y, x0 + pw * b - 4.0, *y + h), active: *active });
        }
        *y += h + 6.0;
    };
    line(&mut y, &[(PanelAct::Toggle, format!("Edit mode: {}", if ed.editing { "ON" } else { "OFF" }), 0.0, 1.0, ed.editing)]);
    line(&mut y, &[(PanelAct::Add, "+ Add control".into(), 0.0, 1.0, false)]);
    line(&mut y, &[(PanelAct::SizeDown, "Size -".into(), 0.0, 0.5, false), (PanelAct::SizeUp, "Size +".into(), 0.5, 1.0, false)]);
    line(&mut y, &[(PanelAct::AlphaDown, "Opacity -".into(), 0.0, 0.5, false), (PanelAct::AlphaUp, "Opacity +".into(), 0.5, 1.0, false)]);
    let action_name = sel.map(|c| c.label().to_string()).unwrap_or_else(|| "-".into());
    line(
        &mut y,
        &[(PanelAct::PrevAct, "<".into(), 0.0, 0.25, false), (PanelAct::ActName, action_name, 0.25, 0.75, false), (PanelAct::NextAct, ">".into(), 0.75, 1.0, false)],
    );
    line(&mut y, &[(PanelAct::Delete, "Delete control".into(), 0.0, 1.0, false)]);
    line(&mut y, &[(PanelAct::Reset, "Reset layout".into(), 0.0, 1.0, false)]);
    line(&mut y, &[(PanelAct::Done, "Save and close".into(), 0.0, 1.0, true)]);
    out
}

// ----------------------------------------------------------------------------------------------
// Touch routing.

#[derive(Clone, Copy, Debug)]
enum Role {
    /// Finger on the movement stick.
    Stick { idx: usize, center: Vec2, radius: f32 },
    /// Finger holding a button.
    Button { idx: usize, out: Out },
    /// Finger on empty screen: a look drag once it moves, a click if it does not.
    Pointer { start: Vec2, last: Vec2, moved: bool, t0: Instant },
    /// Mouse mode: the finger drags the cursor; a tap clicks where the cursor is.
    Trackpad { last: Vec2, moved: bool, t0: Instant },
    /// Editor: dragging a control (`grab` = finger minus centre).
    Move { idx: usize, grab: Vec2 },
    /// Editor: dragging the corner handle.
    Resize { idx: usize },
    /// Panel or editor tap that does nothing to the game.
    Ignore,
}

#[derive(Resource, Default)]
struct Fingers {
    roles: HashMap<u64, Role>,
}

#[derive(Resource, Default)]
struct Held {
    /// Mouse mode: a visible cursor that one finger drags and a tap clicks (for menus).
    mouse: bool,
    /// Mouse-mode cursor position, in logical pixels.
    cursor: Vec2,
    /// Stick knob per control: (shown offset, target offset), in logical pixels.
    knobs: HashMap<usize, (Vec2, Vec2)>,
    /// WASD keys the stick currently holds.
    keys: HashSet<KeyCode>,
    /// Controls drawn as pressed.
    pressed: HashSet<usize>,
    /// Deferred releases: (frame, event). Taps press for one frame and release later.
    pending: Vec<(u32, Emit)>,
    frame: u32,
}

#[derive(Clone, Copy, Debug)]
enum Emit {
    Key(KeyCode, bool),
    Mouse(MouseButton, bool),
    Wheel(f32),
    Motion(Vec2),
}

/// Writes the Bevy messages for `Emit` values.
struct Sink<'a> {
    win: Entity,
    keys: &'a mut Messages<KeyboardInput>,
    buttons: &'a mut Messages<MouseButtonInput>,
    motion: &'a mut Messages<MouseMotion>,
    wheel: &'a mut Messages<MouseWheel>,
}

impl Sink<'_> {
    fn send(&mut self, e: Emit) {
        match e {
            Emit::Key(k, down) => {
                self.keys.write(KeyboardInput {
                    key_code: k,
                    logical_key: Key::Unidentified(NativeKey::Unidentified),
                    state: if down { ButtonState::Pressed } else { ButtonState::Released },
                    text: None,
                    repeat: false,
                    window: self.win,
                });
            }
            Emit::Mouse(b, down) => {
                self.buttons.write(MouseButtonInput {
                    button: b,
                    state: if down { ButtonState::Pressed } else { ButtonState::Released },
                    window: self.win,
                });
            }
            Emit::Wheel(y) => {
                self.wheel.write(MouseWheel { unit: MouseScrollUnit::Line, x: 0.0, y, window: self.win, phase: TouchPhase::Moved });
            }
            Emit::Motion(delta) => {
                self.motion.write(MouseMotion { delta });
            }
        }
    }
}

/// What pressing (`down`) or lifting an output sends. Wheel steps are one-shot.
fn out_emit(out: Out, down: bool) -> Option<Emit> {
    match out {
        Out::Key(k) => Some(Emit::Key(k, down)),
        Out::Mouse(b) => Some(Emit::Mouse(b, down)),
        Out::Wheel(v) => down.then_some(Emit::Wheel(v)),
    }
}

/// Applies an editor button. Layout changes are saved straight away.
fn apply_panel(act: PanelAct, ed: &mut TouchEditor, layout: &mut TouchLayout) {
    let sel = ed.selected.filter(|&i| i < layout.items.len());
    match act {
        PanelAct::Hint | PanelAct::ActName => return,
        PanelAct::Toggle => {
            ed.editing = !ed.editing;
            if !ed.editing {
                ed.selected = None;
            }
        }
        PanelAct::Add => {
            layout.items.push(Control { stick: false, action: action_index("fire").unwrap_or(0), x: 0.5, y: 0.5, size: 0.14, alpha: 0.55 });
            ed.selected = Some(layout.items.len() - 1);
            ed.editing = true;
        }
        PanelAct::SizeDown | PanelAct::SizeUp => {
            if let Some(c) = sel.map(|i| &mut layout.items[i]) {
                let f = if act == PanelAct::SizeUp { 1.1 } else { 0.9 };
                c.size = (c.size * f).clamp(0.05, 0.7);
            }
        }
        PanelAct::AlphaDown | PanelAct::AlphaUp => {
            if let Some(c) = sel.map(|i| &mut layout.items[i]) {
                let d = if act == PanelAct::AlphaUp { 0.1 } else { -0.1 };
                c.alpha = (c.alpha + d).clamp(0.1, 1.0);
            }
        }
        PanelAct::PrevAct | PanelAct::NextAct => {
            if let Some(c) = sel.map(|i| &mut layout.items[i]).filter(|c| !c.stick) {
                let n = ACTIONS.len() as i32;
                let d = if act == PanelAct::NextAct { 1 } else { -1 };
                c.action = (c.action as i32 + d).rem_euclid(n) as usize;
            }
        }
        PanelAct::Delete => {
            if let Some(i) = sel {
                layout.items.remove(i);
            }
            ed.selected = None;
        }
        PanelAct::Reset => {
            layout.items = default_layout();
            ed.selected = None;
        }
        PanelAct::Done => {
            ed.editing = false;
            ed.panel = false;
            ed.selected = None;
        }
    }
    ed.dirty = true;
    save_layout(layout);
}

/// Pops the deferred releases that are due.
fn due_pending(held: &mut Held) -> Vec<Emit> {
    let frame = held.frame;
    let mut due = Vec::new();
    held.pending.retain(|(f, e)| {
        if *f <= frame {
            due.push(*e);
            false
        } else {
            true
        }
    });
    due
}

fn route(
    mut touches: MessageReader<TouchInput>,
    mut windows: Query<(Entity, &mut Window), With<PrimaryWindow>>,
    mut layout: ResMut<TouchLayout>,
    mut editor: ResMut<TouchEditor>,
    mut fingers: ResMut<Fingers>,
    mut held: ResMut<Held>,
    mut keys: ResMut<Messages<KeyboardInput>>,
    mut buttons: ResMut<Messages<MouseButtonInput>>,
    mut motion: ResMut<Messages<MouseMotion>>,
    mut wheel: ResMut<Messages<MouseWheel>>,
) {
    let Ok((win_e, mut win)) = windows.single_mut() else { return };
    let (w, h) = (win.width(), win.height());
    held.frame += 1;
    let mut sink = Sink { win: win_e, keys: &mut *keys, buttons: &mut *buttons, motion: &mut *motion, wheel: &mut *wheel };
    for e in due_pending(&mut held) {
        sink.send(e);
    }
    let touches: Vec<TouchInput> = touches.read().copied().collect();
    for t in touches {
        let p = t.position;
        match t.phase {
            TouchPhase::Started => {
                let role = if gear_area(w).contains(p) {
                    editor.panel = !editor.panel;
                    if !editor.panel {
                        editor.editing = false;
                        editor.selected = None;
                        save_layout(&layout);
                    }
                    editor.dirty = true;
                    Role::Ignore
                } else if editor.panel && panel_bg(w, h).contains(p) {
                    let hit = panel_rows(&editor, &layout, w, h).into_iter().find(|r| r.act.clickable() && r.area.contains(p)).map(|r| r.act);
                    if let Some(act) = hit {
                        apply_panel(act, &mut editor, &mut layout);
                    }
                    Role::Ignore
                } else if !editor.editing && mouse_area(w).contains(p) {
                    held.mouse = !held.mouse;
                    if held.mouse {
                        held.cursor = Vec2::new(w, h) * 0.5;
                        win.set_cursor_position(Some(held.cursor));
                    }
                    editor.dirty = true;
                    Role::Ignore
                } else if editor.editing {
                    match hit_control(&layout.items, w, h, p) {
                        Some(idx) => {
                            editor.selected = Some(idx);
                            editor.dirty = true;
                            let (c, r) = geom(&layout.items[idx], w, h);
                            if (p - handle_pos(c, r)).length() < 30.0 { Role::Resize { idx } } else { Role::Move { idx, grab: p - c } }
                        }
                        None => {
                            editor.selected = None;
                            editor.dirty = true;
                            Role::Ignore
                        }
                    }
                } else if let Some(idx) = hit_control(&layout.items, w, h, p) {
                    let item = layout.items[idx].clone();
                    held.pressed.insert(idx);
                    if item.stick {
                        let (center, radius) = geom(&item, w, h);
                        held.knobs.entry(idx).or_default().1 = Vec2::ZERO;
                        Role::Stick { idx, center, radius }
                    } else {
                        let out = ACTIONS[item.action].out;
                        if let Some(e) = out_emit(out, true) {
                            sink.send(e);
                        }
                        Role::Button { idx, out }
                    }
                } else if held.mouse {
                    Role::Trackpad { last: p, moved: false, t0: Instant::now() }
                } else {
                    Role::Pointer { start: p, last: p, moved: false, t0: Instant::now() }
                };
                fingers.roles.insert(t.id, role);
            }
            TouchPhase::Moved => {
                let Some(role) = fingers.roles.get_mut(&t.id) else { continue };
                match role {
                    Role::Stick { idx, center, radius } => {
                        let raw = p - *center;
                        held.knobs.entry(*idx).or_default().1 = raw.clamp_length_max(*radius * 0.6);
                        let d = raw / radius.max(1.0);
                        let th = 0.3;
                        let mut want = HashSet::new();
                        if d.x > th {
                            want.insert(KeyCode::KeyD);
                        } else if d.x < -th {
                            want.insert(KeyCode::KeyA);
                        }
                        if d.y < -th {
                            want.insert(KeyCode::KeyW);
                        } else if d.y > th {
                            want.insert(KeyCode::KeyS);
                        }
                        let release: Vec<KeyCode> = held.keys.difference(&want).copied().collect();
                        for k in release {
                            held.keys.remove(&k);
                            sink.send(Emit::Key(k, false));
                        }
                        let press: Vec<KeyCode> = want.difference(&held.keys).copied().collect();
                        for k in press {
                            held.keys.insert(k);
                            sink.send(Emit::Key(k, true));
                        }
                    }
                    Role::Pointer { start, last, moved, .. } => {
                        if !*moved && (p - *start).length() > 12.0 {
                            *moved = true;
                        }
                        if *moved && !editor.editing {
                            sink.send(Emit::Motion(p - *last));
                        }
                        *last = p;
                    }
                    Role::Trackpad { last, moved, .. } => {
                        if !*moved && (p - *last).length() > 8.0 {
                            *moved = true;
                        }
                        let next = (held.cursor + (p - *last) * 1.4).clamp(Vec2::ZERO, Vec2::new(w, h));
                        held.cursor = next;
                        win.set_cursor_position(Some(next));
                        *last = p;
                    }
                    Role::Move { idx, grab } => {
                        if let Some(c) = layout.items.get_mut(*idx) {
                            let np = p - *grab;
                            c.x = (np.x / w.max(1.0)).clamp(0.02, 0.98);
                            c.y = (np.y / h.max(1.0)).clamp(0.02, 0.98);
                            editor.dirty = true;
                        }
                    }
                    Role::Resize { idx } => {
                        if let Some(c) = layout.items.get_mut(*idx) {
                            let (centre, _) = geom(c, w, h);
                            c.size = ((p - centre).length() * 2.0 / w.min(h).max(1.0)).clamp(0.05, 0.7);
                            editor.dirty = true;
                        }
                    }
                    Role::Button { .. } | Role::Ignore => {}
                }
            }
            TouchPhase::Ended | TouchPhase::Canceled => {
                let Some(role) = fingers.roles.remove(&t.id) else { continue };
                match role {
                    Role::Stick { idx, .. } => {
                        held.knobs.entry(idx).or_default().1 = Vec2::ZERO;
                        let keys: Vec<KeyCode> = held.keys.drain().collect();
                        for k in keys {
                            sink.send(Emit::Key(k, false));
                        }
                        held.pressed.remove(&idx);
                    }
                    Role::Button { idx, out } => {
                        if let Some(e) = out_emit(out, false) {
                            sink.send(e);
                        }
                        held.pressed.remove(&idx);
                    }
                    Role::Pointer { moved, t0, .. } => {
                        let on_panel = editor.panel && panel_bg(w, h).contains(p);
                        if !moved && t0.elapsed().as_millis() < 450 && !editor.editing && !on_panel {
                            // A tap: move the cursor there (menus read it) and click.
                            win.set_cursor_position(Some(p));
                            sink.send(Emit::Mouse(MouseButton::Left, true));
                            let release_at = held.frame + 2;
                            held.pending.push((release_at, Emit::Mouse(MouseButton::Left, false)));
                        }
                    }
                    Role::Trackpad { moved, t0, .. } => {
                        if !moved && t0.elapsed().as_millis() < 450 {
                            // A tap clicks where the cursor is (menus read the window's cursor).
                            win.set_cursor_position(Some(held.cursor));
                            sink.send(Emit::Mouse(MouseButton::Left, true));
                            let release_at = held.frame + 2;
                            held.pending.push((release_at, Emit::Mouse(MouseButton::Left, false)));
                        }
                    }
                    Role::Move { .. } | Role::Resize { .. } => save_layout(&layout),
                    Role::Ignore => {}
                }
            }
        }
    }
}

// ----------------------------------------------------------------------------------------------
// UI: the gear button, the controls, the editor panel. Rebuilt when something changes.

const Z: i32 = i32::MAX - 10;
const ACCENT: Color = Color::srgb(0.93, 0.26, 0.21);
const TEXT: Color = Color::srgb(0.92, 0.93, 0.95);
const PANEL: Color = Color::srgba(0.055, 0.06, 0.07, 0.92);
const ROW: Color = Color::srgba(0.16, 0.17, 0.20, 0.96);
const SELECTED: Color = Color::srgb(1.0, 0.85, 0.2);

#[derive(Component)]
struct TouchUi;

#[derive(Component)]
struct CtrlNode(usize);

/// The knob inside the stick's base.
#[derive(Component)]
struct StickKnob(usize);

fn font(px: f32) -> TextFont {
    TextFont { font_size: bevy::text::FontSize::Px(px), ..default() }
}

fn rebuild(
    mut commands: Commands,
    existing: Query<Entity, With<TouchUi>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    layout: Res<TouchLayout>,
    held: Res<Held>,
    mut editor: ResMut<TouchEditor>,
) {
    let Ok(win) = windows.single() else { return };
    let (w, h) = (win.width(), win.height());
    if w < 1.0 || h < 1.0 {
        return;
    }
    if (w, h) != editor.last_size {
        editor.last_size = (w, h);
        editor.dirty = true;
    }
    if !editor.dirty {
        return;
    }
    editor.dirty = false;
    for e in &existing {
        commands.entity(e).despawn();
    }
    // Gear button.
    let g = gear_area(w);
    commands
        .spawn((
            TouchUi,
            Node {
                position_type: PositionType::Absolute,
                left: px(g.min.x),
                top: px(g.min.y),
                width: px(g.max.x - g.min.x),
                height: px(g.max.y - g.min.y),
                border: UiRect::all(px(1.0)),
                border_radius: BorderRadius::MAX,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(if editor.panel { ACCENT } else { Color::srgba(0.1, 0.11, 0.13, 0.6) }),
            BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.2)),
            GlobalZIndex(Z),
        ))
        .with_children(|p| {
            p.spawn((Text::new("UI"), font(15.0), TextColor(TEXT)));
        });
    // Mouse-mode button.
    if !editor.editing {
        let m = mouse_area(w);
        commands
            .spawn((
                TouchUi,
                Node {
                    position_type: PositionType::Absolute,
                    left: px(m.min.x),
                    top: px(m.min.y),
                    width: px(m.max.x - m.min.x),
                    height: px(m.max.y - m.min.y),
                    border: UiRect::all(px(1.0)),
                    border_radius: BorderRadius::MAX,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(if held.mouse { ACCENT } else { Color::srgba(0.1, 0.11, 0.13, 0.6) }),
                BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.2)),
                GlobalZIndex(Z),
            ))
            .with_children(|p| {
                p.spawn((Text::new("MOUSE"), font(13.0), TextColor(TEXT)));
            });
    }
    // Controls.
    for (i, c) in layout.items.iter().enumerate() {
        let (centre, r) = geom(c, w, h);
        let label = c.label().to_string();
        commands
            .spawn((
                TouchUi,
                CtrlNode(i),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(centre.x - r),
                    top: px(centre.y - r),
                    width: px(r * 2.0),
                    height: px(r * 2.0),
                    border: UiRect::all(px(2.0)),
                    border_radius: BorderRadius::MAX,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(Color::srgba(0.1, 0.12, 0.15, c.alpha)),
                BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.12)),
                GlobalZIndex(Z - 1),
            ))
            .with_children(|p| {
                if c.stick {
                    // The stick's knob: a child of the fixed base, moved by `paint`.
                    p.spawn((
                        StickKnob(i),
                        Node {
                            position_type: PositionType::Absolute,
                            left: percent(22.5),
                            top: percent(22.5),
                            width: percent(55.0),
                            height: percent(55.0),
                            border: UiRect::all(px(2.0)),
                            border_radius: BorderRadius::MAX,
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.93, 0.93, 0.95, 0.5)),
                        BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.7)),
                    ));
                } else {
                    p.spawn((Text::new(label), font((r * 0.42).clamp(10.0, 24.0)), TextColor(TEXT)));
                }
            });
        if editor.editing {
            let hp = handle_pos(centre, r);
            commands.spawn((
                TouchUi,
                Node {
                    position_type: PositionType::Absolute,
                    left: px(hp.x - 11.0),
                    top: px(hp.y - 11.0),
                    width: px(22.0),
                    height: px(22.0),
                    border_radius: BorderRadius::all(px(4.0)),
                    ..default()
                },
                BackgroundColor(if editor.selected == Some(i) { SELECTED } else { Color::srgba(1.0, 1.0, 1.0, 0.7) }),
                GlobalZIndex(Z),
            ));
        }
    }
    if editor.editing {
        commands.spawn((
            TouchUi,
            Node { position_type: PositionType::Absolute, left: px(12.0), top: px(12.0), padding: UiRect::all(px(6.0)), border_radius: BorderRadius::all(px(4.0)), ..default() },
            BackgroundColor(PANEL),
            GlobalZIndex(Z),
        )).with_children(|p| {
            p.spawn((Text::new("EDIT: drag to move, corner handle to resize, Save and close when done"), font(13.0), TextColor(SELECTED)));
        });
    }
    if editor.panel {
        let bg = panel_bg(w, h);
        commands.spawn((
            TouchUi,
            Node {
                position_type: PositionType::Absolute,
                left: px(bg.min.x),
                top: px(bg.min.y),
                width: px(bg.max.x - bg.min.x),
                height: px(bg.max.y - bg.min.y),
                border_radius: BorderRadius::all(px(6.0)),
                ..default()
            },
            BackgroundColor(PANEL),
            GlobalZIndex(Z),
        ));
        for row in panel_rows(&editor, &layout, w, h) {
            let a = row.area;
            let bgc = match (row.act, row.active) {
                (PanelAct::Hint | PanelAct::ActName, _) => Color::NONE,
                (_, true) => ACCENT,
                _ => ROW,
            };
            commands
                .spawn((
                    TouchUi,
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(a.min.x),
                        top: px(a.min.y),
                        width: px(a.max.x - a.min.x),
                        height: px(a.max.y - a.min.y),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border_radius: BorderRadius::all(px(5.0)),
                        ..default()
                    },
                    BackgroundColor(bgc),
                    GlobalZIndex(Z + 1),
                ))
                .with_children(|p| {
                    p.spawn((Text::new(row.label), font(15.0), TextColor(TEXT)));
                });
        }
    }
}

/// Button colours follow the touches (pressed = accent) and the editor's selection.
fn paint(
    mut held: ResMut<Held>,
    layout: Res<TouchLayout>,
    editor: Res<TouchEditor>,
    time: Res<Time>,
    mut q: Query<(&CtrlNode, &mut BackgroundColor, &mut BorderColor)>,
    mut knobs: Query<(&StickKnob, &mut Node), Without<TouchCursor>>,
    mut cursor: Query<(&mut Node, &mut Visibility), With<TouchCursor>>,
) {
    // The knob eases toward its target (finger, or the centre on release): a spring feel.
    let ease = 1.0 - (-time.delta_secs() * 25.0).exp();
    for (knob, mut node) in &mut knobs {
        let entry = held.knobs.entry(knob.0).or_default();
        entry.0 = entry.0.lerp(entry.1, ease);
        let off = entry.0;
        node.margin = UiRect { left: px(off.x), top: px(off.y), ..default() };
    }
    if let Ok((mut node, mut vis)) = cursor.single_mut() {
        *vis = if held.mouse { Visibility::Visible } else { Visibility::Hidden };
        node.left = px(held.cursor.x - 10.0);
        node.top = px(held.cursor.y - 10.0);
    }
    for (node, mut bg, mut border) in &mut q {
        let Some(c) = layout.items.get(node.0) else { continue };
        let pressed = held.pressed.contains(&node.0);
        bg.0 = if pressed {
            Color::srgba(0.93, 0.26, 0.21, (c.alpha + 0.25).min(1.0))
        } else if c.stick {
            Color::srgba(0.18, 0.20, 0.24, c.alpha)
        } else {
            Color::srgba(0.10, 0.12, 0.15, c.alpha)
        };
        let sel = editor.editing && editor.selected == Some(node.0);
        *border = BorderColor::all(if sel { SELECTED } else { Color::srgba(1.0, 1.0, 1.0, 0.12) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_layout_maps_known_actions() {
        let l = default_layout();
        assert!(l.iter().filter(|c| c.stick).count() == 1);
        assert!(l.iter().filter(|c| !c.stick).all(|c| c.action < ACTIONS.len()));
    }

    #[test]
    fn action_ids_are_unique() {
        for (i, a) in ACTIONS.iter().enumerate() {
            assert!(ACTIONS[i + 1..].iter().all(|b| b.id != a.id), "duplicate id {}", a.id);
        }
    }

    #[test]
    fn hit_test_prefers_later_controls() {
        let mut items = default_layout();
        items.push(Control { stick: false, action: 0, x: 0.5, y: 0.5, size: 0.2, alpha: 0.5 });
        let (c, _) = geom(items.last().unwrap(), 800.0, 400.0);
        assert_eq!(hit_control(&items, 800.0, 400.0, c), Some(items.len() - 1));
    }

    #[test]
    fn panel_rows_fit_the_panel() {
        let ed = TouchEditor { editing: true, panel: true, selected: None, dirty: true, last_size: (0.0, 0.0) };
        let layout = TouchLayout { items: default_layout() };
        let bg = panel_bg(900.0, 500.0);
        for r in panel_rows(&ed, &layout, 900.0, 500.0) {
            assert!(r.area.max.y <= bg.max.y + 1.0, "{:?} overflows the panel", r.act);
        }
    }
}
