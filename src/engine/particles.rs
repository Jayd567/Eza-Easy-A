//! `particles name="sparks" rate=30 life=1 speed=120 color=#FFCC00` inside a `stage` or `scene`,
//! plus `emit 40 from sparks [at x, y]` for bursts. Particles live only on screen (they aren't rewound).
use super::*;
use std::f32::consts::{PI, TAU};

const MAX_PARTICLES: usize = 4000;

#[derive(Component)]
pub(super) struct Emitter {
    pub(super) source: Source,
    pub(super) acc: f32,
    pub(super) two_d: bool,
    pub(super) z: f32,
}

#[derive(Component)]
pub(super) struct Particle {
    vel: Vec3,
    age: f32,
    life: f32,
    size: (f32, f32),
    color: (Srgba, Srgba),
    gravity: f32,
    mat: Option<Handle<StandardMaterial>>,
}

fn source_value(rt: &Runtime, s: &Source) -> Option<Value> {
    match s {
        Source::Global(n) => rt.it.global(n),
        Source::Scene(p) => rt.it.global("scene").and_then(|v| child_at(&v, p)),
        Source::Stage(p) => rt.it.global("stage").and_then(|v| child_at(&v, p)),
        Source::Dyn(id) => rt.it.entity_value(*id),
    }
}

fn nums(v: Option<&Value>) -> Vec<f32> {
    match v {
        Some(Value::List(l)) => l.iter().filter_map(|x| x.as_num(0).ok()).map(|x| x as f32).collect(),
        Some(Value::Num(n)) => vec![*n as f32],
        _ => vec![],
    }
}

/// A random direction at most `half` radians away from `d` (half = PI: any direction).
fn cone(d: Vec3, half: f32, r1: f32, r2: f32) -> Vec3 {
    let cos_t = 1.0 - r1 * (1.0 - half.min(PI).cos());
    let sin_t = (1.0 - cos_t * cos_t).max(0.0).sqrt();
    let phi = r2 * TAU;
    let (u, v) = d.any_orthonormal_pair();
    (d * cos_t + u * sin_t * phi.cos() + v * sin_t * phi.sin()).normalize_or(d)
}

