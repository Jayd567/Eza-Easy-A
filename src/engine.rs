//! `eza play`: runs a script inside a Bevy window. The script's top level runs once,
//! then every frame the engine feeds in keyboard state, calls `tick` (on handlers, persist,
//! mimic cleanup) and copies position/rotation/scale/visible/color back onto the 3D objects.
use crate::interp::Interp;
use crate::value::{equals, Obj, Value};
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::camera::ClearColorConfig;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::image::ImageSampler;
use bevy::sprite::Anchor;
use crate::value::ImageData;
use std::collections::{HashMap, HashSet};
use std::path::Path;

mod chart;
mod inputs;
mod particles;
mod sound;
use inputs::{gui_inputs, input_sync, scripted_inputs, spawn_input};
use particles::{particles, Emitter};
use sound::{play_sounds, EzaSound};
use chart::{chart_sync, spawn_chart};
use bevy::audio::AudioSource;

const MESHES: &[&str] = &["plane", "sphere", "cube", "cylinder"];
const NOISES: &[&str] = &["simplex", "perlin", "worley", "value_noise", "noise"];

struct GuiState {
    value: Value,
    ent: Option<Entity>,
    /// current and base position of the root node; `cur` glides toward the centered target on resize
    cur: (f32, f32),
    base: (f32, f32),
}

struct Runtime {
    it: Interp,
    failed: bool,
    /// last drawn value of each gui window, the UI entity, and the window size it was drawn for
    gui: HashMap<String, GuiState>,
    /// Eza images turned into GPU textures, by file path (so each picture is uploaded once)
    textures: HashMap<String, Handle<Image>>,
    atlas_layouts: HashMap<(String, u32, u32), Handle<TextureAtlasLayout>>,
    font_cache: HashMap<String, Handle<Font>>,
    /// F1 time-travel debugger: the game is paused while this is on
    debug: bool,
    hold: u32,
    /// bumped by `go to`, so per-script caches start over
    generation: u32,
    /// decoded sound files, by path
    sounds: HashMap<std::path::PathBuf, Handle<AudioSource>>,
    master: f32,
    /// after `go to`: looping sounds the new script doesn't play again are stopped
    orphan_check: bool,
    /// the textbox being typed in / the slider being dragged, as (gui root, child path)
    focus: Option<(String, Vec<usize>)>,
    drag: Option<(String, Vec<usize>)>,
    /// the open dropdown list
    dropdown: Option<Entity>,
    rng: u64,
}

impl Runtime {
    fn new(it: Interp) -> Self {
        Runtime {
            it,
            failed: false,
            gui: HashMap::new(),
            textures: HashMap::new(),
            atlas_layouts: HashMap::new(),
            font_cache: HashMap::new(),
            debug: false,
            hold: 0,
            generation: 0,
            sounds: HashMap::new(),
            master: 1.0,
            orphan_check: false,
            focus: None,
            drag: None,
            dropdown: None,
            rng: 0x2545_F491_4F6C_DD1D,
        }
    }

    /// 0..1, for particles (separate from the script's own random numbers)
    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }
}

#[derive(Component)]
struct GuiButton {
    root: String,
    path: Vec<usize>,
    base: Color,
    /// buttons get the built-in hover/press shading
    shade: bool,
    /// styled elements: normal, hover and press looks
    looks: Option<Box<[Look; 3]>>,
    label: Option<Entity>,
    /// runs the `then` handler when pressed (inputs run theirs when their value changes instead)
    click: bool,
}

enum Source {
    /// entities like `player` live in their own global variable
    Global(String),
    /// meshes are read from `scene.children[i].children[j]...`
    Scene(Vec<usize>),
    /// created at runtime by `spawn`
    Dyn(u64),
    /// 2D sprites declared in a `stage`, read from `stage.children[i]`
    Stage(Vec<usize>),
}

#[derive(Component)]
struct EzaNode {
    source: Source,
    material: Handle<StandardMaterial>,
}

#[derive(Clone, Copy, PartialEq)]
enum NoiseKind {
    Simplex,
    Perlin,
    Worley,
    Value,
}

#[derive(Clone)]
struct NoiseCfg {
    kind: NoiseKind,
    freq: f32,
    amp: f32,
    octaves: u32,
}

pub fn play(path: &str) -> i32 {
    if let Err(e) = std::fs::metadata(path) {
        eprintln!("can't open {}: {}", path, e);
        return 2;
    }
    let it = match Interp::start(Path::new(path), None, true) {
        Ok(it) => it,
        Err(e) => {
            eprintln!("{}", e);
            return 1;
        }
    };
    let title = format!("Eza - {}", it.file.display());
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title, ..default() }),
            ..default()
        }))
        .insert_non_send_resource(Runtime::new(it))
        .insert_resource(ClearColor(Color::srgb(0.08, 0.09, 0.12)))
        .add_systems(Startup, (setup, setup_debug))
        .add_systems(
            Update,
            (scripted_inputs, scene_switch, gui_click, gui_inputs, debug_input, step, dyn_sync, sync, sync2d, gui_sync, chart_sync, input_sync, play_sounds, particles)
                .chain(),
        )
        .add_systems(Update, auto_screenshot)
        .run();
    0
}

// ---------- value helpers ----------

fn num(o: &crate::value::Obj, k: &str, d: f32) -> f32 {
    match o.get(k) {
        Some(Value::Num(n)) => *n as f32,
        _ => d,
    }
}

fn vec3(v: Option<&Value>, d: Vec3) -> Vec3 {
    if let Some(Value::List(l)) = v {
        let n = |i: usize| match l.get(i) {
            Some(Value::Num(x)) => Some(*x as f32),
            _ => None,
        };
        if let (Some(x), Some(y), Some(z)) = (n(0), n(1), n(2)) {
            return Vec3::new(x, y, z);
        }
    }
    d
}

fn transform_of(o: &crate::value::Obj) -> Transform {
    let r = vec3(o.get("rotation"), Vec3::ZERO);
    Transform {
        translation: vec3(o.get("position"), Vec3::ZERO),
        rotation: Quat::from_euler(EulerRot::XYZ, r.x.to_radians(), r.y.to_radians(), r.z.to_radians()),
        scale: vec3(o.get("scale"), Vec3::ONE),
    }
}

fn to_color(c: [f64; 4]) -> Color {
    Color::srgba(c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, c[3] as f32)
}

fn child_at(v: &Value, path: &[usize]) -> Option<Value> {
    let mut cur = v.clone();
    for &i in path {
        let next = match &cur {
            Value::Obj(o) => match o.get("children") {
                Some(Value::List(l)) => l.get(i).cloned(),
                _ => None,
            },
            _ => None,
        };
        cur = next?;
    }
    Some(cur)
}

// ---------- noise ----------

fn hash(i: i32, j: i32, k: i32) -> u32 {
    let mut h = (i as u32).wrapping_mul(374_761_393)
        ^ (j as u32).wrapping_mul(668_265_263)
        ^ (k as u32).wrapping_mul(2_147_483_647);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    h ^ (h >> 16)
}

fn grad(h: u32, x: f32, y: f32, z: f32) -> f32 {
    let h = h & 15;
    let u = if h < 8 { x } else { y };
    let v = if h < 4 { y } else if h == 12 || h == 14 { x } else { z };
    (if h & 1 == 0 { u } else { -u }) + (if h & 2 == 0 { v } else { -v })
}

