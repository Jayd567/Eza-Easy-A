//! `chart "Sales" kind=bar data=sales` in a gui: bar, line and pie charts that redraw whenever the
//! variable they show changes. The shapes are drawn into a picture; the labels are normal text.
use super::*;
use crate::value::fmt_num;

/// Drawing happens at twice the size, so edges stay crisp after scaling down.
const S: f32 = 2.0;
const PALETTE: [u32; 8] = [0x4C9AFF, 0xFF8A65, 0x81C784, 0xFFD54F, 0xBA68C8, 0x4DD0E1, 0xF06292, 0xA1887F];

#[derive(Clone)]
pub(super) enum Source {
    /// the name of a variable (looked up every frame)
    Var(String),
    /// labels and numbers; keyed when they came from a dictionary (so the keys are the labels)
    Fixed(Vec<String>, Vec<f64>, bool),
    None,
}

#[derive(Component)]
pub(super) struct ChartView {
    kind: String,
    data: Source,
    labels: Source,
    colors: Vec<[f32; 4]>,
    title: String,
    size: (f32, f32),
    text: Color,
    font: TextFont,
    image: Handle<Image>,
    last: String,
    texts: Vec<Entity>,
}

fn source(v: Option<&Value>) -> Source {
    match v {
        Some(Value::Str(name)) => Source::Var(name.clone()),
        Some(Value::List(l)) => Source::Fixed(l.iter().map(|x| x.display()).collect(), l.iter().map(|x| x.as_num(0).unwrap_or(0.0)).collect(), false),
        Some(Value::Obj(o)) => Source::Fixed(o.fields.iter().map(|(k, _)| k.clone()).collect(), o.fields.iter().map(|(_, x)| x.as_num(0).unwrap_or(0.0)).collect(), true),
        _ => Source::None,
    }
}

fn rgba(c: Color) -> [f32; 4] {
    let s = c.to_srgba();
    [s.red, s.green, s.blue, s.alpha]
}

pub(super) fn spawn_chart(commands: &mut Commands, images: &mut Assets<Image>, e: Entity, o: &Obj, w: f32, h: f32, text: Color, font: TextFont) -> ChartView {
    let mut colors: Vec<[f32; 4]> = match o.get("colors") {
        Some(v) => parts(v).iter().filter_map(|c| color_of(Some(c))).map(rgba).collect(),
        None => vec![],
    };
    if let Some(c) = color_of(o.get("color")) {
        colors.insert(0, rgba(c));
    }
    if colors.is_empty() {
        colors = PALETTE.iter().map(|h| [((h >> 16) & 255) as f32 / 255.0, ((h >> 8) & 255) as f32 / 255.0, (h & 255) as f32 / 255.0, 1.0]).collect();
    }
    let (pw, ph) = ((w * S).max(2.0) as u32, (h * S).max(2.0) as u32);
    let img = Image::new(
        Extent3d { width: pw, height: ph, depth_or_array_layers: 1 },
        TextureDimension::D2,
        vec![0; (pw * ph * 4) as usize],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    let image = images.add(img);
    let pic = commands
        .spawn((ImageNode::new(image.clone()), Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), width: Val::Px(w), height: Val::Px(h), ..default() }))
        .id();
    commands.entity(e).add_child(pic);
    ChartView {
        kind: o.get("kind").or(o.get("type")).map(|k| k.display()).unwrap_or_else(|| "bar".into()),
        data: source(o.get("data")),
        labels: source(o.get("labels")),
        colors,
        title: o.get("label").map(|l| l.display()).unwrap_or_default(),
        size: (w, h),
        text,
        font: TextFont { font_size: 12.0, ..font },
        image,
        last: String::new(),
        texts: vec![],
    }
}

/// (labels, numbers) the chart should show right now.
fn series(rt: &Runtime, c: &ChartView) -> (Vec<String>, Vec<f64>) {
    let resolve = |s: &Source| -> Source {
        match s {
            Source::Var(name) => source(rt.it.global(name).as_ref()),
            other => other.clone(),
        }
    };
    let (mut labels, values, keyed) = match resolve(&c.data) {
        Source::Fixed(l, v, k) => (l, v, k),
        _ => (vec![], vec![], false),
    };
    // a list of numbers gets its labels from `labels=` (or 1, 2, 3...)
    if !keyed {
        labels = match resolve(&c.labels) {
            Source::Fixed(l, _, _) => l,
            _ => vec![],
        };
        while labels.len() < values.len() {
            labels.push(format!("{}", labels.len() + 1));
        }
    }
    (labels, values)
}

struct Canvas {
    w: usize,
    h: usize,
    px: Vec<f32>,
}