/// A white circle that fades out at the edge, 32x32.
fn soft_dot() -> Image {
    let n = 32u32;
    let mut px = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let (dx, dy) = (x as f32 + 0.5 - n as f32 / 2.0, y as f32 + 0.5 - n as f32 / 2.0);
            let d = (dx * dx + dy * dy).sqrt() / (n as f32 / 2.0);
            let a = (1.0 - d).clamp(0.0, 1.0).powf(0.6);
            px.extend_from_slice(&[255, 255, 255, (a * 255.0) as u8]);
        }
    }
    Image::new(
        Extent3d { width: n, height: n, depth_or_array_layers: 1 },
        TextureDimension::D2,
        px,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

fn mix(a: Srgba, b: Srgba, t: f32) -> Srgba {
    Srgba::new(
        a.red + (b.red - a.red) * t,
        a.green + (b.green - a.green) * t,
        a.blue + (b.blue - a.blue) * t,
        a.alpha + (b.alpha - a.alpha) * t,
    )
}

#[allow(clippy::too_many_arguments)]
fn spawn_particle(
    commands: &mut Commands,
    rt: &mut Runtime,
    images: &mut Assets<Image>,
    mats: &mut Assets<StandardMaterial>,
    ball: &Handle<Mesh>,
    dot: &Handle<Image>,
    em: &Emitter,
    o: &Obj,
    at: &Option<Vec<f64>>,
) {
    let two = em.two_d;
    let pos = match at {
        Some(v) => v.iter().map(|x| *x as f32).collect(),
        None => nums(o.get("position")),
    };
    let get = |i: usize| pos.get(i).copied().unwrap_or(0.0);
    let mut origin = if two { Vec3::new(get(0), get(1), em.z) } else { Vec3::new(get(0), get(1), get(2)) };
    // area=200,10 spreads the starting points over a box (rain, snow, embers)
    let area = nums(o.get("area"));
    for (i, a) in area.iter().enumerate().take(if two { 2 } else { 3 }) {
        origin[i] += (rt.rand() - 0.5) * a;
    }
    let speed = num(o, "speed", if two { 100.0 } else { 3.0 }) * (0.7 + 0.6 * rt.rand());
    let spread = num(o, "spread", 360.0).to_radians();
    let dir = if two {
        let a = num(o, "direction", 90.0).to_radians() + (rt.rand() - 0.5) * spread;
        Vec3::new(a.cos(), a.sin(), 0.0)
    } else {
        let d = vec3(o.get("direction"), Vec3::Y).normalize_or(Vec3::Y);
        let (r1, r2) = (rt.rand(), rt.rand());
        cone(d, spread / 2.0, r1, r2)
    };
    let life = num(o, "life", 1.0).max(0.01) * (0.8 + 0.4 * rt.rand());
    let size0 = num(o, "size", if two { 6.0 } else { 0.15 });
    let size1 = num(o, "end_size", size0);
    let c0 = color_of(o.get("color")).unwrap_or(Color::WHITE).to_srgba();
    // without end_color, particles fade out
    let c1 = color_of(o.get("end_color")).map(|c| c.to_srgba()).unwrap_or(Srgba { alpha: 0.0, ..c0 });
    let p = Particle { vel: dir * speed, age: 0.0, life, size: (size0, size1), color: (c0, c1), gravity: num(o, "gravity", 0.0), mat: None };
    if two {
        // a soft round dot unless the particles have their own texture
        let mut sprite = Sprite { color: c0.into(), custom_size: Some(Vec2::splat(size0)), image: dot.clone(), ..default() };
        let tex = match o.get("texture") {
            Some(Value::Str(path)) => rt.it.load(path).ok(),
            other => other.cloned(),
        };
        if let Some(Value::Image(img)) = tex {
            sprite.image = texture(rt, images, &img);
        }
        commands.spawn((sprite, Transform::from_translation(origin), p));
    } else {
        let mat = mats.add(StandardMaterial { base_color: c0.into(), unlit: true, alpha_mode: AlphaMode::Blend, ..default() });
        commands.spawn((
            Mesh3d(ball.clone()),
            MeshMaterial3d(mat.clone()),
            Transform::from_translation(origin).with_scale(Vec3::splat(size0)),
            Particle { mat: Some(mat), ..p },
            bevy::pbr::NotShadowCaster,
            bevy::pbr::NotShadowReceiver,
        ));
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn particles(
    mut commands: Commands,
    time: Res<Time>,
    mut rt: NonSendMut<Runtime>,
    mut emitters: Query<&mut Emitter>,
    mut parts: Query<(Entity, &mut Particle, &mut Transform, Option<&mut Sprite>)>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut ball: Local<Option<Handle<Mesh>>>,
    mut dot: Local<Option<Handle<Image>>>,
) {
    let rt = &mut *rt;
    if rt.debug || rt.failed {
        return; // frozen while time-traveling
    }
    let dt = time.delta_secs().min(0.05);
    let mut alive = 0;
    for (e, mut p, mut tf, sprite) in &mut parts {
        p.age += dt;
        if p.age >= p.life {
            commands.entity(e).despawn();
            continue;
        }
        alive += 1;
        p.vel.y += p.gravity * dt;
        tf.translation += p.vel * dt;
        let t = p.age / p.life;
        let size = p.size.0 + (p.size.1 - p.size.0) * t;
        let c = mix(p.color.0, p.color.1, t);
        match sprite {
            Some(mut s) => {
                s.color = c.into();
                s.custom_size = Some(Vec2::splat(size));
            }
            None => {
                tf.scale = Vec3::splat(size);
                if let Some(m) = p.mat.as_ref().and_then(|h| mats.get_mut(h)) {
                    m.base_color = c.into();
                }
            }
        }
    }
    let bursts = std::mem::take(&mut rt.it.bursts);
    let ball = ball.get_or_insert_with(|| meshes.add(Sphere::new(0.5).mesh().ico(1).unwrap())).clone();
    let dot = dot.get_or_insert_with(|| images.add(soft_dot())).clone();
    for mut em in &mut emitters {
        let Some(Value::Obj(o)) = source_value(rt, &em.source) else { continue };
        let mut jobs: Vec<(usize, Option<Vec<f64>>)> = vec![];
        if !matches!(o.get("visible"), Some(Value::Bool(false))) {
            em.acc += num(&o, "rate", 20.0).max(0.0) * dt;
            let n = em.acc.floor();
            em.acc -= n;
            jobs.push((n as usize, None));
        }
        if let Source::Global(name) = &em.source {
            for (b, count, at) in &bursts {
                if b == name {
                    jobs.push((*count, at.clone()));
                }
            }
        }
        for (count, at) in jobs {
            for _ in 0..count {
                if alive >= MAX_PARTICLES {
                    break;
                }
                spawn_particle(&mut commands, rt, &mut images, &mut mats, &ball, &dot, &em, &o, &at);
                alive += 1;
            }
        }
    }
}