/// Classic gradient (Perlin) noise, roughly -1..1.
fn perlin(x: f32, y: f32, z: f32) -> f32 {
    let (xi, yi, zi) = (x.floor() as i32, y.floor() as i32, z.floor() as i32);
    let (xf, yf, zf) = (x - x.floor(), y - y.floor(), z - z.floor());
    let fade = |t: f32| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    let (u, v, w) = (fade(xf), fade(yf), fade(zf));
    let g = |dx: i32, dy: i32, dz: i32| {
        grad(hash(xi + dx, yi + dy, zi + dz), xf - dx as f32, yf - dy as f32, zf - dz as f32)
    };
    let l = |a: f32, b: f32, t: f32| a + (b - a) * t;
    l(
        l(l(g(0, 0, 0), g(1, 0, 0), u), l(g(0, 1, 0), g(1, 1, 0), u), v),
        l(l(g(0, 0, 1), g(1, 0, 1), u), l(g(0, 1, 1), g(1, 1, 1), u), v),
        w,
    )
}

/// 3D simplex noise (Gustavson), roughly -1..1.
fn simplex(xin: f32, yin: f32, zin: f32) -> f32 {
    const F3: f32 = 1.0 / 3.0;
    const G3: f32 = 1.0 / 6.0;
    let s = (xin + yin + zin) * F3;
    let (i, j, k) = ((xin + s).floor() as i32, (yin + s).floor() as i32, (zin + s).floor() as i32);
    let t = (i + j + k) as f32 * G3;
    let (x0, y0, z0) = (xin - (i as f32 - t), yin - (j as f32 - t), zin - (k as f32 - t));
    let (i1, j1, k1, i2, j2, k2) = if x0 >= y0 {
        if y0 >= z0 {
            (1, 0, 0, 1, 1, 0)
        } else if x0 >= z0 {
            (1, 0, 0, 1, 0, 1)
        } else {
            (0, 0, 1, 1, 0, 1)
        }
    } else if y0 < z0 {
        (0, 0, 1, 0, 1, 1)
    } else if x0 < z0 {
        (0, 1, 0, 0, 1, 1)
    } else {
        (0, 1, 0, 1, 1, 0)
    };
    let corner = |dx: f32, dy: f32, dz: f32, h: u32| {
        let t = 0.6 - dx * dx - dy * dy - dz * dz;
        if t < 0.0 {
            0.0
        } else {
            let t = t * t;
            t * t * grad(h, dx, dy, dz)
        }
    };
    32.0 * (corner(x0, y0, z0, hash(i, j, k))
        + corner(x0 - i1 as f32 + G3, y0 - j1 as f32 + G3, z0 - k1 as f32 + G3, hash(i + i1, j + j1, k + k1))
        + corner(x0 - i2 as f32 + 2.0 * G3, y0 - j2 as f32 + 2.0 * G3, z0 - k2 as f32 + 2.0 * G3, hash(i + i2, j + j2, k + k2))
        + corner(x0 - 1.0 + 3.0 * G3, y0 - 1.0 + 3.0 * G3, z0 - 1.0 + 3.0 * G3, hash(i + 1, j + 1, k + 1)))
}

/// Cellular (Worley) noise: distance to the nearest random point, mapped so cell centers are peaks.
fn worley(x: f32, y: f32, z: f32) -> f32 {
    let (xi, yi, zi) = (x.floor() as i32, y.floor() as i32, z.floor() as i32);
    let mut best = f32::MAX;
    for dx in -1..=1 {
        for dy in -1..=1 {
            for dz in -1..=1 {
                let (cx, cy, cz) = (xi + dx, yi + dy, zi + dz);
                let r = |s: i32| hash(cx, cy.wrapping_add(s * 7919), cz.wrapping_add(s * 104_729)) as f32 / u32::MAX as f32;
                let p = (cx as f32 + r(1), cy as f32 + r(2), cz as f32 + r(3));
                let d = ((p.0 - x).powi(2) + (p.1 - y).powi(2) + (p.2 - z).powi(2)).sqrt();
                best = best.min(d);
            }
        }
    }
    1.0 - 2.0 * best.min(1.0)
}

/// Smoothly interpolated random values on a grid, -1..1.
fn value_noise(x: f32, y: f32, z: f32) -> f32 {
    let (xi, yi, zi) = (x.floor() as i32, y.floor() as i32, z.floor() as i32);
    let (xf, yf, zf) = (x - x.floor(), y - y.floor(), z - z.floor());
    let fade = |t: f32| t * t * (3.0 - 2.0 * t);
    let (u, v, w) = (fade(xf), fade(yf), fade(zf));
    let g = |dx: i32, dy: i32, dz: i32| hash(xi + dx, yi + dy, zi + dz) as f32 / u32::MAX as f32 * 2.0 - 1.0;
    let l = |a: f32, b: f32, t: f32| a + (b - a) * t;
    l(
        l(l(g(0, 0, 0), g(1, 0, 0), u), l(g(0, 1, 0), g(1, 1, 0), u), v),
        l(l(g(0, 0, 1), g(1, 0, 1), u), l(g(0, 1, 1), g(1, 1, 1), u), v),
        w,
    )
}

impl NoiseCfg {
    fn base(&self, p: Vec3) -> f32 {
        match self.kind {
            NoiseKind::Simplex => simplex(p.x, p.y, p.z),
            NoiseKind::Perlin => perlin(p.x, p.y, p.z),
            NoiseKind::Worley => worley(p.x, p.y, p.z),
            NoiseKind::Value => value_noise(p.x, p.y, p.z),
        }
    }

    /// `octaves` layers finer detail on top (each octave: double the frequency, half the weight).
    fn sample(&self, p: Vec3) -> f32 {
        let (mut total, mut norm, mut f, mut a) = (0.0, 0.0, self.freq, 1.0);
        for o in 0..self.octaves {
            total += a * self.base(p * f + Vec3::splat(17.0 * o as f32));
            norm += a;
            f *= 2.0;
            a *= 0.5;
        }
        total / norm * self.amp
    }
}

// ---------- meshes ----------

fn smooth_normals(pos: &[[f32; 3]], idx: &[u32]) -> Vec<[f32; 3]> {
    let mut n = vec![Vec3::ZERO; pos.len()];
    for t in idx.chunks(3) {
        let (a, b, c) = (Vec3::from(pos[t[0] as usize]), Vec3::from(pos[t[1] as usize]), Vec3::from(pos[t[2] as usize]));
        let f = (b - a).cross(c - a);
        for &i in t {
            n[i as usize] += f;
        }
    }
    n.iter().map(|v| v.normalize_or(Vec3::Y).to_array()).collect()
}

fn build_mesh(pos: Vec<[f32; 3]>, uv: Vec<[f32; 2]>, idx: Vec<u32>) -> Mesh {
    let normals = smooth_normals(&pos, &idx);
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
        .with_inserted_indices(Indices::U32(idx))
}

fn plane_mesh(w: f32, d: f32, seg: u32, nz: &Option<NoiseCfg>) -> Mesh {
    let n = seg.max(1);
    let (mut pos, mut uv, mut idx) = (vec![], vec![], vec![]);
    for iz in 0..=n {
        for ix in 0..=n {
            let (u, v) = (ix as f32 / n as f32, iz as f32 / n as f32);
            let (x, z) = ((u - 0.5) * w, (v - 0.5) * d);
            let y = nz.as_ref().map_or(0.0, |c| c.sample(Vec3::new(x, 0.0, z)));
            pos.push([x, y, z]);
            uv.push([u, v]);
        }
    }
    let row = n + 1;
    for iz in 0..n {
        for ix in 0..n {
            let a = iz * row + ix;
            idx.extend([a, a + row, a + 1, a + 1, a + row, a + row + 1]);
        }
    }
    build_mesh(pos, uv, idx)
}

