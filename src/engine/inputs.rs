//! GUI inputs: `textbox`, `slider`, `checkbox` and `dropdown`. With name="volume" the value lives in the
//! variable `volume`, so scripts read it directly and `change volume to 0.5` moves the slider.
//! A `then` handler runs when the value changes (for a textbox: when Enter is pressed).
use super::*;
use bevy::input::keyboard::{Key, KeyboardInput};

#[derive(Clone, Copy, PartialEq)]
enum InputKind {
    Text,
    Slider,
    Check,
    Drop,
}

type Key2 = (String, Vec<usize>);

/// An unnamed input's own value (components must be thread-safe, so not an Eza Value)
#[derive(Clone)]
enum Plain {
    Text(String),
    Num(f64),
    Bool(bool),
}

impl Plain {
    fn of(v: &Value) -> Plain {
        match v {
            Value::Num(n) => Plain::Num(*n),
            Value::Bool(b) => Plain::Bool(*b),
            other => Plain::Text(other.display()),
        }
    }
    fn value(&self) -> Value {
        match self {
            Plain::Text(s) => Value::Str(s.clone()),
            Plain::Num(n) => Value::Num(*n),
            Plain::Bool(b) => Value::Bool(*b),
        }
    }
}

#[derive(Component)]
pub(super) struct GuiInput {
    key: Key2,
    kind: InputKind,
    /// the variable holding the value ("" = no name: the value is kept here in `local`)
    var: String,
    local: Plain,
    text: Option<Entity>,
    knob: Option<Entity>,
    fill: Option<Entity>,
    mark: Option<Entity>,
    min: f64,
    max: f64,
    step: f64,
    options: Vec<String>,
    placeholder: String,
    color: Color,
    char_w: f32,
    width: f32,
    max_len: usize,
    font: TextFont,
}

