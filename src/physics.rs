//! Basic physics: gravity plus box collisions against solid scene objects.
//! Opt in per entity with `physics=true`. Meshes (plane, cube, cylinder, sphere) are solid by default;
//! set `solid=false` to walk through one, or `solid=true` on any other scene node to make it an obstacle.
use crate::error::R;
use crate::interp::Interp;
use crate::methods::aabb_with;
use crate::value::*;

/// Velocities are in units per second; one tick is one 60 fps frame.
const DT: f64 = 1.0 / 60.0;
const DEFAULT_GRAVITY: f64 = -20.0;
const SOLID_KINDS: &[&str] = &["plane", "cube", "cylinder", "sphere"];

type Box3 = ([f64; 3], [f64; 3]);

fn vec3(v: Option<&Value>) -> Option<[f64; 3]> {
    if let Some(Value::List(l)) = v {
        if let (Some(Value::Num(x)), Some(Value::Num(y)), Some(Value::Num(z))) = (l.first(), l.get(1), l.get(2)) {
            return Some([*x, *y, *z]);
        }
    }
    None
}

fn list3(a: [f64; 3]) -> Value {
    Value::list(a.iter().map(|n| Value::Num(*n)).collect())
}

fn collect_solids(v: &Value, out: &mut Vec<Box3>) {
    let Value::Obj(o) = v else { return };
    let moving = matches!(o.get("physics"), Some(Value::Bool(true)));
    let solid = match o.get("solid") {
        Some(Value::Bool(b)) => *b,
        _ => SOLID_KINDS.contains(&o.type_name.as_str()),
    };
    if solid && !moving {
        if let Some(b) = aabb_with(o, true) {
            out.push(b);
        }
    }
    if let Some(Value::List(ch)) = o.get("children") {
        for c in ch.iter() {
            collect_solids(c, out);
        }
    }
}

fn close(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9)
}

/// Advances every `physics=true` entity (scene entities and spawned ones) by one frame.
pub fn step(it: &mut Interp) -> R<()> {
    let bodies = it.physics_vars();
    if bodies.is_empty() {
        return Ok(());
    }
    let mut solids = vec![];
    if let Some(scene) = it.global("scene") {
        collect_solids(&scene, &mut solids);
    }
    for v in it.solid_values() {
        collect_solids(&v, &mut solids);
    }
    // `scene gravity=-20` (like `stage gravity=`), or the older `param gravity = -20`
    let from_scene = match it.global("scene") {
        Some(Value::Obj(s)) => match s.get("gravity") {
            Some(Value::Num(g)) => Some(*g),
            _ => None,
        },
        _ => None,
    };
    let gravity = match (from_scene, it.global("gravity")) {
        (Some(g), _) => g,
        (None, Some(Value::Num(g))) => g,
        _ => DEFAULT_GRAVITY,
    };
    for var in bodies {
        let Value::Obj(o) = var.borrow().get().clone() else { continue };
        if matches!(o.get("destroyed"), Some(Value::Bool(true))) {
            continue;
        }
        let (Some(old_pos), Some((_, half))) = (vec3(o.get("position")), aabb_with(&o, true)) else { continue };
        let old_vel = vec3(o.get("velocity")).unwrap_or([0.0; 3]);
        let (mut pos, mut vel) = (old_pos, old_vel);

        vel[1] += gravity * DT;
        for i in 0..3 {
            pos[i] += vel[i] * DT;
        }
        let mut grounded = false;
        // a few passes so corners resolve cleanly
        for _ in 0..3 {
            for (c, h) in &solids {
                let d = [pos[0] - c[0], pos[1] - c[1], pos[2] - c[2]];
                let pen = [half[0] + h[0] - d[0].abs(), half[1] + h[1] - d[1].abs(), half[2] + h[2] - d[2].abs()];
                if pen.iter().any(|p| *p <= 0.0) {
                    continue;
                }
                // push out along the axis with the smallest overlap
                let axis = (0..3).min_by(|&a, &b| pen[a].partial_cmp(&pen[b]).unwrap()).unwrap();
                let sign = if d[axis] >= 0.0 { 1.0 } else { -1.0 };
                pos[axis] += sign * pen[axis];
                if vel[axis] * sign < 0.0 {
                    vel[axis] = 0.0;
                }
                if axis == 1 && sign > 0.0 {
                    grounded = true;
                }
            }
        }
        let was_grounded = matches!(o.get("grounded"), Some(Value::Bool(true)));
        if close(pos, old_pos) && close(vel, old_vel) && grounded == was_grounded {
            continue; // at rest: don't spam the history
        }
        let mut n = (*o).clone();
        n.set("position", list3(pos));
        n.set("velocity", list3(vel));
        n.set("grounded", Value::Bool(grounded));
        it.commit(&var, Value::obj(n));
    }
    Ok(())
}