fn sphere_mesh(r: f32, seg: u32, nz: &Option<NoiseCfg>) -> Mesh {
    let mesh: Mesh = Sphere::new(r).mesh().uv(seg.clamp(8, 256), (seg / 2).clamp(4, 128));
    let Some(c) = nz else { return mesh };
    let Some(VertexAttributeValues::Float32x3(p)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION).cloned() else {
        return mesh;
    };
    let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0).cloned() else {
        return mesh;
    };
    let idx: Vec<u32> = match mesh.indices() {
        Some(Indices::U32(v)) => v.clone(),
        Some(Indices::U16(v)) => v.iter().map(|&i| i as u32).collect(),
        None => return mesh,
    };
    let pos = p
        .iter()
        .map(|q| {
            let v = Vec3::from(*q);
            (v + v.normalize_or_zero() * c.sample(v)).to_array()
        })
        .collect();
    build_mesh(pos, uv, idx)
}

// ---------- systems ----------

/// Creates the 3D object (mesh + material) for an Eza object and returns its entity.
fn spawn_visual(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<StandardMaterial>,
    o: &Obj,
    noise: &Option<NoiseCfg>,
    source: Source,
) -> Entity {
    let kind = o.type_name.as_str();
        let (w, h) = (num(o, "width", 1.0), num(o, "height", 1.0));
        let seg = num(o, "seg", 32.0) as u32;
        let is_mesh = MESHES.contains(&kind);
        let mesh: Mesh = match kind {
            "plane" => plane_mesh(w, h, seg, noise),
            "sphere" => sphere_mesh(w / 2.0, seg, noise),
            "cube" => Cuboid::new(w, h, num(o, "depth", w)).into(),
            "cylinder" => Cylinder::new(w / 2.0, h).into(),
            _ => Capsule3d::new(0.4, 1.0).into(),
        };
        let fallback = match kind {
            "plane" => Color::srgb(0.35, 0.55, 0.35),
            "player" => Color::srgb(0.25, 0.5, 0.95),
            "enemy" => Color::srgb(0.9, 0.3, 0.25),
            _ if is_mesh => Color::srgb(0.65, 0.65, 0.72),
            _ => Color::srgb(0.95, 0.7, 0.2),
        };
        let base_color = match o.get("color") {
            Some(Value::Color(c)) => to_color(**c),
            _ => fallback,
        };
        let material = mats.add(StandardMaterial { base_color, perceptual_roughness: 0.85, ..default() });
        commands
        .spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(material.clone()), transform_of(o), EzaNode { source, material }))
        .id()
}

/// Keeps the 3D view in step with objects made (or destroyed) by `spawn` / `destroy` while the game runs.
fn dyn_sync(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut atlases: ResMut<Assets<TextureAtlasLayout>>,
    mut rt: NonSendMut<Runtime>,
    mut reg: Local<HashMap<u64, Entity>>,
    mut seen_generation: Local<u32>,
) {
    let rt = &mut *rt;
    if *seen_generation != rt.generation {
        // a new script: the old spawned objects were already cleared
        *seen_generation = rt.generation;
        reg.clear();
    }
    for (id, alive) in rt.it.entity_states() {
        match (alive, reg.contains_key(&id)) {
            (true, false) => {
                if let Some(Value::Obj(o)) = rt.it.entity_value(id) {
                    let e = if o.type_name == "sprite" {
                        spawn_sprite(&mut commands, rt, &mut images, &mut atlases, &o, Source::Dyn(id), 0.5)
                    } else {
                        spawn_visual(&mut commands, &mut meshes, &mut mats, &o, &None, Source::Dyn(id))
                    };
                    reg.insert(id, e);
                }
            }
            (false, true) => {
                if let Some(e) = reg.remove(&id) {
                    commands.entity(e).despawn_recursive();
                }
            }
            _ => {}
        }
    }
    rt.it.prune_dead();
}

#[allow(clippy::too_many_arguments)]
fn spawn_node(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<StandardMaterial>,
    v: &Value,
    path: Vec<usize>,
    noise: Option<NoiseCfg>,
    cam: &mut Option<Vec3>,
    have_light: &mut bool,
) {
    let Value::Obj(o) = v else { return };
    let kind = o.type_name.as_str();
    let mut noise = noise;
    if NOISES.contains(&kind) {
        let nk = match kind {
            "perlin" => NoiseKind::Perlin,
            "worley" => NoiseKind::Worley,
            "value_noise" => NoiseKind::Value,
            _ => NoiseKind::Simplex,
        };
        noise = Some(NoiseCfg {
            kind: nk,
            freq: num(o, "freq", 1.0),
            amp: num(o, "amp", 1.0),
            octaves: num(o, "octaves", 1.0).clamp(1.0, 8.0) as u32,
        });
    } else if kind == "camera" {
        *cam = Some(vec3(o.get("position"), Vec3::new(0.0, 12.0, 18.0)));
    } else if kind == "particles" {
        let source = match o.get("name") {
            Some(Value::Str(n)) => Source::Global(n.clone()),
            _ => Source::Scene(path.clone()),
        };
        commands.spawn((Transform::default(), Emitter { source, acc: 0.0, two_d: false, z: 0.0 }));
    } else if kind == "light" {
        *have_light = true;
        let p = vec3(o.get("position"), Vec3::new(5.0, 12.0, 8.0));
        commands.spawn((
            DirectionalLight { illuminance: num(o, "brightness", 10_000.0), shadows_enabled: true, ..default() },
            Transform::from_translation(p).looking_at(Vec3::ZERO, Vec3::Y),
        ));
    } else {
        let source = match o.get("name") {
            Some(Value::Str(n)) => Source::Global(n.clone()),
            _ if MESHES.contains(&kind) => Source::Scene(path.clone()),
            _ => Source::Global(kind.to_string()),
        };
        spawn_visual(commands, meshes, mats, o, &noise, source);
    }
    if let Some(Value::List(ch)) = o.get("children") {
        for (i, c) in ch.iter().enumerate() {
            let mut p = path.clone();
            p.push(i);
            spawn_node(commands, meshes, mats, c, p, noise.clone(), cam, have_light);
        }
    }
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut atlases: ResMut<Assets<TextureAtlasLayout>>,
    mut rt: NonSendMut<Runtime>,
) {
    build_world(&mut commands, &mut meshes, &mut mats, &mut images, &mut atlases, &mut rt);
}