#[derive(Component)]
pub(super) struct DropOption {
    key: Key2,
    value: String,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_input(
    commands: &mut Commands,
    e: Entity,
    o: &Obj,
    kind: &str,
    root: &str,
    path: &[usize],
    w: f32,
    text: Color,
    accent: Color,
    font: TextFont,
) -> GuiInput {
    let n = |k: &str, d: f64| o.get(k).and_then(|v| v.as_num(0).ok()).unwrap_or(d);
    let options: Vec<String> = match o.get("options") {
        Some(v) => parts(v).iter().map(|x| x.display()).collect(),
        None => vec![],
    };
    let (min, max) = (n("min", 0.0), n("max", 100.0));
    let label = o.get("label").map(|l| l.display()).unwrap_or_default();
    let kind = match kind {
        "textbox" => InputKind::Text,
        "slider" => InputKind::Slider,
        "checkbox" => InputKind::Check,
        _ => InputKind::Drop,
    };
    let local = match kind {
        InputKind::Text => Value::Str(o.get("text").or(o.get("value")).map(|v| v.display()).unwrap_or_default()),
        InputKind::Slider => Value::Num(n("value", min)),
        InputKind::Check => Value::Bool(o.get("checked").or(o.get("value")).is_some_and(|v| v.truthy())),
        InputKind::Drop => Value::Str(o.get("value").map(|v| v.display()).or(options.first().cloned()).unwrap_or_default()),
    };
    let mut inp = GuiInput {
        key: (root.to_string(), path.to_vec()),
        kind,
        var: match o.get("name") {
            Some(Value::Str(s)) => s.clone(),
            _ => String::new(),
        },
        local: Plain::of(&local),
        text: None,
        knob: None,
        fill: None,
        mark: None,
        min,
        max,
        step: n("step", (max - min).abs() / 100.0),
        options,
        placeholder: o.get("placeholder").map(|p| p.display()).unwrap_or(label.clone()),
        color: text,
        char_w: 0.6 * font.font_size,
        width: w,
        max_len: n("max_length", 200.0) as usize,
        font: font.clone(),
    };
    let abs = |left: Val, top: Val, width: Val, height: Val| Node { position_type: PositionType::Absolute, left, top, width, height, ..default() };
    match kind {
        InputKind::Text => {
            let t = commands.spawn((Text::new(""), TextColor(text), font, TextLayout::new_with_no_wrap())).id();
            commands.entity(e).add_child(t);
            inp.text = Some(t);
        }
        InputKind::Slider => {
            // the track is inset by the knob's radius so the knob never sticks out
            let inner = commands.spawn(Node { position_type: PositionType::Absolute, left: Val::Px(8.0), right: Val::Px(8.0), top: Val::Px(0.0), bottom: Val::Px(0.0), ..default() }).id();
            let bar = |c: Color, width: Val| (
                Node { margin: UiRect::top(Val::Px(-3.0)), ..abs(Val::Px(0.0), Val::Percent(50.0), width, Val::Px(6.0)) },
                BackgroundColor(c),
                BorderRadius::all(Val::Px(3.0)),
            );
            let track = commands.spawn(bar(Color::srgb(0.3, 0.3, 0.34), Val::Percent(100.0))).id();
            let fill = commands.spawn(bar(accent, Val::Percent(0.0))).id();
            let knob = commands
                .spawn((
                    Node { margin: UiRect { left: Val::Px(-8.0), top: Val::Px(-8.0), ..default() }, ..abs(Val::Percent(0.0), Val::Percent(50.0), Val::Px(16.0), Val::Px(16.0)) },
                    BackgroundColor(Color::WHITE),
                    BorderRadius::all(Val::Px(8.0)),
                ))
                .id();
            commands.entity(inner).add_children(&[track, fill, knob]);
            commands.entity(e).add_child(inner);
            (inp.fill, inp.knob) = (Some(fill), Some(knob));
        }
        InputKind::Check => {
            let size = crate::layout::CHECK_BOX as f32;
            let bx = commands
                .spawn((
                    Node {
                        margin: UiRect::top(Val::Px(-size / 2.0)),
                        border: UiRect::all(Val::Px(2.0)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..abs(Val::Px(0.0), Val::Percent(50.0), Val::Px(size), Val::Px(size))
                    },
                    BorderColor(Color::srgb(0.6, 0.6, 0.65)),
                    BackgroundColor(Color::srgb(0.1, 0.1, 0.12)),
                    BorderRadius::all(Val::Px(4.0)),
                ))
                .id();
            let mark = commands.spawn((Node { width: Val::Px(size / 2.0), height: Val::Px(size / 2.0), ..default() }, BackgroundColor(accent), BorderRadius::all(Val::Px(2.0)), Visibility::Hidden)).id();
            commands.entity(bx).add_child(mark);
            let holder = commands.spawn(Node { position_type: PositionType::Absolute, left: Val::Px(size + 8.0), top: Val::Px(0.0), bottom: Val::Px(0.0), align_items: AlignItems::Center, ..default() }).id();
            let t = commands.spawn((Text::new(label), TextColor(text), font, TextLayout::new_with_no_wrap())).id();
            commands.entity(holder).add_child(t);
            commands.entity(e).add_children(&[bx, holder]);
            inp.mark = Some(mark);
        }
        InputKind::Drop => {
            let t = commands.spawn((Text::new(""), TextColor(text), font.clone(), TextLayout::new_with_no_wrap())).id();
            let arrow = commands.spawn((Text::new("v"), TextColor(text.with_alpha(0.6)), TextFont { font_size: font.font_size * 0.8, ..font })).id();
            commands.entity(e).add_children(&[t, arrow]);
            inp.text = Some(t);
        }
    }
    inp
}

fn get(rt: &Runtime, inp: &GuiInput) -> Value {
    if inp.var.is_empty() {
        return inp.local.value();
    }
    rt.it.global(&inp.var).unwrap_or_else(|| inp.local.value())
}

/// Returns true when the value really changed.
fn set(rt: &mut Runtime, inp: &mut GuiInput, v: Value) -> bool {
    if equals(&get(rt, inp), &v) {
        return false;
    }
    if inp.var.is_empty() {
        inp.local = Plain::of(&v);
    } else {
        rt.it.set_var(&inp.var, v);
    }
    true
}

fn run_handler(rt: &mut Runtime, key: &Key2) {
    let f = rt.it.global(&key.0).and_then(|r| child_at(&r, &key.1)).and_then(|v| match v {
        Value::Obj(o) => o.get("on_click").cloned(),
        _ => None,
    });
    if let Some(f) = f {
        if let Err(e) = rt.it.call_value(f, vec![], vec![], vec![]) {
            if !e.is_switch() {
                eprintln!("{}", e);
            }
        }
    }
}

fn open_list(commands: &mut Commands, inp: &GuiInput, gt: &GlobalTransform, cn: &ComputedNode, scale: f32) -> Entity {
    let size = cn.size() / scale;
    let center = gt.translation().truncate() / scale;
    let row = inp.font.font_size * 1.25 + 10.0;
    let list = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(center.x - size.x / 2.0),
                top: Val::Px(center.y + size.y / 2.0 + 2.0),
                width: Val::Px(size.x),
                flex_direction: FlexDirection::Column,
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(Color::srgb(0.16, 0.16, 0.19)),
            BorderRadius::all(Val::Px(4.0)),
            GlobalZIndex(900),
        ))
        .id();
    for opt in &inp.options {
        let item = commands
            .spawn((
                Button,
                Node { height: Val::Px(row), padding: UiRect::horizontal(Val::Px(10.0)), align_items: AlignItems::Center, ..default() },
                BackgroundColor(Color::srgb(0.16, 0.16, 0.19)),
                DropOption { key: inp.key.clone(), value: opt.clone() },
            ))
            .with_children(|p| {
                p.spawn((Text::new(opt.clone()), TextColor(inp.color), inp.font.clone(), TextLayout::new_with_no_wrap()));
            })
            .id();
        commands.entity(list).add_child(item);
    }
    list
}

