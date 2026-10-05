//! Terminal apps: a `gui window ... terminal=true` is drawn in the terminal instead of a window,
//! with the same elements (text, buttons, text boxes, checkboxes, sliders, dropdowns) and the same
//! `then` actions. Tab / arrows move between elements, Enter or Space uses them, the mouse works
//! too, and Ctrl+C (or `quit`) ends the app. `on every frame`, `wait` and live `{labels}` keep going.
use crate::interp::Interp;
use crate::server::Server;
use crate::value::*;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use crossterm::style::{Attribute, Color, Print, SetAttribute, SetBackgroundColor, SetForegroundColor};
use crossterm::{cursor, queue, terminal};
use std::io::Write;
use std::rc::Rc;
use std::time::{Duration, Instant};

const PX_PER_COL: f64 = 9.6;
const PX_PER_ROW: f64 = 20.0;
const FOCUSABLE: [&str; 5] = ["button", "textbox", "checkbox", "slider", "dropdown"];
const ACCENT: Color = Color::Rgb { r: 88, g: 101, b: 242 };
const DIM: Color = Color::Rgb { r: 120, g: 120, b: 130 };

/// Windows that asked to be drawn in the terminal.
pub fn terminal_roots(it: &Interp) -> Vec<String> {
    it.gui_roots
        .iter()
        .filter(|r| matches!(it.global(r), Some(Value::Obj(o)) if o.get("terminal").is_some_and(|v| v.truthy())))
        .cloned()
        .collect()
}

#[derive(Clone, Copy, PartialEq)]
struct Cell {
    ch: char,
    fg: Option<Color>,
    bg: Option<Color>,
    bold: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Cell { ch: ' ', fg: None, bg: None, bold: false }
    }
}

struct Buf {
    w: usize,
    h: usize,
    cells: Vec<Cell>,
}