impl Canvas {
    fn blend(&mut self, x: i64, y: i64, c: [f32; 4], cover: f32) {
        if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h {
            return;
        }
        let a = c[3] * cover.clamp(0.0, 1.0);
        if a <= 0.0 {
            return;
        }
        // "over" blending with straight (not premultiplied) alpha
        let i = (y as usize * self.w + x as usize) * 4;
        let da = self.px[i + 3];
        let out = a + da * (1.0 - a);
        for k in 0..3 {
            self.px[i + k] = (c[k] * a + self.px[i + k] * da * (1.0 - a)) / out;
        }
        self.px[i + 3] = out;
    }
    /// all coordinates in logical pixels
    fn rect(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, c: [f32; 4]) {
        let (x0, x1) = ((x0.min(x1) * S) as i64, (x0.max(x1) * S) as i64);
        let (y0, y1) = ((y0.min(y1) * S) as i64, (y0.max(y1) * S) as i64);
        for y in y0..y1 {
            for x in x0..x1 {
                self.blend(x, y, c, 1.0);
            }
        }
    }
    fn disc(&mut self, cx: f32, cy: f32, r: f32, c: [f32; 4]) {
        let (cx, cy, r) = (cx * S, cy * S, r * S);
        for y in (cy - r - 1.0) as i64..=(cy + r + 1.0) as i64 {
            for x in (cx - r - 1.0) as i64..=(cx + r + 1.0) as i64 {
                let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                self.blend(x, y, c, r - d + 0.5);
            }
        }
    }
    fn line(&mut self, a: (f32, f32), b: (f32, f32), width: f32, c: [f32; 4]) {
        let (ax, ay, bx, by, hw) = (a.0 * S, a.1 * S, b.0 * S, b.1 * S, width * S / 2.0);
        let (minx, maxx) = (ax.min(bx) - hw - 1.0, ax.max(bx) + hw + 1.0);
        let (miny, maxy) = (ay.min(by) - hw - 1.0, ay.max(by) + hw + 1.0);
        let (dx, dy) = (bx - ax, by - ay);
        let len2 = (dx * dx + dy * dy).max(1e-6);
        for y in miny as i64..=maxy as i64 {
            for x in minx as i64..=maxx as i64 {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let t = (((px - ax) * dx + (py - ay) * dy) / len2).clamp(0.0, 1.0);
                let d = ((px - ax - t * dx).powi(2) + (py - ay - t * dy).powi(2)).sqrt();
                self.blend(x, y, c, hw - d + 0.5);
            }
        }
    }
    /// fills the area under a line chart
    fn area_under(&mut self, pts: &[(f32, f32)], base: f32, c: [f32; 4]) {
        for w in pts.windows(2) {
            let ((x0, y0), (x1, y1)) = (w[0], w[1]);
            for x in (x0 * S) as i64..(x1 * S) as i64 {
                let t = (x as f32 / S - x0) / (x1 - x0).max(1e-6);
                let top = (y0 + (y1 - y0) * t) * S;
                for y in top as i64..(base * S) as i64 {
                    self.blend(x, y, c, 1.0);
                }
            }
        }
    }
    fn slice(&mut self, cx: f32, cy: f32, r: f32, from: f32, to: f32, c: [f32; 4]) {
        let (cx, cy, r) = (cx * S, cy * S, r * S);
        for y in (cy - r - 1.0) as i64..=(cy + r + 1.0) as i64 {
            for x in (cx - r - 1.0) as i64..=(cx + r + 1.0) as i64 {
                let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                let d = (dx * dx + dy * dy).sqrt();
                // angle measured clockwise from the top, 0..1
                let a = (dx.atan2(-dy) / std::f32::consts::TAU).rem_euclid(1.0);
                if a >= from && a < to {
                    self.blend(x, y, c, r - d + 0.5);
                }
            }
        }
    }
    fn bytes(&self) -> Vec<u8> {
        self.px.iter().map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8).collect()
    }
}

/// A round number at or above `m` (for the top of the scale), and how many grid steps suit it.
fn nice(m: f64) -> (f64, usize) {
    if m <= 0.0 {
        return (1.0, 5);
    }
    let p = 10f64.powf(m.log10().floor());
    for s in [1.0, 2.0, 2.5, 5.0, 10.0] {
        if s * p >= m - 1e-9 {
            return (s * p, if s == 2.0 { 4 } else { 5 });
        }
    }
    (10.0 * p, 5)
}

fn short(n: f64) -> String {
    fmt_num((n * 100.0).round() / 100.0)
}

/// A label to place: text, x, y, width, alignment.
type Label = (String, f32, f32, f32, JustifyText, Option<[f32; 4]>);