/// `go to "level2"`: clears the old script's world and builds the new one in the same window.
/// Only `global` survives; looping music the new script plays again keeps playing without a restart.
#[allow(clippy::too_many_arguments)]
fn scene_switch(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut atlases: ResMut<Assets<TextureAtlasLayout>>,
    mut rt: NonSendMut<Runtime>,
    things: Query<Entity, (Or<(With<Transform>, With<Node>)>, Without<Parent>, Without<DebugPanel>, Without<EzaSound>)>,
    mut sounds: Query<&mut EzaSound>,
    mut windows: Query<&mut Window>,
    mut exit: EventWriter<AppExit>,
) {
    let Some(next) = rt.it.go_to.take() else { return };
    for e in &things {
        commands.entity(e).despawn_recursive();
    }
    let prev = std::mem::replace(&mut rt.it, Interp::new(&next));
    match Interp::start(&next, Some(prev), true) {
        Ok(it) => rt.it = it,
        Err(e) => {
            eprintln!("{}", e);
            rt.failed = true;
            exit.send(AppExit::error());
            return;
        }
    }
    rt.gui.clear();
    rt.generation += 1;
    (rt.focus, rt.drag, rt.dropdown, rt.debug) = (None, None, None, false);
    for mut s in &mut sounds {
        s.orphan = s.looping;
    }
    rt.orphan_check = true;
    if let Ok(mut w) = windows.get_single_mut() {
        w.title = format!("Eza - {}", rt.it.file.display());
    }
    build_world(&mut commands, &mut meshes, &mut mats, &mut images, &mut atlases, &mut rt);
}

fn build_world(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    atlases: &mut Assets<TextureAtlasLayout>,
    rt: &mut Runtime,
) {
    let (mut commands, mut meshes, mut mats, mut images, mut atlases) = (commands, meshes, mats, images, atlases);
    let has_scene = rt.it.global("scene").is_some();
    let stage = rt.it.global("stage");
    let (mut cam, mut have_light) = (None, false);
    if let Some(Value::Obj(scene)) = rt.it.global("scene") {
        if let Some(Value::List(ch)) = scene.get("children") {
            for (i, c) in ch.iter().enumerate() {
                spawn_node(&mut commands, &mut meshes, &mut mats, c, vec![i], None, &mut cam, &mut have_light);
            }
        }
    } else {
        if stage.is_none() && rt.it.gui_roots.is_empty() {
            println!("[Notice] This script has no `scene` or `stage` block, so the window will be empty.");
        }
    }
    if has_scene || stage.is_none() {
        let cam = cam.unwrap_or(Vec3::new(0.0, 12.0, 18.0));
        commands.spawn((Camera3d::default(), Transform::from_translation(cam).looking_at(Vec3::ZERO, Vec3::Y)));
        if !have_light {
            commands.spawn((
                DirectionalLight { illuminance: 10_000.0, shadows_enabled: true, ..default() },
                Transform::from_xyz(5.0, 12.0, 8.0).looking_at(Vec3::ZERO, Vec3::Y),
            ));
        }
        commands.insert_resource(AmbientLight { color: Color::WHITE, brightness: 400.0 });
    }
    if let Some(Value::Obj(st)) = stage {
        // the 2D camera draws on top of a 3D scene when there is one
        let clear = if has_scene { ClearColorConfig::None } else { ClearColorConfig::Default };
        commands.spawn((Camera2d, Camera { order: 1, clear_color: clear, ..default() }));
        if let Some(Value::List(kids)) = st.get("children") {
            for (i, c) in kids.iter().enumerate() {
                let Value::Obj(o) = c else { continue };
                let z = num(o, "layer", 0.0) + i as f32 * 0.001;
                let source = match o.get("name") {
                    Some(Value::Str(n)) => Source::Global(n.clone()),
                    _ => Source::Stage(vec![i]),
                };
                match o.type_name.as_str() {
                    "tilemap" => spawn_tilemap(&mut commands, rt, &mut images, &mut atlases, o, z),
                    "particles" => {
                        commands.spawn((Transform::default(), Emitter { source, acc: 0.0, two_d: true, z }));
                    }
                    _ => {
                        // named sprites live in their own variable (that's what scripts and physics change)
                        spawn_sprite(&mut commands, rt, &mut images, &mut atlases, o, source, z);
                    }
                }
            }
        }
    }
}

/// Bevy key names -> Eza names: "z", "1", "space", "shift", "ctrl", "alt", "up", "enter", ...
fn key_name(k: &KeyCode) -> String {
    let s = format!("{:?}", k);
    if s.starts_with("Shift") {
        return "shift".into();
    }
    if s.starts_with("Control") {
        return "ctrl".into();
    }
    if s.starts_with("Alt") {
        return "alt".into();
    }
    let s = s.strip_prefix("Key").or_else(|| s.strip_prefix("Digit")).or_else(|| s.strip_prefix("Arrow")).unwrap_or(&s);
    s.to_lowercase()
}

fn step(
    mut rt: NonSendMut<Runtime>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    mut exit: EventWriter<AppExit>,
) {
    if rt.failed || rt.debug || rt.it.go_to.is_some() {
        return;
    }
    let names = |it: &mut dyn Iterator<Item = &KeyCode>| it.map(key_name).collect::<HashSet<_>>();
    if rt.focus.is_some() {
        // typing in a textbox: the game doesn't see those keys
        rt.it.keys_pressed.clear();
        rt.it.keys_held.clear();
        rt.it.keys_released.clear();
    } else {
        rt.it.keys_pressed = names(&mut keys.get_just_pressed());
        rt.it.keys_held = names(&mut keys.get_pressed());
        rt.it.keys_released = names(&mut keys.get_just_released());
    }
    let mname = |b: &MouseButton| match b {
        MouseButton::Left => "left".to_string(),
        MouseButton::Right => "right".to_string(),
        MouseButton::Middle => "middle".to_string(),
        other => format!("{:?}", other).to_lowercase(),
    };
    for b in mouse.get_just_pressed() {
        rt.it.keys_pressed.insert(format!("mouse:{}", mname(b)));
    }
    for b in mouse.get_pressed() {
        rt.it.keys_held.insert(format!("mouse:{}", mname(b)));
    }
    for b in mouse.get_just_released() {
        rt.it.keys_released.insert(format!("mouse:{}", mname(b)));
    }
    if let Ok(w) = windows.get_single() {
        if let Some(p) = w.cursor_position() {
            rt.it.set_gui_field("mouse", &[], "position", Value::list(vec![Value::Num(p.x as f64), Value::Num(p.y as f64)]));
        }
        rt.it.set_gui_field("screen", &[], "width", Value::Num(w.width() as f64));
        rt.it.set_gui_field("screen", &[], "height", Value::Num(w.height() as f64));
    }
    if let Err(e) = rt.it.tick() {
        if e.is_switch() {
            return; // `go to`: scene_switch takes over next frame
        }
        eprintln!("{}", e);
        rt.failed = true;
        exit.send(AppExit::error());
    }
}

fn sync(
    rt: NonSend<Runtime>,
    mut q: Query<(&EzaNode, &mut Transform, &mut Visibility)>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let scene = rt.it.global("scene");
    for (node, mut tf, mut vis) in &mut q {
        let v = match &node.source {
            Source::Global(n) => rt.it.global(n),
            Source::Scene(p) => scene.as_ref().and_then(|s| child_at(s, p)),
            Source::Dyn(id) => rt.it.entity_value(*id),
            Source::Stage(_) => None,
        };
        let Some(Value::Obj(o)) = v else { continue };
        let t = transform_of(&o);
        if *tf != t {
            *tf = t;
        }
        let want = if matches!(o.get("visible"), Some(Value::Bool(false))) { Visibility::Hidden } else { Visibility::Inherited };
        if *vis != want {
            *vis = want;
        }
        if let Some(Value::Color(c)) = o.get("color") {
            let col = to_color(**c);
            if mats.get(&node.material).is_some_and(|m| m.base_color != col) {
                mats.get_mut(&node.material).unwrap().base_color = col;
            }
        }
    }
}