impl Buf {
    fn new(w: usize, h: usize) -> Buf {
        Buf { w, h, cells: vec![Cell::default(); w * h] }
    }
    fn put(&mut self, x: i32, y: i32, ch: char, fg: Option<Color>, bg: Option<Color>, bold: bool) {
        if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h {
            return;
        }
        let c = &mut self.cells[y as usize * self.w + x as usize];
        c.ch = ch;
        if fg.is_some() {
            c.fg = fg;
        }
        if bg.is_some() {
            c.bg = bg;
        }
        c.bold = bold;
    }
    /// Text clipped to `max` columns.
    fn text(&mut self, x: i32, y: i32, s: &str, max: usize, fg: Option<Color>, bg: Option<Color>, bold: bool) {
        for (i, ch) in s.chars().take(max).enumerate() {
            self.put(x + i as i32, y, ch, fg, bg, bold);
        }
    }
    fn fill(&mut self, r: Rect, bg: Color) {
        for y in r.y..r.y + r.h {
            for x in r.x..r.x + r.w {
                self.put(x, y, ' ', None, Some(bg), false);
            }
        }
    }
    fn border(&mut self, r: Rect, fg: Color, title: Option<&str>) {
        if r.w < 2 || r.h < 2 {
            return;
        }
        let (x1, y1) = (r.x + r.w - 1, r.y + r.h - 1);
        for x in r.x + 1..x1 {
            self.put(x, r.y, '─', Some(fg), None, false);
            self.put(x, y1, '─', Some(fg), None, false);
        }
        for y in r.y + 1..y1 {
            self.put(r.x, y, '│', Some(fg), None, false);
            self.put(x1, y, '│', Some(fg), None, false);
        }
        self.put(r.x, r.y, '╭', Some(fg), None, false);
        self.put(x1, r.y, '╮', Some(fg), None, false);
        self.put(r.x, y1, '╰', Some(fg), None, false);
        self.put(x1, y1, '╯', Some(fg), None, false);
        if let Some(t) = title {
            self.text(r.x + 2, r.y, &format!(" {} ", t), (r.w - 4).max(0) as usize, Some(fg), None, true);
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq)]
struct Rect {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

impl Rect {
    fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

/// One element placed on screen.
struct Node {
    root: String,
    path: Vec<usize>,
    o: Rc<Obj>,
    rect: Rect,
}

fn num(o: &Obj, k: &str) -> Option<f64> {
    match o.get(k) {
        Some(Value::Num(n)) => Some(*n),
        _ => None,
    }
}

fn label(o: &Obj) -> String {
    o.get("label").map(|l| l.display()).unwrap_or_default()
}

fn cols(px: f64) -> i32 {
    (px / PX_PER_COL).round().max(1.0) as i32
}

fn rows(px: f64) -> i32 {
    (px / PX_PER_ROW).round().max(1.0) as i32
}

fn kids(o: &Obj) -> Vec<Rc<Obj>> {
    match o.get("children") {
        Some(Value::List(l)) => l.iter().filter_map(|c| if let Value::Obj(c) = c { Some(c.clone()) } else { None }).collect(),
        _ => vec![],
    }
}

fn visible(o: &Obj) -> bool {
    !matches!(o.get("visible"), Some(Value::Bool(false)))
}

fn options(o: &Obj) -> Vec<String> {
    match o.get("options") {
        Some(Value::List(l)) => l.iter().map(|v| v.display()).collect(),
        Some(v) => vec![v.display()],
        None => vec![],
    }
}

fn anchor(o: &Obj) -> String {
    match o.get("position") {
        Some(Value::Str(s)) => s.clone(),
        _ => String::new(),
    }
}

fn is_container(o: &Obj) -> bool {
    !matches!(o.type_name.as_str(), "text" | "button" | "textbox" | "checkbox" | "slider" | "dropdown" | "chart")
}

/// Windows (and boxes with a color) get a border or fill; the space inside them.
fn inner_pad(o: &Obj) -> (i32, i32) {
    let border = o.type_name == "window";
    let pad = match o.get("padding") {
        Some(Value::Num(p)) if *p >= 8.0 => 1,
        Some(Value::Num(_)) => 0,
        _ if border => 1,
        _ => 0,
    };
    let b = if border { 1 } else { 0 };
    (b + pad, b)
}

fn gap(o: &Obj) -> i32 {
    match num(o, "gap") {
        Some(g) if g >= 12.0 => 1,
        _ => 0,
    }
}

/// The size an element wants, in terminal cells.
fn natural(o: &Obj) -> (i32, i32) {
    let l = label(o).chars().count() as i32;
    let set = |k: &str| matches!(o.get(k), Some(Value::Bool(true)));
    let ew = num(o, "width").filter(|_| set("fixed_width")).map(cols);
    let eh = num(o, "height").filter(|_| set("fixed_height")).map(rows);
    match o.type_name.as_str() {
        "text" => (ew.unwrap_or(l.max(1)), 1),
        "button" => (ew.unwrap_or(l + 4), 1),
        "textbox" => (ew.unwrap_or(24), 1),
        "slider" => (ew.unwrap_or(24), 1),
        "checkbox" => (l + 4, 1),
        "dropdown" => (options(o).iter().map(|s| s.chars().count() as i32).max().unwrap_or(4) + 4, 1),
        "chart" => (ew.unwrap_or(40), eh.unwrap_or(10)),
        _ => {
            let (px, b) = inner_pad(o);
            let ch: Vec<(i32, i32)> = kids(o).iter().filter(|c| visible(c)).map(|c| natural(c)).collect();
            let w = ch.iter().map(|c| c.0).max().unwrap_or(0) + 2 * px;
            let h = ch.iter().map(|c| c.1).sum::<i32>() + gap(o) * (ch.len() as i32 - 1).max(0) + 2 * b;
            (ew.unwrap_or(0).max(w), eh.unwrap_or(0).max(h))
        }
    }
}

/// Places an element and everything inside it (the same anchors as the window layout).
fn place(o: &Rc<Obj>, r: Rect, root: &str, path: Vec<usize>, out: &mut Vec<Node>) {
    out.push(Node { root: root.to_string(), path: path.clone(), o: o.clone(), rect: r });
    if !is_container(o) {
        return;
    }
    let (px, b) = inner_pad(o);
    let mut rem = Rect { x: r.x + px, y: r.y + b, w: (r.w - 2 * px).max(0), h: (r.h - 2 * b).max(0) };
    let children = kids(o);
    let g = gap(o);
    for (i, c) in children.iter().enumerate() {
        if !visible(c) {
            continue;
        }
        let (nw, nh) = natural(c);
        let mut p = path.clone();
        p.push(i);
        let cr = match anchor(c).as_str() {
            "top_bar" => {
                let cr = Rect { x: rem.x, y: rem.y, w: rem.w, h: nh };
                rem.y += nh;
                rem.h = (rem.h - nh).max(0);
                cr
            }
            "bottom_bar" => {
                rem.h = (rem.h - nh).max(0);
                Rect { x: rem.x, y: rem.y + rem.h, w: rem.w, h: nh }
            }
            "side_left" => {
                let cr = Rect { x: rem.x, y: rem.y, w: nw, h: rem.h };
                rem.x += nw;
                rem.w = (rem.w - nw).max(0);
                cr
            }
            "side_right" => {
                rem.w = (rem.w - nw).max(0);
                Rect { x: rem.x + rem.w, y: rem.y, w: nw, h: rem.h }
            }
            _ => continue,
        };
        place(c, cr, root, p, out);
    }
    let mut flow = rem.y;
    for (i, c) in children.iter().enumerate() {
        let a = anchor(c);
        if !visible(c) || ["top_bar", "bottom_bar", "side_left", "side_right"].contains(&a.as_str()) {
            continue;
        }
        let (nw, nh) = natural(c);
        let mut p = path.clone();
        p.push(i);
        let (nw, nh) = (nw.min(rem.w.max(1)), nh);
        let cr = match a.as_str() {
            "fill" => rem,
            "center" => Rect { x: rem.x + (rem.w - nw) / 2, y: rem.y + (rem.h - nh) / 2, w: nw, h: nh },
            "top_left" => Rect { x: rem.x, y: rem.y, w: nw, h: nh },
            "top_right" => Rect { x: rem.x + rem.w - nw, y: rem.y, w: nw, h: nh },
            "bottom_left" => Rect { x: rem.x, y: rem.y + rem.h - nh, w: nw, h: nh },
            "bottom_right" => Rect { x: rem.x + rem.w - nw, y: rem.y + rem.h - nh, w: nw, h: nh },
            _ => {
                let cr = Rect { x: rem.x, y: flow, w: nw, h: nh };
                flow += nh + g;
                cr
            }
        };
        place(c, cr, root, p, out);
    }
}

struct App {
    focus: Option<(String, Vec<usize>)>,
    open: Option<(String, Vec<usize>, usize)>,
    hover: Option<(String, Vec<usize>)>,
    status: Option<(String, Instant)>,
    errors: Vec<String>,
}

fn key_of(n: &Node) -> (String, Vec<usize>) {
    (n.root.clone(), n.path.clone())
}

/// Lays out every terminal window for a w x h screen.
fn layout(it: &Interp, roots: &[String], w: i32, h: i32) -> Vec<Node> {
    let mut out = vec![];
    for r in roots {
        let Some(Value::Obj(o)) = it.global(r) else { continue };
        if !visible(&o) {
            continue;
        }
        let (nw, nh) = natural(&o);
        let (nw, nh) = (nw.min(w), nh.min(h));
        let rect = if matches!(o.get("centered"), Some(Value::Bool(true))) {
            Rect { x: (w - nw) / 2, y: (h - nh) / 2, w: nw, h: nh }
        } else if matches!(o.get("fullscreen"), Some(Value::Bool(true))) {
            Rect { x: 0, y: 0, w, h: h - 1 }
        } else {
            let x = num(&o, "x").map(|x| (x / PX_PER_COL).round() as i32).unwrap_or(0);
            let y = num(&o, "y").map(|y| (y / PX_PER_ROW).round() as i32).unwrap_or(0);
            Rect { x, y, w: nw, h: nh }
        };
        place(&o, rect, r, vec![], &mut out);
    }
    out
}

/// The value an input shows: from its variable (name="x") or kept on the element itself.
fn value_of(it: &Interp, n: &Node) -> Value {
    if let Some(Value::Str(name)) = n.o.get("name") {
        if let Some(v) = it.global(name) {
            return v;
        }
    }
    match n.o.type_name.as_str() {
        "textbox" => Value::Str(n.o.get("text").or(n.o.get("value")).map(|v| v.display()).unwrap_or_default()),
        "checkbox" => Value::Bool(n.o.get("checked").or(n.o.get("value")).is_some_and(|v| v.truthy())),
        "slider" => Value::Num(num(&n.o, "value").unwrap_or(num(&n.o, "min").unwrap_or(0.0))),
        _ => n.o.get("value").cloned().unwrap_or_else(|| Value::Str(options(&n.o).first().cloned().unwrap_or_default())),
    }
}

impl App {
    fn set_value(&mut self, it: &mut Interp, n: &Node, v: Value) {
        if equals(&value_of(it, n), &v) {
            return;
        }
        match n.o.get("name") {
            Some(Value::Str(name)) => it.set_var(name, v),
            _ => {
                let field = match n.o.type_name.as_str() {
                    "textbox" => "text",
                    "checkbox" => "checked",
                    _ => "value",
                };
                it.set_gui_field(&n.root, &n.path, field, v);
            }
        }
    }

