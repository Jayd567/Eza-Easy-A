//! Shared 2D geometry: sprite boxes, origin points, tilemap cells and ray tests.
//! World space is y-up (like the 3D world); `position` is where the sprite's origin point sits.
use crate::value::*;

pub fn vec2(v: Option<&Value>) -> Option<[f64; 2]> {
    match v {
        Some(Value::List(l)) if l.len() == 2 => match (&l[0], &l[1]) {
            (Value::Num(x), Value::Num(y)) => Some([*x, *y]),
            _ => None,
        },
        _ => None,
    }
}

pub fn is_2d(o: &Obj) -> bool {
    vec2(o.get("position")).is_some()
}

fn num(o: &Obj, k: &str, d: f64) -> f64 {
    match o.get(k) {
        Some(Value::Num(n)) => *n,
        _ => d,
    }
}

/// `scale=2` or `scale=2,1`
pub fn scale2(o: &Obj) -> [f64; 2] {
    match o.get("scale") {
        Some(Value::Num(s)) => [*s, *s],
        v => vec2(v).unwrap_or([1.0, 1.0]),
    }
}

/// On-screen size in world units (width/height times scale).
pub fn size(o: &Obj) -> [f64; 2] {
    let s = scale2(o);
    [num(o, "width", 32.0) * s[0].abs(), num(o, "height", 32.0) * s[1].abs()]
}

/// Where the origin sits inside the sprite, from -0.5 (left/bottom) to 0.5 (right/top).
/// `origin=bottom_center`, or `origin=16,16` in pixels from the image's top-left.
pub fn origin_frac(o: &Obj) -> [f64; 2] {
    let raw = [num(o, "width", 32.0).max(1e-9), num(o, "height", 32.0).max(1e-9)];
    match o.get("origin") {
        Some(Value::Str(s)) => match s.as_str() {
            "top_left" => [-0.5, 0.5],
            "top_center" | "top" => [0.0, 0.5],
            "top_right" => [0.5, 0.5],
            "center_left" | "left" => [-0.5, 0.0],
            "center_right" | "right" => [0.5, 0.0],
            "bottom_left" => [-0.5, -0.5],
            "bottom_center" | "bottom" => [0.0, -0.5],
            "bottom_right" => [0.5, -0.5],
            _ => [0.0, 0.0],
        },
        Some(v) => match vec2(Some(v)) {
            Some([px, py]) => [px / raw[0] - 0.5, 0.5 - py / raw[1]],
            None => [0.0, 0.0],
        },
        // a tilemap's position is its top-left corner
        None if o.type_name == "tilemap" => [-0.5, 0.5],
        None => [0.0, 0.0],
    }
}

/// (center, half size) of a 2D object's box.
pub fn aabb2d(o: &Obj) -> Option<([f64; 2], [f64; 2])> {
    let p = vec2(o.get("position"))?;
    let sz = size(o);
    let f = origin_frac(o);
    Some(([p[0] - f[0] * sz[0], p[1] - f[1] * sz[1]], [sz[0] / 2.0, sz[1] / 2.0]))
}

pub fn boxes_touch(ca: [f64; 2], ha: [f64; 2], cb: [f64; 2], hb: [f64; 2]) -> bool {
    (0..2).all(|i| (ca[i] - cb[i]).abs() <= ha[i] + hb[i] + 0.01)
}

pub struct Tile {
    pub center: [f64; 2],
    pub half: [f64; 2],
    pub index: i64,
    pub col: usize,
    pub row: usize,
}

/// Non-empty tiles of a tilemap that overlap the box lo..hi (world units).
pub fn tiles_in(map: &Obj, lo: [f64; 2], hi: [f64; 2]) -> Vec<Tile> {
    let mut out = vec![];
    let (Some(p), Some(Value::List(grid))) = (vec2(map.get("position")), map.get("grid")) else { return out };
    let ts = num(map, "tile_size", 32.0);
    if ts <= 0.0 || grid.is_empty() {
        return out;
    }
    let (c0, c1) = (((lo[0] - p[0]) / ts).floor(), ((hi[0] - p[0]) / ts).floor());
    let (r0, r1) = (((p[1] - hi[1]) / ts).floor(), ((p[1] - lo[1]) / ts).floor());
    if c1 < 0.0 || r1 < 0.0 {
        return out;
    }
    let r_end = (r1 as usize).min(grid.len() - 1);
    for r in (r0.max(0.0) as usize)..=r_end {
        let Some(Value::List(row)) = grid.get(r) else { continue };
        if row.is_empty() || c0.max(0.0) as usize >= row.len() {
            continue;
        }
        let c_end = (c1 as usize).min(row.len() - 1);
        for c in (c0.max(0.0) as usize)..=c_end {
            if let Value::Num(i) = row[c] {
                if i >= 0.0 {
                    let center = [p[0] + (c as f64 + 0.5) * ts, p[1] - (r as f64 + 0.5) * ts];
                    out.push(Tile { center, half: [ts / 2.0, ts / 2.0], index: i as i64, col: c, row: r });
                }
            }
        }
    }
    out
}

/// The tile index at a world point, or -1 for an empty spot.
pub fn tile_at(map: &Obj, pos: [f64; 2]) -> i64 {
    tiles_in(map, pos, pos).first().map_or(-1, |t| t.index)
}

fn solid(o: &Obj) -> bool {
    !matches!(o.get("solid"), Some(Value::Bool(false)))
}

/// 2D `collides_with`, including against the solid tiles of a tilemap.
pub fn overlap(a: &Obj, b: &Obj) -> bool {
    if a.type_name == "tilemap" && b.type_name != "tilemap" {
        return overlap(b, a);
    }
    let Some((ca, ha)) = aabb2d(a) else { return false };
    if b.type_name == "tilemap" {
        if !solid(b) {
            return false;
        }
        let (lo, hi) = ([ca[0] - ha[0], ca[1] - ha[1]], [ca[0] + ha[0], ca[1] + ha[1]]);
        return tiles_in(b, lo, hi).iter().any(|t| boxes_touch(ca, ha, t.center, t.half));
    }
    let Some((cb, hb)) = aabb2d(b) else { return false };
    boxes_touch(ca, ha, cb, hb)
}

/// Distance along a ray (unit `dir`) to a box, ignoring boxes the ray starts inside (like its caster).
pub fn ray_box(from: &[f64], dir: &[f64], c: &[f64], h: &[f64]) -> Option<f64> {
    let (mut tmin, mut tmax) = (f64::NEG_INFINITY, f64::INFINITY);
    for i in 0..from.len() {
        let (lo, hi) = (c[i] - h[i], c[i] + h[i]);
        if dir[i].abs() < 1e-12 {
            if from[i] < lo || from[i] > hi {
                return None;
            }
        } else {
            let (t1, t2) = ((lo - from[i]) / dir[i], (hi - from[i]) / dir[i]);
            tmin = tmin.max(t1.min(t2));
            tmax = tmax.min(t1.max(t2));
        }
    }
    if tmax < tmin || tmin <= 0.0 {
        None
    } else {
        Some(tmin)
    }
}