// ---------- GUI ----------

fn rect(o: &Obj) -> (f32, f32, f32, f32) {
    (num(o, "x", 0.0), num(o, "y", 0.0), num(o, "width", 0.0), num(o, "height", 0.0))
}

fn shade(c: Color, d: f32) -> Color {
    let s = c.to_srgba();
    Color::srgba((s.red + d).clamp(0.0, 1.0), (s.green + d).clamp(0.0, 1.0), (s.blue + d).clamp(0.0, 1.0), s.alpha)
}

/// What an element looks like in one state (normal, hovered, pressed).
#[derive(Clone, Default)]
struct Look {
    bg: Option<Color>,
    text: Option<Color>,
    glow: Option<BoxShadow>,
    border: Option<Color>,
    scale: Option<f32>,
}

impl Look {
    fn over(&self, top: &Look) -> Look {
        Look {
            bg: top.bg.or(self.bg),
            text: top.text.or(self.text),
            glow: top.glow.clone().or_else(|| self.glow.clone()),
            border: top.border.or(self.border),
            scale: top.scale.or(self.scale),
        }
    }
}

fn color_of(v: Option<&Value>) -> Option<Color> {
    match v {
        Some(Value::Color(c)) => Some(to_color(**c)),
        _ => None,
    }
}

fn parts(v: &Value) -> Vec<Value> {
    match v {
        Value::List(l) => (**l).clone(),
        other => vec![other.clone()],
    }
}

fn no_shadow() -> BoxShadow {
    BoxShadow { color: Color::NONE, x_offset: Val::Px(0.0), y_offset: Val::Px(0.0), spread_radius: Val::Px(0.0), blur_radius: Val::Px(0.0) }
}

/// glow = size, color, strength   or   shadow = x, y, blur, color
fn shadow_of(o: &Obj) -> Option<BoxShadow> {
    let split = |v: &Value| {
        let (mut nums, mut color, mut none) = (vec![], None, false);
        for p in parts(v) {
            match p {
                Value::Num(n) => nums.push(n as f32),
                Value::Color(c) => color = Some(to_color(*c)),
                Value::Str(s) if s == "none" => none = true,
                _ => {}
            }
        }
        (nums, color, none)
    };
    if let Some(g) = o.get("glow") {
        let (n, color, none) = split(g);
        if none {
            return Some(no_shadow());
        }
        let blur = n.first().copied().unwrap_or(10.0);
        let c = color.unwrap_or(Color::WHITE).to_srgba();
        let strength = n.get(1).copied().unwrap_or(0.5);
        return Some(BoxShadow {
            color: Color::srgba(c.red, c.green, c.blue, c.alpha * strength),
            x_offset: Val::Px(0.0),
            y_offset: Val::Px(0.0),
            spread_radius: Val::Px(blur * 0.2),
            blur_radius: Val::Px(blur),
        });
    }
    if let Some(sh) = o.get("shadow") {
        let (n, color, none) = split(sh);
        if none {
            return Some(no_shadow());
        }
        return Some(BoxShadow {
            color: color.unwrap_or(Color::srgba(0.0, 0.0, 0.0, 0.5)),
            x_offset: Val::Px(n.first().copied().unwrap_or(0.0)),
            y_offset: Val::Px(n.get(1).copied().unwrap_or(4.0)),
            spread_radius: Val::Px(0.0),
            blur_radius: Val::Px(n.get(2).copied().unwrap_or(8.0)),
        });
    }
    None
}

/// border = 1, solid, #00FFCC  (width, style, color) - or `border = none`
fn border_of(o: &Obj) -> Option<(f32, Option<Color>)> {
    let v = o.get("border")?;
    let (mut width, mut color) = (1.0, None);
    for p in parts(v) {
        match p {
            Value::Num(n) => width = n as f32,
            Value::Color(c) => color = Some(to_color(*c)),
            Value::Str(s) if s == "none" => return None,
            _ => {}
        }
    }
    Some((width, color))
}

/// The look an element has with nothing happening. `background` sets the background and `color`
/// the text; without `background`, `color` on a box or button is its background (the older style).
fn base_look(o: &Obj, kind: &str) -> Look {
    let has_bg = o.get("background").is_some();
    Look {
        // (a chart's `color` is its line or bar color)
        bg: color_of(o.get("background")).or_else(|| if kind != "text" && kind != "chart" { color_of(o.get("color")) } else { None }),
        text: color_of(o.get("text_color")).or_else(|| if kind == "text" || has_bg { color_of(o.get("color")) } else { None }),
        glow: shadow_of(o),
        border: border_of(o).and_then(|b| b.1),
        scale: None,
    }
}

/// `on hover` / `on press` lines of a style: here `color` always means the text color.
fn state_look(v: Option<&Value>) -> Look {
    let Some(Value::Obj(o)) = v else { return Look::default() };
    Look {
        bg: color_of(o.get("background")),
        text: color_of(o.get("color")).or_else(|| color_of(o.get("text_color"))),
        glow: shadow_of(o),
        border: border_of(o).and_then(|b| b.1),
        scale: match o.get("scale") {
            Some(Value::Num(s)) => Some(*s as f32),
            _ => None,
        },
    }
}

/// Fonts: the built-in font, or a .ttf/.otf file next to the script (`font = "fonts/neon.ttf"`).
struct UiCtx<'a> {
    fonts: &'a mut Assets<Font>,
    images: &'a mut Assets<Image>,
    cache: &'a mut HashMap<String, Handle<Font>>,
    resolve: &'a dyn Fn(&str) -> std::path::PathBuf,
}

fn font_handle(ctx: &mut UiCtx, name: &str) -> Handle<Font> {
    if matches!(name, "" | "default" | "sans-serif" | "sans_serif" | "serif" | "monospace") {
        return Handle::default();
    }
    if let Some(h) = ctx.cache.get(name) {
        return h.clone();
    }
    let h = match std::fs::read((ctx.resolve)(name)).ok().and_then(|b| Font::try_from_bytes(b).ok()) {
        Some(f) => ctx.fonts.add(f),
        None => {
            eprintln!("[Notice] couldn't load the font \"{}\" - using the built-in font", name);
            Handle::default()
        }
    };
    ctx.cache.insert(name.to_string(), h.clone());
    h
}