#[allow(clippy::too_many_arguments)]
pub(super) fn gui_inputs(
    mut commands: Commands,
    mut rt: NonSendMut<Runtime>,
    mut inputs: Query<(&Interaction, &mut GuiInput, &GlobalTransform, &ComputedNode)>,
    mut opts: Query<(&Interaction, &DropOption, &mut BackgroundColor)>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut typed: EventReader<KeyboardInput>,
    windows: Query<&Window>,
) {
    let events: Vec<KeyboardInput> = typed.read().cloned().collect();
    let rt = &mut *rt;
    if rt.debug || rt.failed || rt.it.go_to.is_some() {
        return;
    }
    let clicked = mouse.just_pressed(MouseButton::Left);
    let window = windows.get_single().ok();
    let cursor = window.and_then(|w| w.physical_cursor_position());
    let scale = window.map_or(1.0, |w| w.scale_factor());

    // an open dropdown list: hover shading and picking an option
    let mut chosen = None;
    for (i, opt, mut bg) in &mut opts {
        let c = if *i == Interaction::None { Color::srgb(0.16, 0.16, 0.19) } else { Color::srgb(0.3, 0.3, 0.36) };
        if bg.0 != c {
            bg.0 = c;
        }
        if clicked && *i != Interaction::None {
            chosen = Some((opt.key.clone(), opt.value.clone()));
        }
    }
    let close_list = |commands: &mut Commands, rt: &mut Runtime| {
        if let Some(l) = rt.dropdown.take() {
            commands.entity(l).despawn_recursive();
        }
    };
    let mut handlers: Vec<Key2> = vec![];
    let mut clicked_input = false;
    let had_list = rt.dropdown.is_some();
    for (i, mut inp, gt, cn) in &mut inputs {
        let hovered = *i != Interaction::None;
        let key = inp.key.clone();
        if let Some((k, v)) = &chosen {
            if *k == key && set(rt, &mut inp, Value::Str(v.clone())) {
                handlers.push(key.clone());
            }
        }
        if clicked && hovered && chosen.is_none() {
            clicked_input = true;
            match inp.kind {
                InputKind::Text => rt.focus = Some(key.clone()),
                InputKind::Slider => rt.drag = Some(key.clone()),
                InputKind::Check => {
                    let v = !get(rt, &inp).truthy();
                    if set(rt, &mut inp, Value::Bool(v)) {
                        handlers.push(key.clone());
                    }
                }
                InputKind::Drop => {
                    close_list(&mut commands, rt);
                    if !had_list {
                        rt.dropdown = Some(open_list(&mut commands, &inp, gt, cn, scale));
                    }
                }
            }
        }
        if inp.kind == InputKind::Slider && rt.drag.as_ref() == Some(&key) {
            if !mouse.pressed(MouseButton::Left) {
                rt.drag = None;
            } else if let Some(c) = cursor {
                let size = cn.size();
                let (left, width) = (gt.translation().x - size.x / 2.0 + 8.0 * scale, (size.x - 16.0 * scale).max(1.0));
                let t = ((c.x - left) / width).clamp(0.0, 1.0) as f64;
                let mut v = inp.min + t * (inp.max - inp.min);
                if inp.step > 0.0 {
                    v = inp.min + ((v - inp.min) / inp.step).round() * inp.step;
                }
                let v = (v * 1e9).round() / 1e9;
                if set(rt, &mut inp, Value::Num(v)) {
                    handlers.push(key.clone());
                }
            }
        }
        if inp.kind == InputKind::Text && rt.focus.as_ref() == Some(&key) {
            let mut s = get(rt, &inp).display();
            let mut done = false;
            for ev in &events {
                if !ev.state.is_pressed() {
                    continue;
                }
                match &ev.logical_key {
                    Key::Character(c) => {
                        for ch in c.chars().filter(|ch| !ch.is_control()) {
                            if s.chars().count() < inp.max_len {
                                s.push(ch);
                            }
                        }
                    }
                    Key::Space if s.chars().count() < inp.max_len => s.push(' '),
                    Key::Backspace => {
                        s.pop();
                    }
                    Key::Enter => {
                        done = true;
                        handlers.push(key.clone());
                    }
                    Key::Escape => done = true,
                    _ => {}
                }
            }
            set(rt, &mut inp, Value::Str(s));
            if done {
                rt.focus = None;
            }
        }
    }
    if clicked && !clicked_input {
        rt.focus = None;
    }
    if had_list && clicked && (chosen.is_some() || !clicked_input) {
        close_list(&mut commands, rt);
    }
    for key in handlers {
        run_handler(rt, &key);
    }
}

