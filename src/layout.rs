//! GUI layout: the "Claim & Shrink" three-pass border layout.
//! Pass 1 docked anchors claim edges, pass 2 floating anchors position in what's left, pass 3 `fill` floods the rest.
use crate::ast::{DeclNode, Stmt};
use crate::error::{EzaError, R};
use crate::interp::{Interp, Scope};
use crate::value::*;
use std::rc::Rc;

/// Screen size used for `centered=true` until the engine provides a real window.
const SCREEN: (f64, f64) = (1920.0, 1080.0);
const DEFAULT_PADDING: f64 = 8.0;
const CHAR_W: f64 = 9.6; // the built-in font is about 0.6em wide per character
const LINE_H: f64 = 16.0;
const DOCKED: [&str; 4] = ["top_bar", "bottom_bar", "side_left", "side_right"];
/// GUI elements people type into or drag; with name="x" their value lives in the variable x
pub const INPUT_KINDS: [&str; 4] = ["textbox", "slider", "checkbox", "dropdown"];
pub const CHECK_BOX: f64 = 20.0;

struct Ui {
    kind: String,
    label: Option<String>,
    props: Vec<(String, Value)>,
    children: Vec<Ui>,
    handler: Option<Value>,
    rect: [f64; 4],
    clipped: bool,
}

impl Ui {
    fn prop(&self, k: &str) -> Option<&Value> {
        self.props.iter().find(|(n, _)| n == k).map(|(_, v)| v)
    }
    fn num(&self, k: &str) -> Option<f64> {
        match self.prop(k) {
            Some(Value::Num(n)) => Some(*n),
            _ => None,
        }
    }
    fn anchor(&self) -> String {
        match self.prop("position") {
            Some(Value::Str(s)) => s.clone(),
            _ => String::new(),
        }
    }
    fn explicit(&self) -> (Option<f64>, Option<f64>) {
        let (mut w, mut h) = (self.num("width"), self.num("height"));
        if let Some(Value::List(l)) = self.prop("size") {
            if let (Some(Value::Num(a)), Some(Value::Num(b))) = (l.first(), l.get(1)) {
                w = Some(*a);
                h = Some(*b);
            }
        }
        (w, h)
    }
    /// padding = 10  |  10, 20 (vertical, horizontal)  |  top, right, bottom, left  ->  (horizontal, vertical)
    fn pad_xy(&self) -> Option<(f64, f64)> {
        match self.prop("padding") {
            Some(Value::Num(p)) => Some((*p, *p)),
            Some(Value::List(l)) => {
                let n: Vec<f64> = l.iter().filter_map(|v| if let Value::Num(x) = v { Some(*x) } else { None }).collect();
                match n.len() {
                    1 => Some((n[0], n[0])),
                    2 | 3 => Some((n[1], n[0])),
                    4 => Some(((n[1] + n[3]) / 2.0, (n[0] + n[2]) / 2.0)),
                    _ => None,
                }
            }
            _ => None,
        }
    }
    /// font_size is in points (12pt = 16 pixels, the default)
    fn font_px(&self) -> f64 {
        self.num("font_size").map(|pt| pt * 4.0 / 3.0).unwrap_or(LINE_H)
    }
    /// Size when nothing forces it: text measures itself, boxes hug their children plus padding.
    fn natural(&self) -> (f64, f64) {
        let (w, h) = self.explicit();
        let fpx = self.font_px();
        let text_w = self.label.as_ref().map(|s| s.chars().count() as f64 * CHAR_W * fpx / LINE_H).unwrap_or(0.0);
        let (px, py) = self.pad_xy().unwrap_or((DEFAULT_PADDING, DEFAULT_PADDING));
        let char_w = CHAR_W * fpx / LINE_H;
        let (nw, nh) = match self.kind.as_str() {
            "text" => (text_w, fpx * 1.25),
            "button" => (text_w + 2.0 * px, fpx * 1.25 + 2.0 * py),
            "textbox" => (200.0, fpx * 1.25 + 12.0),
            "slider" => (200.0, 24.0),
            "chart" => (360.0, 220.0),
            "checkbox" => (CHECK_BOX + 8.0 + text_w, (fpx * 1.25).max(CHECK_BOX)),
            "dropdown" => {
                let longest = match self.prop("options") {
                    Some(Value::List(l)) => l.iter().map(|o| o.display().chars().count()).max().unwrap_or(0),
                    Some(o) => o.display().chars().count(),
                    None => 0,
                };
                (longest as f64 * char_w + 2.0 * px + 20.0, fpx * 1.25 + 2.0 * py)
            }
            _ => {
                let p = px.max(py);
                let (mut cw, mut ch) = (0.0f64, 0.0);
                for c in &self.children {
                    let (a, b) = c.natural();
                    cw = cw.max(a);
                    ch += b;
                }
                ch += self.num("gap").unwrap_or(0.0) * self.children.len().saturating_sub(1) as f64;
                (cw.max(text_w) + 2.0 * p, ch + 2.0 * p)
            }
        };
        (w.unwrap_or(nw), h.unwrap_or(nh))
    }
}