    /// Runs an element's `then` action.
    fn act(&mut self, it: &mut Interp, n: &Node) {
        let Some(f) = n.o.get("on_click").cloned() else { return };
        if let Err(e) = it.call_value(f, vec![], vec![], vec![]) {
            if !e.is_switch() {
                self.status = Some((e.headline(), Instant::now()));
                self.errors.push(e.to_string());
            }
        }
    }

    /// Enter / Space / a click on an element.
    fn activate(&mut self, it: &mut Interp, n: &Node, click_x: Option<i32>) {
        match n.o.type_name.as_str() {
            "button" => self.act(it, n),
            "checkbox" => {
                let v = !value_of(it, n).truthy();
                self.set_value(it, n, Value::Bool(v));
                self.act(it, n);
            }
            "dropdown" => {
                let opts = options(&n.o);
                let cur = value_of(it, n).display();
                let at = opts.iter().position(|o| *o == cur).unwrap_or(0);
                self.open = match &self.open {
                    Some((r, p, _)) if *r == n.root && *p == n.path => None,
                    _ => Some((n.root.clone(), n.path.clone(), at)),
                };
            }
            "slider" => {
                if let Some(x) = click_x {
                    let (lo, hi) = (num(&n.o, "min").unwrap_or(0.0), num(&n.o, "max").unwrap_or(1.0));
                    let bar = (n.rect.w - 7).max(2) as f64;
                    let t = ((x - n.rect.x) as f64 / (bar - 1.0)).clamp(0.0, 1.0);
                    self.set_value(it, n, Value::Num(round_step(&n.o, lo + t * (hi - lo))));
                    self.act(it, n);
                }
            }
            "textbox" if click_x.is_none() => self.act(it, n),
            _ => {}
        }
    }
}

fn round_step(o: &Obj, v: f64) -> f64 {
    match num(o, "step") {
        Some(s) if s > 0.0 => (v / s).round() * s,
        _ => (v * 100.0).round() / 100.0,
    }
}

fn colors(o: &Obj) -> (Option<Color>, Option<Color>) {
    let c = |k: &str| o.get(k).and_then(crate::term::color_of);
    let kind = o.type_name.as_str();
    let has_bg = o.get("background").is_some();
    let bg = c("background").or_else(|| if kind != "text" && kind != "chart" { c("color") } else { None });
    let fg = c("text_color").or_else(|| if kind == "text" || has_bg { c("color") } else { None });
    (fg, bg)
}

fn draw(it: &Interp, nodes: &[Node], app: &App, w: usize, h: usize) -> Buf {
    let mut b = Buf::new(w, h);
    for n in nodes {
        let (fg, bg) = colors(&n.o);
        let r = n.rect;
        let focused = app.focus.as_ref() == Some(&key_of(n));
        let hovered = app.hover.as_ref() == Some(&key_of(n));
        match n.o.type_name.as_str() {
            "window" => {
                if let Some(bg) = bg {
                    b.fill(r, bg);
                }
                let title = n.o.get("title").map(|t| t.display());
                b.border(r, fg.unwrap_or(DIM), title.as_deref());
            }
            "text" => b.text(r.x, r.y, &label(&n.o), r.w as usize, fg, None, matches!(n.o.get("bold"), Some(Value::Bool(true)))),
            "button" => {
                let bgc = if focused { ACCENT } else if hovered { Color::Rgb { r: 80, g: 80, b: 92 } } else { bg.unwrap_or(Color::Rgb { r: 55, g: 55, b: 65 }) };
                let text = format!(" {} ", label(&n.o));
                let pad = (r.w - text.chars().count() as i32).max(0);
                let shown = format!("{}{}{}", " ".repeat(pad as usize / 2), text, " ".repeat((pad - pad / 2) as usize));
                b.text(r.x, r.y, &shown, r.w as usize, Some(fg.unwrap_or(Color::White)), Some(bgc), focused);
            }
            "checkbox" => {
                let mark = if value_of(it, n).truthy() { "[x] " } else { "[ ] " };
                b.text(r.x, r.y, &format!("{}{}", mark, label(&n.o)), r.w as usize, if focused { Some(ACCENT) } else { fg }, None, focused);
            }
            "textbox" => {
                let v = value_of(it, n).display();
                let field = Color::Rgb { r: 40, g: 40, b: 48 };
                b.text(r.x, r.y, &" ".repeat(r.w as usize), r.w as usize, None, Some(if focused { Color::Rgb { r: 50, g: 50, b: 70 } } else { field }), false);
                if v.is_empty() && !focused {
                    b.text(r.x + 1, r.y, &label(&n.o), (r.w - 2).max(0) as usize, Some(DIM), None, false);
                } else {
                    let fit = (r.w - 2).max(1) as usize;
                    let mut s: String = v.chars().rev().take(fit - 1).collect::<Vec<_>>().into_iter().rev().collect();
                    if focused {
                        s.push('▏');
                    }
                    b.text(r.x + 1, r.y, &s, fit, Some(fg.unwrap_or(Color::White)), None, false);
                }
            }
            "slider" => {
                let (lo, hi) = (num(&n.o, "min").unwrap_or(0.0), num(&n.o, "max").unwrap_or(1.0));
                let v = value_of(it, n).as_num(0).unwrap_or(lo);
                let bar = (r.w - 7).max(2);
                let t = if (hi - lo).abs() < 1e-12 { 0.0 } else { ((v - lo) / (hi - lo)).clamp(0.0, 1.0) };
                let knob = (t * (bar - 1) as f64).round() as i32;
                for i in 0..bar {
                    let (ch, c) = if i == knob { ('●', if focused { ACCENT } else { Color::White }) } else if i < knob { ('━', ACCENT) } else { ('─', DIM) };
                    b.put(r.x + i, r.y, ch, Some(c), None, false);
                }
                b.text(r.x + bar + 1, r.y, &fmt_num((v * 100.0).round() / 100.0), 6, fg, None, false);
            }
            "dropdown" => {
                let s = format!(" {} ▾ ", value_of(it, n).display());
                b.text(r.x, r.y, &s, r.w.max(s.chars().count() as i32) as usize, Some(fg.unwrap_or(Color::White)), Some(if focused { ACCENT } else { bg.unwrap_or(Color::Rgb { r: 55, g: 55, b: 65 }) }), focused);
            }
            "chart" => draw_chart(&mut b, it, n),
            _ => {
                if let Some(bg) = bg {
                    b.fill(r, bg);
                }
                if let Some(l) = n.o.get("label") {
                    if n.o.type_name != "box" {
                        b.text(r.x, r.y, &l.display(), r.w as usize, fg, None, false);
                    }
                }
            }
        }
    }
    // an open dropdown's list goes on top of everything
    if let Some((root, path, at)) = &app.open {
        if let Some(n) = nodes.iter().find(|n| n.root == *root && n.path == *path) {
            let opts = options(&n.o);
            let width = opts.iter().map(|o| o.chars().count()).max().unwrap_or(1) + 2;
            for (i, o) in opts.iter().enumerate() {
                let sel = i == *at;
                let s = format!(" {:<w$} ", o, w = width - 2);
                b.text(n.rect.x, n.rect.y + 1 + i as i32, &s, width, Some(Color::White), Some(if sel { ACCENT } else { Color::Rgb { r: 45, g: 45, b: 55 } }), sel);
            }
        }
    }
    if let Some((msg, at)) = &app.status {
        if at.elapsed() < Duration::from_secs(6) {
            b.text(0, h as i32 - 1, &format!(" {} ", msg), w, Some(Color::White), Some(Color::DarkRed), false);
        }
    } else {
        let hint = " Tab / arrows: move   Enter: use   Ctrl+C: quit ";
        b.text((w as i32 - hint.chars().count() as i32).max(0), h as i32 - 1, hint, w, Some(DIM), None, false);
    }
    b
}

/// A bar chart from the chart's data (a list of numbers, or a dictionary of name: number).
fn draw_chart(b: &mut Buf, it: &Interp, n: &Node) {
    let r = n.rect;
    let data = match n.o.get("data") {
        Some(Value::Str(name)) => it.global(name).unwrap_or(Value::None),
        Some(v) => v.clone(),
        None => Value::None,
    };
    let items: Vec<(String, f64)> = match &data {
        Value::List(l) => l.iter().enumerate().map(|(i, v)| (format!("{}", i + 1), v.as_num(0).unwrap_or(0.0))).collect(),
        Value::Obj(o) => o.fields.iter().map(|(k, v)| (k.clone(), v.as_num(0).unwrap_or(0.0))).collect(),
        _ => vec![],
    };
    let title = label(&n.o);
    b.text(r.x, r.y, &title, r.w as usize, None, None, true);
    let max = items.iter().map(|(_, v)| *v).fold(0.0f64, f64::max).max(1e-9);
    let name_w = items.iter().map(|(k, _)| k.chars().count()).max().unwrap_or(0).min(12) as i32;
    for (i, (k, v)) in items.iter().enumerate().take((r.h - 1).max(0) as usize) {
        let y = r.y + 1 + i as i32;
        b.text(r.x, y, k, name_w as usize, Some(DIM), None, false);
        let room = (r.w - name_w - 8).max(1) as f64;
        let len = (v / max * room).round() as usize;
        b.text(r.x + name_w + 1, y, &"█".repeat(len), room as usize, Some(ACCENT), None, false);
        b.text(r.x + name_w + 2 + len as i32, y, &fmt_num(*v), 6, None, None, false);
    }
}

fn flush(out: &mut impl Write, b: &Buf, old: Option<&Buf>) {
    for y in 0..b.h {
        let row = &b.cells[y * b.w..(y + 1) * b.w];
        if let Some(o) = old {
            if o.w == b.w && o.h == b.h && &o.cells[y * o.w..(y + 1) * o.w] == row {
                continue;
            }
        }
        let _ = queue!(out, cursor::MoveTo(0, y as u16));
        let mut last: Option<(Option<Color>, Option<Color>, bool)> = None;
        for c in row {
            let style = (c.fg, c.bg, c.bold);
            if last != Some(style) {
                let _ = queue!(out, SetAttribute(Attribute::Reset));
                let _ = queue!(out, SetForegroundColor(c.fg.unwrap_or(Color::Reset)), SetBackgroundColor(c.bg.unwrap_or(Color::Reset)));
                if c.bold {
                    let _ = queue!(out, SetAttribute(Attribute::Bold));
                }
                last = Some(style);
            }
            let _ = queue!(out, Print(c.ch));
        }
    }
    let _ = queue!(out, SetAttribute(Attribute::Reset));
    let _ = out.flush();
}

fn key_name(code: KeyCode) -> Option<String> {
    Some(match code {
        KeyCode::Char(' ') => "space".into(),
        KeyCode::Char(c) => c.to_lowercase().to_string(),
        KeyCode::Enter => "enter".into(),
        KeyCode::Esc => "escape".into(),
        KeyCode::Backspace => "backspace".into(),
        KeyCode::Tab => "tab".into(),
        KeyCode::Up => "up".into(),
        KeyCode::Down => "down".into(),
        KeyCode::Left => "left".into(),
        KeyCode::Right => "right".into(),
        KeyCode::Delete => "delete".into(),
        KeyCode::F(n) => format!("f{}", n),
        _ => return None,
    })
}

/// Runs the terminal app until Ctrl+C or `quit`. The web server (if any) keeps answering too.
pub fn run(it: &mut Interp, server: Option<&Server>) -> i32 {
    // developer check: EZA_TUI_DUMP=screen.txt plays EZA_TUI_KEYS (like "tab,type:milk,enter") and saves the screen
    if let Ok(dump) = std::env::var("EZA_TUI_DUMP") {
        return headless(it, &dump);
    }
    if !crate::term::is_tty() || !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        eprintln!("this program shows a terminal app (gui ... terminal=true), which needs a real terminal - run it from a terminal window");
        return 1;
    }
    it.frame_mode = true;
    let mut out = std::io::stdout();
    let _ = terminal::enable_raw_mode();
    let _ = crossterm::execute!(out, terminal::EnterAlternateScreen, cursor::Hide, event::EnableMouseCapture);
    // put the terminal back even if something goes badly wrong
    let old_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |p| {
        let _ = crossterm::execute!(std::io::stdout(), event::DisableMouseCapture, terminal::LeaveAlternateScreen, cursor::Show);
        let _ = terminal::disable_raw_mode();
        old_hook(p);
    }));
    let mut app = App { focus: None, open: None, hover: None, status: None, errors: vec![] };
    let mut prev: Option<Buf> = None;
    let frame = Duration::from_micros(16_667);
    let mut next = Instant::now();
    let code = 'main: loop {
        let roots = terminal_roots(it);
        if roots.is_empty() || it.quitting {
            break 0;
        }
        let (w, h) = terminal::size().map(|(w, h)| (w as i32, h as i32)).unwrap_or((80, 24));
        let nodes = layout(it, &roots, w, h);
        let focusables: Vec<usize> = nodes.iter().enumerate().filter(|(_, n)| FOCUSABLE.contains(&n.o.type_name.as_str())).map(|(i, _)| i).collect();
        if app.focus.as_ref().map_or(true, |f| !focusables.iter().any(|&i| key_of(&nodes[i]) == *f)) {
            app.focus = focusables.first().map(|&i| key_of(&nodes[i]));
        }
        let buf = draw(it, &nodes, &app, w.max(1) as usize, h.max(1) as usize);
        flush(&mut out, &buf, prev.as_ref());
        prev = Some(buf);