/// Shows each input's current value every frame (so `change volume to 0.2` moves its slider).
pub(super) fn input_sync(
    rt: NonSend<Runtime>,
    time: Res<Time>,
    inputs: Query<&GuiInput>,
    mut texts: Query<(&mut Text, &mut TextColor)>,
    mut nodes: Query<&mut Node, Without<GuiInput>>,
    mut vis: Query<&mut Visibility>,
) {
    let blink = (time.elapsed_secs() * 2.0) as u32 % 2 == 0;
    for inp in &inputs {
        let v = get(&rt, inp);
        match inp.kind {
            InputKind::Text | InputKind::Drop => {
                let Some(Ok((mut t, mut c))) = inp.text.map(|e| texts.get_mut(e)) else { continue };
                let focused = rt.focus.as_ref() == Some(&inp.key);
                let s = v.display();
                let (mut shown, color) = if inp.kind == InputKind::Text && s.is_empty() && !focused {
                    (inp.placeholder.clone(), inp.color.with_alpha(0.45))
                } else {
                    (s, inp.color)
                };
                if inp.kind == InputKind::Text {
                    // long text: show the end, where the typing happens
                    let fit = (((inp.width - 16.0) / inp.char_w) as usize).saturating_sub(1).max(1);
                    let n = shown.chars().count();
                    if n > fit {
                        shown = shown.chars().skip(n - fit).collect();
                    }
                    if focused && blink {
                        shown.push('|');
                    }
                }
                if t.0 != shown {
                    t.0 = shown;
                }
                if c.0 != color {
                    c.0 = color;
                }
            }
            InputKind::Slider => {
                let x = v.as_num(0).unwrap_or(inp.min);
                let range = inp.max - inp.min;
                let t = if range.abs() < 1e-12 { 0.0 } else { ((x - inp.min) / range).clamp(0.0, 1.0) as f32 };
                let pct = Val::Percent(t * 100.0);
                if let Some(Ok(mut n)) = inp.knob.map(|e| nodes.get_mut(e)) {
                    if n.left != pct {
                        n.left = pct;
                    }
                }
                if let Some(Ok(mut n)) = inp.fill.map(|e| nodes.get_mut(e)) {
                    if n.width != pct {
                        n.width = pct;
                    }
                }
            }
            InputKind::Check => {
                let want = if v.truthy() { Visibility::Inherited } else { Visibility::Hidden };
                if let Some(Ok(mut m)) = inp.mark.map(|e| vis.get_mut(e)) {
                    if *m != want {
                        *m = want;
                    }
                }
            }
        }
    }
}

/// Developer hook: with EZA_TEST_INPUTS=1 the engine clicks, types and drags through the first textbox,
/// slider, checkbox and dropdown by itself, so input handling can be checked with EZA_SCREENSHOT.
pub(super) fn scripted_inputs(
    mut frame: Local<u32>,
    inputs: Query<(&GuiInput, &GlobalTransform, &ComputedNode)>,
    opts: Query<&GlobalTransform, With<DropOption>>,
    mut windows: Query<(Entity, &mut Window)>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut keys: EventWriter<KeyboardInput>,
) {
    if std::env::var("EZA_TEST_INPUTS").is_err() {
        return;
    }
    *frame += 1;
    let f = *frame;
    let Ok((win, mut w)) = windows.get_single_mut() else { return };
    // (center, size) of the first input of a kind, in physical pixels
    let find = |k: InputKind| inputs.iter().find(|(i, _, _)| i.kind == k).map(|(_, gt, cn)| (gt.translation().truncate(), cn.size()));
    let at = |k: InputKind, frac: f32| find(k).map(|(c, s)| Vec2::new(c.x - s.x / 2.0 + frac * s.x, c.y));
    let target = match f {
        30 => at(InputKind::Text, 0.5),
        40 => at(InputKind::Slider, 0.1),
        42 => at(InputKind::Slider, 0.7),
        50 => at(InputKind::Check, 0.05),
        60 => at(InputKind::Drop, 0.5),
        66 => opts.iter().last().map(|gt| gt.translation().truncate()),
        _ => None,
    };
    if let Some(p) = target {
        w.set_physical_cursor_position(Some(p.as_dvec2()));
    }
    match f {
        31 | 41 | 51 | 61 | 67 => mouse.press(MouseButton::Left),
        32 | 44 | 52 | 62 | 68 => mouse.release(MouseButton::Left),
        33..=35 => {
            let ch = ["A", "d", "a"][(f - 33) as usize];
            keys.send(KeyboardInput { key_code: KeyCode::KeyA, logical_key: Key::Character(ch.into()), state: bevy::input::ButtonState::Pressed, repeat: false, window: win });
        }
        _ => {}
    }
}