fn draw(c: &ChartView, labels: &[String], values: &[f64]) -> (Vec<u8>, Vec<Label>) {
    let (w, h) = c.size;
    let mut cv = Canvas { w: (w * S) as usize, h: (h * S) as usize, px: vec![0.0; (w * S) as usize * (h * S) as usize * 4] };
    let mut out: Vec<Label> = vec![];
    let top = if c.title.is_empty() { 10.0 } else { 30.0 };
    if !c.title.is_empty() {
        out.push((c.title.clone(), 0.0, 6.0, w, JustifyText::Center, None));
    }
    let faint = { let t = rgba(c.text); [t[0], t[1], t[2], 0.12] };
    if values.is_empty() {
        out.push(("no data".into(), 0.0, h / 2.0 - 8.0, w, JustifyText::Center, None));
        return (cv.bytes(), out);
    }
    let color = |i: usize| c.colors[i % c.colors.len()];
    if c.kind == "pie" {
        let total: f64 = values.iter().map(|v| v.max(0.0)).sum::<f64>().max(1e-12);
        let r = ((h - top - 12.0) / 2.0).min(w * 0.28).max(10.0);
        let (cx, cy) = (12.0 + r, top + (h - top) / 2.0);
        let mut from = 0.0f32;
        for (i, v) in values.iter().enumerate() {
            let part = (v.max(0.0) / total) as f32;
            cv.slice(cx, cy, r, from, from + part, color(i));
            from += part;
        }
        // legend on the right
        let lx = cx + r + 18.0;
        let row = 18.0;
        let start = cy - row * values.len() as f32 / 2.0;
        for (i, v) in values.iter().enumerate() {
            let y = start + i as f32 * row;
            cv.rect(lx, y + 3.0, lx + 10.0, y + 13.0, color(i));
            let pct = v.max(0.0) / total * 100.0;
            out.push((format!("{}  {} ({}%)", labels.get(i).cloned().unwrap_or_default(), short(*v), pct.round()), lx + 16.0, y, w - lx - 16.0, JustifyText::Left, None));
        }
        return (cv.bytes(), out);
    }
    // bar and line charts share the axes
    let (left, right, bottom) = (46.0, w - 12.0, h - 24.0);
    let (hi, steps) = nice(values.iter().cloned().fold(0.0, f64::max));
    let lowest = values.iter().cloned().fold(0.0, f64::min);
    let lo = if lowest < 0.0 { -nice(-lowest).0 } else { 0.0 };
    let to_y = |v: f64| bottom - ((v - lo) / (hi - lo)) as f32 * (bottom - top);
    for k in 0..=steps {
        let v = lo + (hi - lo) * k as f64 / steps as f64;
        let y = to_y(v);
        cv.rect(left, y, right, y + 1.0, faint);
        out.push((short(v), 0.0, y - 8.0, left - 6.0, JustifyText::Right, None));
    }
    let n = values.len();
    let slot = (right - left) / n as f32;
    // skip x labels when they would overlap
    let longest = labels.iter().map(|l| l.chars().count()).max().unwrap_or(1).max(1) as f32;
    let every = ((longest * 7.0 + 6.0) / slot).ceil().max(1.0) as usize;
    let zero = to_y(0.0);
    if c.kind == "line" {
        let pts: Vec<(f32, f32)> = values.iter().enumerate().map(|(i, v)| (left + (i as f32 + 0.5) * slot, to_y(*v))).collect();
        let col = color(0);
        cv.area_under(&pts, zero, [col[0], col[1], col[2], 0.15]);
        for p in pts.windows(2) {
            cv.line(p[0], p[1], 2.5, col);
        }
        for p in &pts {
            cv.disc(p.0, p.1, 3.5, col);
        }
    } else {
        for (i, v) in values.iter().enumerate() {
            let x = left + i as f32 * slot + slot * 0.15;
            let c2 = if c.colors.len() == 1 { color(0) } else { color(i) };
            cv.rect(x, to_y(*v), x + slot * 0.7, zero, c2);
        }
    }
    for (i, l) in labels.iter().enumerate().take(n) {
        if i % every == 0 {
            out.push((l.clone(), left + i as f32 * slot - slot * (every as f32 - 1.0) / 2.0, bottom + 4.0, slot * every as f32, JustifyText::Center, None));
        }
    }
    (cv.bytes(), out)
}

pub(super) fn chart_sync(mut commands: Commands, rt: NonSend<Runtime>, mut images: ResMut<Assets<Image>>, mut charts: Query<(Entity, &mut ChartView)>) {
    for (e, mut c) in &mut charts {
        let (labels, values) = series(&rt, &c);
        let sig = format!("{:?}{:?}", labels, values);
        if sig == c.last {
            continue;
        }
        c.last = sig;
        let (bytes, texts) = draw(&c, &labels, &values);
        if let Some(img) = images.get_mut(&c.image) {
            img.data = bytes;
        }
        for t in std::mem::take(&mut c.texts) {
            commands.entity(t).despawn_recursive();
        }
        for (text, x, y, width, justify, _) in texts {
            let side = match justify {
                JustifyText::Center => JustifyContent::Center,
                JustifyText::Right => JustifyContent::FlexEnd,
                _ => JustifyContent::FlexStart,
            };
            let t = commands
                .spawn(Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(x),
                    top: Val::Px(y),
                    width: Val::Px(width.max(1.0)),
                    justify_content: side,
                    overflow: Overflow::clip(),
                    ..default()
                })
                .with_children(|p| {
                    p.spawn((Text::new(text), TextColor(c.text.with_alpha(0.85)), c.font.clone(), TextLayout::new_with_no_wrap()));
                })
                .id();
            commands.entity(e).add_child(t);
            c.texts.push(t);
        }
    }
}