/// Spawns one GUI element positioned relative to its parent's top-left corner (px, py).
#[allow(clippy::too_many_arguments)]
fn spawn_el(commands: &mut Commands, ctx: &mut UiCtx, v: &Value, px: f32, py: f32, offset: (f32, f32), root: &str, path: Vec<usize>) -> Option<Entity> {
    let Value::Obj(o) = v else { return None };
    if matches!(o.get("visible"), Some(Value::Bool(false))) {
        return None;
    }
    let (x, y, w, h) = rect(o);
    let kind = o.type_name.as_str();
    let label = o.get("label").map(|l| l.display()).unwrap_or_default();
    let centered_text = matches!(o.get("position"), Some(Value::Str(s)) if s == "center");

    // normal, hovered and pressed looks, each complete so leaving a state restores everything
    let base = base_look(o, kind);
    let hover = base.over(&state_look(o.get("hover_look")));
    let press = hover.over(&state_look(o.get("press_look")));
    let has_states = o.get("hover_look").is_some() || o.get("press_look").is_some();
    let mut looks = [base, hover, press];
    let is_input = crate::layout::INPUT_KINDS.contains(&kind);
    let default_bg = match kind {
        "button" | "dropdown" => Color::srgb(0.23, 0.23, 0.26),
        "textbox" => Color::srgb(0.09, 0.09, 0.11),
        "chart" => Color::srgb(0.11, 0.12, 0.15),
        "window" => Color::srgba(0.1, 0.1, 0.12, 0.92),
        _ => Color::NONE,
    };
    let border = border_of(o);
    let any_glow = looks.iter().any(|l| l.glow.is_some());
    let any_border = border.is_some() || looks.iter().any(|l| l.border.is_some());
    let bg0 = looks[0].bg.unwrap_or(default_bg);
    let text0 = looks[0].text.unwrap_or(Color::WHITE);
    let border0 = looks[0].border.unwrap_or(text0);
    for l in looks.iter_mut() {
        l.bg.get_or_insert(bg0);
        l.text.get_or_insert(text0);
        l.border.get_or_insert(border0);
        l.scale.get_or_insert(1.0);
        if any_glow && l.glow.is_none() {
            l.glow = Some(no_shadow());
        }
    }

    let mut node = Node {
        position_type: PositionType::Absolute,
        left: Val::Px(x - px + offset.0),
        top: Val::Px(y - py + offset.1),
        width: Val::Px(w),
        height: Val::Px(h),
        ..default()
    };
    // textboxes get a thin frame unless the style says otherwise
    let any_border = any_border || kind == "textbox";
    let border0 = if kind == "textbox" && border.is_none() { Color::srgb(0.35, 0.35, 0.4) } else { border0 };
    if any_border {
        node.border = UiRect::all(Val::Px(border.map_or(1.0, |b| b.0)));
    }
    let node = match kind {
        "text" => Node {
            justify_content: if centered_text { JustifyContent::Center } else { JustifyContent::FlexStart },
            align_items: AlignItems::Center,
            ..node
        },
        "button" => Node { justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..node },
        "textbox" => Node { align_items: AlignItems::Center, padding: UiRect::horizontal(Val::Px(8.0)), overflow: Overflow::clip(), ..node },
        "dropdown" => Node {
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            padding: UiRect::horizontal(Val::Px(10.0)),
            ..node
        },
        "slider" | "checkbox" => node,
        _ => Node { overflow: Overflow::clip(), ..node },
    };
    let font_name = o.get("font").map(|f| f.display()).unwrap_or_default();
    let font = TextFont {
        font: font_handle(ctx, &font_name),
        // font_size is in points: 12pt = 16 pixels
        font_size: o.get("font_size").and_then(|v| v.as_num(0).ok()).map_or(16.0, |pt| pt as f32 * 4.0 / 3.0),
        ..default()
    };

    let mut ec = commands.spawn((node, BackgroundColor(bg0)));
    if kind == "button" {
        ec.insert(Button);
    }
    let rounded = o.get("rounded").and_then(|v| v.as_num(0).ok()).map(|r| r as f32);
    if let Some(r) = rounded.or(if kind == "textbox" || kind == "dropdown" { Some(4.0) } else { None }) {
        ec.insert(BorderRadius::all(Val::Px(r)));
    }
    if any_border {
        ec.insert(BorderColor(border0));
    }
    if let Some(g) = &looks[0].glow {
        ec.insert(g.clone());
    }
    let e = ec.id();
    let label_entity = if kind == "text" || kind == "button" {
        let t = commands.spawn((Text::new(label), TextColor(text0), font.clone(), TextLayout::new_with_no_wrap())).id();
        commands.entity(e).add_child(t);
        Some(t)
    } else {
        None
    };
    if is_input {
        let accent = color_of(o.get("accent")).unwrap_or(Color::srgb(0.3, 0.6, 1.0));
        let input = spawn_input(commands, e, o, kind, root, &path, w, text0, accent, font);
        commands.entity(e).insert((Interaction::default(), input));
    } else if kind == "chart" {
        let chart = spawn_chart(commands, ctx.images, e, o, w, h, text0, font);
        commands.entity(e).insert(chart);
    }
    if kind == "button" || has_states || matches!(o.get("hoverable"), Some(Value::Bool(true))) {
        commands.entity(e).insert((
            Interaction::default(),
            GuiButton {
                root: root.to_string(),
                path: path.clone(),
                base: bg0,
                shade: kind == "button" && !has_states,
                looks: if has_states { Some(Box::new(looks)) } else { None },
                label: label_entity,
                click: !is_input,
            },
        ));
    }
    if let Some(Value::List(ch)) = o.get("children") {
        for (i, c) in ch.iter().enumerate() {
            let mut p = path.clone();
            p.push(i);
            if let Some(child) = spawn_el(commands, ctx, c, x, y, (0.0, 0.0), root, p) {
                commands.entity(e).add_child(child);
            }
        }
    }
    Some(e)
}

fn gui_offset(v: &Value, size: (f32, f32)) -> (f32, f32) {
    match v {
        // centered windows are laid out for 1920x1080; shift them to the real window's center
        Value::Obj(o) if matches!(o.get("centered"), Some(Value::Bool(true))) => {
            let (x, y, w, h) = rect(o);
            ((size.0 - w) / 2.0 - x, (size.1 - h) / 2.0 - y)
        }
        _ => (0.0, 0.0),
    }
}

/// Redraws a gui window whenever its value changes, so `change pause_menu.visible to false`,
/// label updates and even `rewind` show up on screen. When only the window size changes,
/// centered windows glide to their new position over a few frames instead of snapping.
fn gui_sync(
    mut commands: Commands,
    mut rt: NonSendMut<Runtime>,
    windows: Query<&Window>,
    mut nodes: Query<&mut Node>,
    mut fonts: ResMut<Assets<Font>>,
    mut images: ResMut<Assets<Image>>,
) {
    let Ok(win) = windows.get_single() else { return };
    let size = (win.width(), win.height());
    let rt = &mut *rt;
    for name in rt.it.gui_roots.clone() {
        let Some(v) = rt.it.global(&name) else { continue };
        let target = gui_offset(&v, size);
        let same = rt.gui.get(&name).is_some_and(|s| same_ignoring_hover(&s.value, &v));
        if same {
            let s = rt.gui.get_mut(&name).unwrap();
            if s.cur != target {
                let d = (target.0 - s.cur.0, target.1 - s.cur.1);
                s.cur = if d.0.abs() + d.1.abs() < 0.5 { target } else { (s.cur.0 + d.0 * 0.25, s.cur.1 + d.1 * 0.25) };
                if let Some(e) = s.ent {
                    if let Ok(mut n) = nodes.get_mut(e) {
                        n.left = Val::Px(s.base.0 + s.cur.0);
                        n.top = Val::Px(s.base.1 + s.cur.1);
                    }
                }
            }
            continue;
        }
        let cur = match rt.gui.get(&name) {
            Some(s) => {
                if let Some(e) = s.ent {
                    commands.entity(e).despawn_recursive();
                }
                s.cur
            }
            None => target,
        };
        let base = match &v {
            Value::Obj(o) => {
                let r = rect(o);
                (r.0, r.1)
            }
            _ => (0.0, 0.0),
        };
        let ent = {
            let it = &rt.it;
            let resolve = |p: &str| it.resolve_path(p);
            let mut ctx = UiCtx { fonts: &mut fonts, images: &mut images, cache: &mut rt.font_cache, resolve: &resolve };
            spawn_el(&mut commands, &mut ctx, &v, 0.0, 0.0, cur, &name, vec![])
        };
        rt.gui.insert(name, GuiState { value: v, ent, cur, base });
    }
}