        let wait = next.saturating_duration_since(Instant::now());
        let mut pressed: Vec<String> = vec![];
        if event::poll(wait).unwrap_or(false) {
            while event::poll(Duration::ZERO).unwrap_or(false) {
                let Ok(ev) = event::read() else { break };
                let focused = app.focus.clone().and_then(|f| nodes.iter().position(|n| key_of(n) == f));
                match ev {
                    Event::Key(k) if k.kind != KeyEventKind::Release => {
                        if k.modifiers.contains(KeyModifiers::CONTROL) && matches!(k.code, KeyCode::Char('c') | KeyCode::Char('q')) {
                            break 'main 0;
                        }
                        if let Some(name) = key_name(k.code) {
                            pressed.push(name);
                        }
                        let move_focus = |app: &mut App, d: i32| {
                            if focusables.is_empty() {
                                return;
                            }
                            let cur = app.focus.as_ref().and_then(|f| focusables.iter().position(|&i| key_of(&nodes[i]) == *f)).unwrap_or(0) as i32;
                            let n = focusables.len() as i32;
                            app.focus = Some(key_of(&nodes[focusables[((cur + d) % n + n) as usize % n as usize]]));
                        };
                        // an open dropdown list takes the keys first
                        if let Some((root, path, at)) = app.open.clone() {
                            if let Some(n) = nodes.iter().find(|n| n.root == root && n.path == path) {
                                let opts = options(&n.o);
                                match k.code {
                                    KeyCode::Up => app.open = Some((root, path, (at + opts.len().max(1) - 1) % opts.len().max(1))),
                                    KeyCode::Down | KeyCode::Tab => app.open = Some((root, path, (at + 1) % opts.len().max(1))),
                                    KeyCode::Enter | KeyCode::Char(' ') => {
                                        if let Some(o) = opts.get(at) {
                                            app.set_value(it, n, Value::Str(o.clone()));
                                            app.act(it, n);
                                        }
                                        app.open = None;
                                    }
                                    KeyCode::Esc => app.open = None,
                                    _ => {}
                                }
                                continue;
                            }
                        }
                        let fnode = focused.map(|i| &nodes[i]);
                        let typing = fnode.is_some_and(|n| n.o.type_name == "textbox");
                        match k.code {
                            KeyCode::Tab | KeyCode::Down => move_focus(&mut app, 1),
                            KeyCode::BackTab | KeyCode::Up => move_focus(&mut app, -1),
                            KeyCode::Left | KeyCode::Right if fnode.is_some_and(|n| n.o.type_name == "slider") => {
                                let n = fnode.unwrap();
                                let (lo, hi) = (num(&n.o, "min").unwrap_or(0.0), num(&n.o, "max").unwrap_or(1.0));
                                let step = num(&n.o, "step").unwrap_or((hi - lo) / 20.0);
                                let v = value_of(it, n).as_num(0).unwrap_or(lo) + if k.code == KeyCode::Left { -step } else { step };
                                app.set_value(it, n, Value::Num(round_step(&n.o, v.clamp(lo.min(hi), hi.max(lo)))));
                                app.act(it, n);
                            }
                            KeyCode::Backspace if typing => {
                                let n = fnode.unwrap();
                                let mut s = value_of(it, n).display();
                                s.pop();
                                app.set_value(it, n, Value::Str(s));
                            }
                            KeyCode::Char(c) if typing && !k.modifiers.contains(KeyModifiers::CONTROL) => {
                                let n = fnode.unwrap();
                                let s = value_of(it, n).display() + &c.to_string();
                                app.set_value(it, n, Value::Str(s));
                            }
                            KeyCode::Enter | KeyCode::Char(' ') => {
                                if let Some(n) = fnode {
                                    app.activate(it, n, None);
                                }
                            }
                            KeyCode::Esc => app.status = None,
                            _ => {}
                        }
                    }
                    Event::Mouse(m) => {
                        let (x, y) = (m.column as i32, m.row as i32);
                        let top = nodes.iter().rposition(|n| FOCUSABLE.contains(&n.o.type_name.as_str()) && n.rect.contains(x, y));
                        match m.kind {
                            MouseEventKind::Down(MouseButton::Left) => {
                                // a click in an open dropdown list picks that option
                                if let Some((root, path, _)) = app.open.clone() {
                                    if let Some(n) = nodes.iter().find(|n| n.root == root && n.path == path) {
                                        let opts = options(&n.o);
                                        let i = y - n.rect.y - 1;
                                        if i >= 0 && (i as usize) < opts.len() && x >= n.rect.x && x < n.rect.x + opts.iter().map(|o| o.chars().count() as i32).max().unwrap_or(0) + 2 {
                                            app.set_value(it, n, Value::Str(opts[i as usize].clone()));
                                            app.act(it, n);
                                            app.open = None;
                                            continue;
                                        }
                                    }
                                    app.open = None;
                                }
                                if let Some(i) = top {
                                    app.focus = Some(key_of(&nodes[i]));
                                    app.activate(it, &nodes[i], Some(x));
                                }
                            }
                            MouseEventKind::Drag(MouseButton::Left) => {
                                if let Some(i) = focused.filter(|&i| nodes[i].o.type_name == "slider") {
                                    app.activate(it, &nodes[i], Some(x));
                                }
                            }
                            MouseEventKind::Moved => {
                                let now = top.map(|i| key_of(&nodes[i]));
                                if now != app.hover {
                                    if let Some((r, p)) = &app.hover {
                                        it.set_gui_field(r, p, "hover", Value::Bool(false));
                                    }
                                    if let Some((r, p)) = &now {
                                        it.set_gui_field(r, p, "hover", Value::Bool(true));
                                    }
                                    app.hover = now;
                                }
                            }
                            _ => {}
                        }
                    }
                    Event::Resize(..) => prev = None,
                    _ => {}
                }
            }
        }
        if let Some(s) = server {
            s.poll(it, Duration::ZERO);
        }
        if Instant::now() >= next {
            next = (next + frame).max(Instant::now() - frame);
            it.keys_pressed = pressed.iter().cloned().collect();
            it.keys_held = it.keys_pressed.clone();
            let r = it.tick();
            it.keys_pressed.clear();
            it.keys_held.clear();
            for e in it.take_errors() {
                app.status = Some((e.headline(), Instant::now()));
                app.errors.push(e.to_string());
            }
            match r {
                Err(e) if e.is_switch() => {
                    if it.quitting {
                        break 0;
                    }
                }
                Err(e) => {
                    app.status = Some((e.headline(), Instant::now()));
                    app.errors.push(e.to_string());
                }
                Ok(()) => {}
            }
        }
    };
    let _ = crossterm::execute!(out, event::DisableMouseCapture, terminal::LeaveAlternateScreen, cursor::Show);
    let _ = terminal::disable_raw_mode();
    let _ = std::panic::take_hook();
    // the full error messages, now that the terminal is back to normal
    for e in &app.errors {
        eprintln!("{}\n", e);
    }
    code
}