fn layout(ui: &mut Ui, rect: [f64; 4]) {
    ui.rect = rect;
    let [x, y, w, h] = rect;
    let (padx, pady) = match ui.pad_xy() {
        Some(p) => p,
        None if ui.kind != "window" && ui.anchor().is_empty() => (DEFAULT_PADDING, DEFAULT_PADDING),
        None => (0.0, 0.0),
    };
    let mut rem = [x + padx, y + pady, (w - 2.0 * padx).max(0.0), (h - 2.0 * pady).max(0.0)];

    // Pass 1: docked anchors claim their edge, in script order
    for c in ui.children.iter_mut().filter(|c| DOCKED.contains(&c.anchor().as_str())) {
        let (nw, nh) = c.natural();
        let r = match c.anchor().as_str() {
            "top_bar" => {
                let r = [rem[0], rem[1], rem[2], nh];
                rem[1] += nh;
                rem[3] = (rem[3] - nh).max(0.0);
                r
            }
            "bottom_bar" => {
                let r = [rem[0], rem[1] + rem[3] - nh, rem[2], nh];
                rem[3] = (rem[3] - nh).max(0.0);
                r
            }
            "side_left" => {
                let r = [rem[0], rem[1], nw, rem[3]];
                rem[0] += nw;
                rem[2] = (rem[2] - nw).max(0.0);
                r
            }
            _ => {
                let r = [rem[0] + rem[2] - nw, rem[1], nw, rem[3]];
                rem[2] = (rem[2] - nw).max(0.0);
                r
            }
        };
        place(c, r, rect);
    }

    // Pass 2: floating anchors inside the space left over; un-anchored items stack top-down
    let mut flow = rem[1];
    // gap=8: space between items stacked top-down
    let gap = ui.num("gap").unwrap_or(0.0);
    for c in ui.children.iter_mut() {
        let a = c.anchor();
        if DOCKED.contains(&a.as_str()) || a == "fill" {
            continue;
        }
        let (nw, nh) = c.natural();
        let (cx, cy) = match a.as_str() {
            "center" => (rem[0] + (rem[2] - nw) / 2.0, rem[1] + (rem[3] - nh) / 2.0),
            "top_left" => (rem[0], rem[1]),
            "top_right" => (rem[0] + rem[2] - nw, rem[1]),
            "bottom_left" => (rem[0], rem[1] + rem[3] - nh),
            "bottom_right" => (rem[0] + rem[2] - nw, rem[1] + rem[3] - nh),
            _ => {
                let p = (rem[0], flow);
                flow += nh + gap;
                p
            }
        };
        place(c, [cx, cy, nw, nh], rect);
    }

    // Pass 3: fill floods whatever is left
    for c in ui.children.iter_mut().filter(|c| c.anchor() == "fill") {
        place(c, rem, rect);
    }
}

/// Children are trapped inside the parent: overflow is clipped.
fn place(c: &mut Ui, r: [f64; 4], parent: [f64; 4]) {
    let x0 = r[0].max(parent[0]);
    let y0 = r[1].max(parent[1]);
    let x1 = (r[0] + r[2]).min(parent[0] + parent[2]);
    let y1 = (r[1] + r[3]).min(parent[1] + parent[3]);
    let clipped = x0 != r[0] || y0 != r[1] || x1 != r[0] + r[2] || y1 != r[1] + r[3];
    layout(c, [x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0)]);
    c.clipped = clipped;
}