/// Hover flags change every time the mouse moves; they shouldn't force a full redraw.
fn same_ignoring_hover(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Obj(x), Value::Obj(y)) => {
            let keep = |o: &Obj| o.fields.iter().filter(|(k, _)| k != "hover").count();
            x.type_name == y.type_name
                && keep(x) == keep(y)
                && x.fields
                    .iter()
                    .filter(|(k, _)| k != "hover")
                    .all(|(k, v)| y.get(k).is_some_and(|w| same_ignoring_hover(v, w)))
        }
        (Value::List(x), Value::List(y)) => {
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(p, q)| same_ignoring_hover(p, q))
        }
        _ => equals(a, b),
    }
}

fn gui_click(
    mut rt: NonSendMut<Runtime>,
    mut q: Query<
        (Ref<Interaction>, &GuiButton, &mut BackgroundColor, Option<&mut BoxShadow>, Option<&mut BorderColor>, &mut Transform),
        Changed<Interaction>,
    >,
    mut texts: Query<&mut TextColor>,
) {
    for (i, b, mut bg, shadow, border, mut tf) in &mut q {
        if *i == Interaction::None && i.is_added() {
            continue; // freshly redrawn element; the real hover state arrives next frame
        }
        let hovering = *i != Interaction::None;
        rt.it.set_gui_field(&b.root, &b.path, "hover", Value::Bool(hovering));
        if *i == Interaction::Pressed && b.click {
            let f = rt.it.global(&b.root).and_then(|r| child_at(&r, &b.path)).and_then(|v| match v {
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
        let state = match *i {
            Interaction::None => 0,
            Interaction::Hovered => 1,
            Interaction::Pressed => 2,
        };
        if let Some(looks) = &b.looks {
            let l = &looks[state];
            if let Some(c) = l.bg {
                bg.0 = c;
            }
            if let (Some(mut s), Some(g)) = (shadow, &l.glow) {
                *s = g.clone();
            }
            if let (Some(mut bc), Some(c)) = (border, l.border) {
                bc.0 = c;
            }
            tf.scale = Vec3::splat(l.scale.unwrap_or(1.0));
            if let (Some(lbl), Some(c)) = (b.label, l.text) {
                if let Ok(mut t) = texts.get_mut(lbl) {
                    t.0 = c;
                }
            }
        } else if b.shade {
            bg.0 = match state {
                0 => b.base,
                1 => shade(b.base, 0.12),
                _ => shade(b.base, -0.08),
            };
        }
    }
}

// ---------- 2D ----------

#[derive(Component)]
struct EzaSprite {
    source: Source,
    z: f32,
}

/// Uploads an Eza image as a texture (once per file) with crisp pixel-art sampling.
fn texture(rt: &mut Runtime, images: &mut Assets<Image>, img: &ImageData) -> Handle<Image> {
    if let Some(h) = rt.textures.get(&img.path) {
        return h.clone();
    }
    let mut im = Image::new(
        Extent3d { width: img.width, height: img.height, depth_or_array_layers: 1 },
        TextureDimension::D2,
        img.rgba.clone(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    im.sampler = ImageSampler::nearest();
    let h = images.add(im);
    rt.textures.insert(img.path.clone(), h.clone());
    h
}

fn atlas_layout(rt: &mut Runtime, atlases: &mut Assets<TextureAtlasLayout>, img: &ImageData, fw: u32, fh: u32) -> Handle<TextureAtlasLayout> {
    let key = (img.path.clone(), fw, fh);
    if let Some(h) = rt.atlas_layouts.get(&key) {
        return h.clone();
    }
    let (cols, rows) = ((img.width / fw.max(1)).max(1), (img.height / fh.max(1)).max(1));
    let h = atlases.add(TextureAtlasLayout::from_grid(UVec2::new(fw.max(1), fh.max(1)), cols, rows, None, None));
    rt.atlas_layouts.insert(key, h.clone());
    h
}

fn transform2d(o: &Obj, z: f32) -> Transform {
    let p = crate::two_d::vec2(o.get("position")).unwrap_or([0.0, 0.0]);
    let s = crate::two_d::scale2(o);
    Transform {
        translation: Vec3::new(p[0] as f32, p[1] as f32, z),
        rotation: Quat::from_rotation_z(num(o, "rotation", 0.0).to_radians()),
        scale: Vec3::new(s[0] as f32, s[1] as f32, 1.0),
    }
}

/// Builds or refreshes a Bevy sprite from an Eza sprite object (texture, frame, flip, origin, size, tint).
fn apply_sprite(s: &mut Sprite, rt: &mut Runtime, images: &mut Assets<Image>, atlases: &mut Assets<TextureAtlasLayout>, o: &Obj) {
    let size = Vec2::new(num(o, "width", 32.0), num(o, "height", 32.0));
    let tint = match o.get("color") {
        Some(Value::Color(c)) => Some(to_color(**c)),
        _ => None,
    };
    match o.get("texture") {
        Some(Value::Image(img)) => {
            let h = texture(rt, images, img);
            if s.image != h {
                s.image = h;
            }
            s.color = tint.unwrap_or(Color::WHITE);
            if let Some([fw, fh]) = crate::two_d::vec2(o.get("frame_size")) {
                let layout = atlas_layout(rt, atlases, img, fw as u32, fh as u32);
                let count = ((img.width / (fw as u32).max(1)) * (img.height / (fh as u32).max(1))).max(1) as usize;
                let index = (num(o, "frame", 0.0).max(0.0) as usize) % count;
                match &mut s.texture_atlas {
                    Some(a) if a.layout == layout => a.index = index,
                    _ => s.texture_atlas = Some(TextureAtlas { layout, index }),
                }
            } else {
                s.texture_atlas = None;
            }
        }
        // no texture: a plain colored rectangle
        _ => s.color = tint.unwrap_or(Color::srgb(0.95, 0.7, 0.2)),
    }
    s.custom_size = Some(size);
    s.flip_x = matches!(o.get("flip_x"), Some(Value::Bool(true)));
    s.flip_y = matches!(o.get("flip_y"), Some(Value::Bool(true)));
    let f = crate::two_d::origin_frac(o);
    s.anchor = Anchor::Custom(Vec2::new(f[0] as f32, f[1] as f32));
}

fn spawn_sprite(
    commands: &mut Commands,
    rt: &mut Runtime,
    images: &mut Assets<Image>,
    atlases: &mut Assets<TextureAtlasLayout>,
    o: &Obj,
    source: Source,
    z: f32,
) -> Entity {
    let mut s = Sprite::default();
    apply_sprite(&mut s, rt, images, atlases, o);
    let vis = if matches!(o.get("visible"), Some(Value::Bool(false))) { Visibility::Hidden } else { Visibility::Inherited };
    commands.spawn((s, transform2d(o, z), vis, EzaSprite { source, z })).id()
}

/// One sprite per tile; Bevy batches sprites that share a texture into few draw calls.
fn spawn_tilemap(commands: &mut Commands, rt: &mut Runtime, images: &mut Assets<Image>, atlases: &mut Assets<TextureAtlasLayout>, o: &Obj, z: f32) {
    let (Some([px, py]), Some(Value::List(grid))) = (crate::two_d::vec2(o.get("position")), o.get("grid")) else { return };
    let ts = num(o, "tile_size", 32.0);
    let tex = match o.get("texture") {
        Some(Value::Image(img)) => Some((texture(rt, images, img), atlas_layout(rt, atlases, img, ts as u32, ts as u32))),
        _ => None,
    };
    for (r, row) in grid.iter().enumerate() {
        let Value::List(row) = row else { continue };
        for (c, cell) in row.iter().enumerate() {
            let Value::Num(i) = cell else { continue };
            if *i < 0.0 {
                continue;
            }
            let pos = Vec3::new(px as f32 + (c as f32 + 0.5) * ts, py as f32 - (r as f32 + 0.5) * ts, z);
            let sprite = match &tex {
                Some((image, layout)) => Sprite {
                    image: image.clone(),
                    texture_atlas: Some(TextureAtlas { layout: layout.clone(), index: *i as usize }),
                    custom_size: Some(Vec2::splat(ts)),
                    ..default()
                },
                None => Sprite::from_color(Color::srgb(0.4, 0.4, 0.45), Vec2::splat(ts)),
            };
            commands.spawn((sprite, Transform::from_translation(pos)));
        }
    }
}

/// Copies sprite properties and the 2D camera from Eza onto the screen every frame.
fn sync2d(
    mut rt: NonSendMut<Runtime>,
    mut images: ResMut<Assets<Image>>,
    mut atlases: ResMut<Assets<TextureAtlasLayout>>,
    mut q: Query<(&EzaSprite, &mut Transform, &mut Visibility, &mut Sprite)>,
    mut cams: Query<(&mut Transform, &mut OrthographicProjection), (With<Camera2d>, Without<EzaSprite>)>,
) {
    let rt = &mut *rt;
    let stage = rt.it.global("stage");
    for (node, mut tf, mut vis, mut sprite) in &mut q {
        let v = match &node.source {
            Source::Global(n) => rt.it.global(n),
            Source::Stage(p) => stage.as_ref().and_then(|s| child_at(s, p)),
            Source::Dyn(id) => rt.it.entity_value(*id),
            Source::Scene(_) => None,
        };
        let Some(Value::Obj(o)) = v else { continue };
        let t = transform2d(&o, node.z);
        if *tf != t {
            *tf = t;
        }
        let want = if matches!(o.get("visible"), Some(Value::Bool(false))) { Visibility::Hidden } else { Visibility::Inherited };
        if *vis != want {
            *vis = want;
        }
        apply_sprite(&mut sprite, rt, &mut images, &mut atlases, &o);
    }
    if let Some(Value::Obj(st)) = &stage {
        if let Some(Value::Obj(cam)) = st.get("camera") {
            let p = crate::two_d::vec2(cam.get("position")).unwrap_or([0.0, 0.0]);
            let zoom = num(cam, "zoom", 1.0).max(0.001);
            for (mut tf, mut proj) in &mut cams {
                tf.translation.x = p[0] as f32;
                tf.translation.y = p[1] as f32;
                tf.rotation = Quat::from_rotation_z(num(cam, "rotation", 0.0).to_radians());
                if (proj.scale - 1.0 / zoom).abs() > 1e-6 {
                    proj.scale = 1.0 / zoom;
                }
            }
        }
    }
}

/// Developer helper: `EZA_SCREENSHOT=shot.png eza game.eza` saves what the window shows
/// after a moment, then closes. Handy for checking visuals and for bug reports.
fn auto_screenshot(mut commands: Commands, mut frames: Local<u32>, mut exit: EventWriter<AppExit>, mut rt: NonSendMut<Runtime>) {
    use bevy::render::view::screenshot::{save_to_disk, Screenshot};
    let Ok(path) = std::env::var("EZA_SCREENSHOT") else { return };
    *frames += 1;
    // EZA_SCREENSHOT_TIMETRAVEL=1 also opens the time-travel debugger, 30 frames back
    if *frames == 70 && std::env::var("EZA_SCREENSHOT_TIMETRAVEL").is_ok() {
        rt.debug = true;
        rt.it.scrub(-30);
    }
    // EZA_SCREENSHOT_FRAME=60 takes it earlier or later
    let at = std::env::var("EZA_SCREENSHOT_FRAME").ok().and_then(|f| f.parse().ok()).unwrap_or(90u32);
    if *frames == at {
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
    }
    if *frames == at + 60 {
        exit.send(AppExit::Success);
    }
}

// ---------- time-travel debugger (F1) ----------

#[derive(Component)]
struct DebugText;

#[derive(Component)]
struct DebugPanel;

fn setup_debug(mut commands: Commands) {
    commands
        .spawn((
            Node { position_type: PositionType::Absolute, left: Val::Px(10.0), bottom: Val::Px(10.0), padding: UiRect::all(Val::Px(12.0)), ..default() },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.8)),
            BorderRadius::all(Val::Px(6.0)),
            GlobalZIndex(1000),
            Visibility::Hidden,
            DebugPanel,
        ))
        .with_children(|p| {
            p.spawn((Text::new(""), TextFont { font_size: 14.0, ..default() }, TextColor(Color::srgb(0.75, 1.0, 0.8)), DebugText));
        });
}

/// F1 pauses the game and lets you scrub through its history: Left/Right step one frame
/// (hold to keep going), Shift steps 10, Home jumps to the start, End back to the newest frame.
/// Pressing F1 again resumes from the frame you're looking at.
fn debug_input(
    mut rt: NonSendMut<Runtime>,
    keys: Res<ButtonInput<KeyCode>>,
    mut texts: Query<&mut Text, With<DebugText>>,
    mut panels: Query<&mut Visibility, With<DebugPanel>>,
) {
    if keys.just_pressed(KeyCode::F1) {
        rt.debug = !rt.debug;
        rt.hold = 0;
    }
    if rt.debug {
        let unit = if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) { 10 } else { 1 };
        let (left, right) = (keys.pressed(KeyCode::ArrowLeft), keys.pressed(KeyCode::ArrowRight));
        rt.hold = if left || right { rt.hold + 1 } else { 0 };
        let mut delta = 0i64;
        if keys.just_pressed(KeyCode::ArrowLeft) || (left && rt.hold > 15) {
            delta = -unit;
        }
        if keys.just_pressed(KeyCode::ArrowRight) || (right && rt.hold > 15) {
            delta = unit;
        }
        if keys.just_pressed(KeyCode::Home) {
            delta = i64::MIN / 2;
        }
        if keys.just_pressed(KeyCode::End) {
            delta = i64::MAX / 2;
        }
        if delta != 0 {
            rt.it.scrub(delta);
        }
    }
    let (Ok(mut text), Ok(mut vis)) = (texts.get_single_mut(), panels.get_single_mut()) else { return };
    let want = if rt.debug { Visibility::Inherited } else { Visibility::Hidden };
    if *vis != want {
        *vis = want;
    }
    if rt.debug {
        let (cur, max) = rt.it.debug_cursor();
        let mut s = format!("TIME TRAVEL   frame {} / {}\nF1 resume   <- -> step (hold)   Shift x10   Home / End\n", cur, max);
        for line in rt.it.debug_watch(22) {
            s.push('\n');
            s.push_str(&line);
        }
        if text.0 != s {
            text.0 = s;
        }
    }
}