/// The screen as plain text (for EZA_TUI_DUMP).
fn screen_text(b: &Buf) -> String {
    (0..b.h).map(|y| b.cells[y * b.w..(y + 1) * b.w].iter().map(|c| c.ch).collect::<String>().trim_end().to_string()).collect::<Vec<_>>().join("\n")
}

/// Runs a terminal app without a terminal: scripted keys in, the final screen out to a file.
fn headless(it: &mut Interp, dump: &str) -> i32 {
    it.frame_mode = true;
    let keys: Vec<String> = std::env::var("EZA_TUI_KEYS").unwrap_or_default().split(',').map(|k| k.trim().to_string()).filter(|k| !k.is_empty()).collect();
    let mut app = App { focus: None, open: None, hover: None, status: None, errors: vec![] };
    let (w, h) = (80, 24);
    let mut shots = vec![];
    for step in 0..keys.len() + 3 {
        let roots = terminal_roots(it);
        let nodes = layout(it, &roots, w, h);
        let focusables: Vec<usize> = nodes.iter().enumerate().filter(|(_, n)| FOCUSABLE.contains(&n.o.type_name.as_str())).map(|(i, _)| i).collect();
        if app.focus.as_ref().map_or(true, |f| !focusables.iter().any(|&i| key_of(&nodes[i]) == *f)) {
            app.focus = focusables.first().map(|&i| key_of(&nodes[i]));
        }
        if let Some(k) = keys.get(step) {
            let focused = app.focus.clone().and_then(|f| nodes.iter().position(|n| key_of(n) == f));
            let fnode = focused.map(|i| &nodes[i]);
            let cur = app.focus.as_ref().and_then(|f| focusables.iter().position(|&i| key_of(&nodes[i]) == *f)).unwrap_or(0);
            let open = app.open.clone();
            match (k.as_str(), open) {
                ("snap", _) => {
                    let b = draw(it, &nodes, &app, w as usize, h as usize);
                    shots.push(screen_text(&b));
                }
                ("down", Some((r, p, at))) | ("tab", Some((r, p, at))) => {
                    let n = nodes.iter().find(|n| n.root == r && n.path == p).unwrap();
                    app.open = Some((r, p, (at + 1) % options(&n.o).len().max(1)));
                }
                ("enter", Some((r, p, at))) => {
                    let n = nodes.iter().find(|n| n.root == r && n.path == p).unwrap();
                    if let Some(o) = options(&n.o).get(at) {
                        app.set_value(it, n, Value::Str(o.clone()));
                    }
                    app.open = None;
                }
                ("tab" | "down", None) if !focusables.is_empty() => app.focus = Some(key_of(&nodes[focusables[(cur + 1) % focusables.len()]])),
                ("up", None) if !focusables.is_empty() => app.focus = Some(key_of(&nodes[focusables[(cur + focusables.len() - 1) % focusables.len()]])),
                ("right" | "left", None) => {
                    if let Some(n) = fnode.filter(|n| n.o.type_name == "slider") {
                        let (lo, hi) = (num(&n.o, "min").unwrap_or(0.0), num(&n.o, "max").unwrap_or(1.0));
                        let step = num(&n.o, "step").unwrap_or((hi - lo) / 20.0);
                        let v = value_of(it, n).as_num(0).unwrap_or(lo) + if k == "left" { -step } else { step };
                        app.set_value(it, n, Value::Num(round_step(&n.o, v.clamp(lo, hi))));
                    }
                }
                ("enter" | "space", None) => {
                    if let Some(n) = fnode {
                        app.activate(it, n, None);
                    }
                }
                (t, _) if t.starts_with("type:") => {
                    if let Some(n) = fnode.filter(|n| n.o.type_name == "textbox") {
                        let s = value_of(it, n).display() + &t[5..];
                        app.set_value(it, n, Value::Str(s));
                    }
                }
                _ => {}
            }
        }
        let _ = it.tick();
        for e in it.take_errors() {
            app.status = Some((e.headline(), Instant::now()));
        }
        if it.quitting {
            shots.push("(the app quit)".to_string());
            break;
        }
    }
    if !it.quitting {
        let roots = terminal_roots(it);
        let nodes = layout(it, &roots, w, h);
        shots.push(screen_text(&draw(it, &nodes, &app, w as usize, h as usize)));
    }
    let _ = std::fs::write(dump, shots.join("\n\n=====\n\n"));
    0
}