/// (path of child indexes, element kind, its `on ...` lines)
pub type GuiEvents = Vec<(Vec<usize>, String, Vec<Stmt>)>;

fn to_ui(it: &mut Interp, n: &DeclNode, scope: &Rc<Scope>, path: Vec<usize>, events: &mut GuiEvents) -> R<Ui> {
    let label = match &n.label {
        Some(e) => Some(it.eval(e, scope)?.display()),
        None => None,
    };
    let mut props = vec![];
    for (k, e) in &n.props {
        props.push((k.clone(), it.eval(e, scope)?));
    }
    // style=neon: the style's lines first, then this element's own props win
    if let Some(i) = props.iter().position(|(k, _)| k == "style") {
        let name = props.remove(i).1.display();
        let st = match scope.lookup(&name).map(|v| v.borrow().get().clone()) {
            Some(Value::Obj(o)) if o.type_name == "style" => o,
            _ => return Err(EzaError::runtime(it.line, format!("there's no style called '{0}' - define one with: style {0}", name))),
        };
        let mut merged = st.fields.clone();
        for (k, v) in props {
            match merged.iter_mut().find(|(n, _)| *n == k) {
                Some(slot) => slot.1 = v,
                None => merged.push((k, v)),
            }
        }
        props = merged;
    }
    if !n.events.is_empty() {
        events.push((path.clone(), n.kind.clone(), n.events.clone()));
        props.push(("hoverable".to_string(), Value::Bool(true)));
    }
    let mut children = vec![];
    for (i, c) in n.children.iter().enumerate() {
        let mut p = path.clone();
        p.push(i);
        children.push(to_ui(it, c, scope, p, events)?);
    }
    let handler = n.handler.as_ref().map(|h| it.make_func("on_click", h.clone(), scope));
    Ok(Ui { kind: n.kind.clone(), label, props, children, handler, rect: [0.0; 4], clipped: false })
}

fn to_value(ui: &Ui) -> Value {
    let mut o = Obj::new(&ui.kind);
    o.set("kind", Value::Str(ui.kind.clone()));
    if let Some(l) = &ui.label {
        o.set("label", Value::Str(l.clone()));
    }
    for (k, v) in &ui.props {
        o.set(k, v.clone());
    }
    o.set("x", Value::Num(ui.rect[0]));
    o.set("y", Value::Num(ui.rect[1]));
    o.set("width", Value::Num(ui.rect[2]));
    o.set("height", Value::Num(ui.rect[3]));
    o.set("clipped", Value::Bool(ui.clipped));
    // which sizes the script chose itself (terminal apps lay things out again in letters and lines)
    let (ew, eh) = ui.explicit();
    if ew.is_some() {
        o.set("fixed_width", Value::Bool(true));
    }
    if eh.is_some() {
        o.set("fixed_height", Value::Bool(true));
    }
    if o.get("visible").is_none() {
        o.set("visible", Value::Bool(true));
    }
    o.set("children", Value::list(ui.children.iter().map(to_value).collect()));
    if let Some(h) = &ui.handler {
        o.set("on_click", h.clone());
    }
    Value::obj(o)
}

pub fn build_gui(it: &mut Interp, n: &DeclNode, scope: &Rc<Scope>) -> R<(Value, GuiEvents)> {
    let mut events = vec![];
    let mut ui = to_ui(it, n, scope, vec![], &mut events)?;
    let (w, h) = ui.natural();
    let centered = matches!(ui.prop("centered"), Some(Value::Bool(true)));
    let (x, y) = if centered {
        ((SCREEN.0 - w) / 2.0, (SCREEN.1 - h) / 2.0)
    } else {
        (ui.num("x").unwrap_or(0.0), ui.num("y").unwrap_or(0.0))
    };
    layout(&mut ui, [x, y, w, h]);
    Ok((to_value(&ui), events))
}