/// Advances every 2D `physics=true` sprite by one frame: gravity (the stage's `gravity`,
/// default -980 pixels/s^2), then pushes it out of solid sprites and tilemap tiles.
pub fn step2d(it: &mut Interp) -> R<()> {
    use crate::two_d::{aabb2d, is_2d, tiles_in, vec2};
    let bodies: Vec<_> = it
        .physics_vars()
        .into_iter()
        .filter(|v| matches!(v.borrow().get(), Value::Obj(o) if is_2d(o)))
        .collect();
    if bodies.is_empty() {
        return Ok(());
    }
    let stage = it.global("stage");
    let gravity = match &stage {
        Some(Value::Obj(s)) => match s.get("gravity") {
            Some(Value::Num(g)) => *g,
            _ => -980.0,
        },
        _ => -980.0,
    };
    // static obstacles: sprites with solid=true, and tilemaps (solid unless solid=false)
    let mut boxes: Vec<([f64; 2], [f64; 2])> = vec![];
    let mut maps: Vec<Value> = vec![];
    let mut add = |v: &Value| {
        let Value::Obj(o) = v else { return };
        if matches!(o.get("physics"), Some(Value::Bool(true))) || matches!(o.get("visible"), Some(Value::Bool(false))) {
            return;
        }
        if o.type_name == "tilemap" {
            if !matches!(o.get("solid"), Some(Value::Bool(false))) {
                maps.push(v.clone());
            }
        } else if matches!(o.get("solid"), Some(Value::Bool(true))) {
            if let Some(b) = aabb2d(o) {
                boxes.push(b);
            }
        }
    };
    if let Some(Value::Obj(s)) = &stage {
        if let Some(Value::List(kids)) = s.get("children") {
            kids.iter().for_each(&mut add);
        }
    }
    it.solid_values().iter().for_each(&mut add);

    for var in bodies {
        let Value::Obj(o) = var.borrow().get().clone() else { continue };
        if matches!(o.get("destroyed"), Some(Value::Bool(true))) {
            continue;
        }
        let (Some(old_pos), Some((c0, half))) = (vec2(o.get("position")), aabb2d(&o)) else { continue };
        let offset = [c0[0] - old_pos[0], c0[1] - old_pos[1]];
        let old_vel = vec2(o.get("velocity")).unwrap_or([0.0, 0.0]);
        let (mut pos, mut vel) = (old_pos, old_vel);
        vel[1] += gravity * DT;
        let mut grounded = false;
        // move along x, fix overlaps along x; then the same for y. Resolving one axis at a time
        // stops bodies from snagging on the seams between neighbouring tiles.
        for axis in 0..2 {
            pos[axis] += vel[axis] * DT;
            let c = [pos[0] + offset[0], pos[1] + offset[1]];
            let (lo, hi) = ([c[0] - half[0] - 1.0, c[1] - half[1] - 1.0], [c[0] + half[0] + 1.0, c[1] + half[1] + 1.0]);
            let mut nearby = boxes.clone();
            for m in &maps {
                if let Value::Obj(mo) = m {
                    nearby.extend(tiles_in(mo, lo, hi).into_iter().map(|t| (t.center, t.half)));
                }
            }
            for (bc, bh) in nearby {
                let c = [pos[0] + offset[0], pos[1] + offset[1]];
                let d = [c[0] - bc[0], c[1] - bc[1]];
                let pen = [half[0] + bh[0] - d[0].abs(), half[1] + bh[1] - d[1].abs()];
                if pen[0] <= 1e-6 || pen[1] <= 1e-6 {
                    continue;
                }
                // push back against the direction of travel on this axis
                let sign = if vel[axis] > 0.0 {
                    -1.0
                } else if vel[axis] < 0.0 {
                    1.0
                } else if d[axis] >= 0.0 {
                    1.0
                } else {
                    -1.0
                };
                pos[axis] += sign * pen[axis];
                if vel[axis] * sign < 0.0 {
                    vel[axis] = 0.0;
                }
                if axis == 1 && sign > 0.0 {
                    grounded = true;
                }
            }
        }
        let was_grounded = matches!(o.get("grounded"), Some(Value::Bool(true)));
        let same = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9;
        if same(pos, old_pos) && same(vel, old_vel) && grounded == was_grounded {
            continue;
        }
        let mut n = (*o).clone();
        n.set("position", Value::list(vec![Value::Num(pos[0]), Value::Num(pos[1])]));
        n.set("velocity", Value::list(vec![Value::Num(vel[0]), Value::Num(vel[1])]));
        n.set("grounded", Value::Bool(grounded));
        it.commit(&var, Value::obj(n));
    }
    Ok(())
}
