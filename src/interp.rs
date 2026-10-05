use crate::ast::*;
use crate::error::{EzaError, R};
use crate::value::*;
use crate::{layout, lexer, methods, parser};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;

#[path = "interp_explain.rs"]
mod explain;

/// How many steps of history each variable (and the scene timeline) keeps.
pub const HISTORY: usize = 1000;
const MESHES: &[&str] = &["plane", "sphere", "cube", "cylinder", "cone", "torus", "mesh", "light", "camera"];
const SPRITE_KINDS: &[&str] = &["sprite", "tilemap"];
const NOISES: &[&str] = &["simplex", "perlin", "worley", "value_noise", "noise"];
const MAX_DEPTH: usize = 2000;

/// Where a value came from: the file, line and frame of the code that set it.
#[derive(Clone, Copy, Default, PartialEq)]
pub struct Origin {
    pub line: u32,
    /// an index into Interp::files
    pub file: u16,
    pub frame: u32,
}

/// A variable and its past values, each with the place in the code that set it. The history is
/// `first` followed by `later`, so a variable that never changes (most loop variables and
/// function arguments) needs no extra memory.
pub struct Var {
    first: Value,
    first_at: Origin,
    later: VecDeque<(Value, Origin)>,
    pub idx: usize,
    /// created inside a mimic block, so the purity guard lets it change
    pub sandbox: bool,
    /// where this variable's value was copied from: a loop's list item (with its position), the
    /// object a method was called on, ... (followed when explaining an error)
    pub source: Option<(Rc<Source>, Option<usize>)>,
}
pub type VarRef = Rc<RefCell<Var>>;

/// A place values get copied from, like the list an `each` loop walks through.
pub struct Source {
    pub var: VarRef,
    pub path: Vec<PathEl>,
    /// how the code wrote it, e.g. "enemies"
    pub name: String,
}

impl Var {
    fn new(v: Value, sandbox: bool, at: Origin) -> Var {
        Var { first: v, first_at: at, later: VecDeque::new(), idx: 0, sandbox, source: None }
    }
    fn len(&self) -> usize {
        self.later.len() + 1
    }
    pub fn get(&self) -> &Value {
        if self.idx == 0 {
            &self.first
        } else {
            &self.later[self.idx - 1].0
        }
    }
    /// The current value, to change without making a new history step.
    pub fn get_mut(&mut self) -> &mut Value {
        if self.idx == 0 {
            &mut self.first
        } else {
            &mut self.later[self.idx - 1].0
        }
    }
    /// History step `i` (0 = when it was created), up to the step being shown now.
    pub fn step(&self, i: usize) -> (&Value, Origin) {
        if i == 0 {
            (&self.first, self.first_at)
        } else {
            let (v, o) = &self.later[i - 1];
            (v, *o)
        }
    }
    /// A new step: erases any rewound "future" and starts a new one from here.
    pub fn set(&mut self, v: Value) {
        self.set_at(v, Origin::default());
    }
    pub fn set_at(&mut self, v: Value, at: Origin) {
        self.later.truncate(self.idx);
        self.later.push_back((v, at));
        if self.len() > HISTORY {
            if let Some((next, next_at)) = self.later.pop_front() {
                self.first = next;
                self.first_at = next_at;
            }
        }
        self.idx = self.len() - 1;
    }
    /// Slides the pointer forward again (the time-travel debugger's "step forward").
    #[allow(dead_code)] // used by the engine build
    pub fn forward(&mut self, n: usize) {
        self.idx = (self.idx + n).min(self.len() - 1);
    }
    /// Takes the current value out (for changing a big value in place) - put it back with `put`.
    fn take(&mut self) -> Value {
        std::mem::replace(self.get_mut(), Value::None)
    }
    fn put(&mut self, v: Value) {
        *self.get_mut() = v;
    }
    /// Slides the pointer back. Returns true if it had to clamp at the creation value.
    pub fn rewind(&mut self, n: usize) -> bool {
        if n > self.idx {
            self.idx = 0;
            true
        } else {
            self.idx -= n;
            false
        }
    }
}

/// A quick hash for variable names (the standard one is built to resist attacks, which costs time
/// on every variable lookup and doesn't matter here).
#[derive(Default, Clone, Copy)]
pub struct NameHasher(u64);

impl std::hash::Hasher for NameHasher {
    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut word = [0u8; 8];
            word[..chunk.len()].copy_from_slice(chunk);
            self.0 = (self.0.rotate_left(5) ^ u64::from_le_bytes(word)).wrapping_mul(0x517c_c1b7_2722_0a95);
        }
    }
    fn write_u8(&mut self, b: u8) {
        self.0 = (self.0.rotate_left(5) ^ b as u64).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
    fn finish(&self) -> u64 {
        self.0
    }
}

type NameMap = HashMap<Rc<str>, VarRef, std::hash::BuildHasherDefault<NameHasher>>;

/// Up to this many names live right inside the scope (no extra memory to find, and faster than
/// hashing); bigger scopes, like a script's top level, switch to a hash map.
const FEW: usize = 4;

enum Vars {
    Few { names: [Option<Rc<str>>; FEW], vars: [Option<VarRef>; FEW], len: usize },
    Many(NameMap),
}

impl Vars {
    fn get(&self, name: &str) -> Option<&VarRef> {
        match self {
            Vars::Few { names, vars, len } => {
                (0..*len).find(|&i| names[i].as_deref() == Some(name)).and_then(|i| vars[i].as_ref())
            }
            Vars::Many(m) => m.get(name),
        }
    }
}

pub struct Scope {
    vars: RefCell<Vars>,
    parent: Option<Rc<Scope>>,
}

impl Scope {
    pub fn new(parent: Option<Rc<Scope>>) -> Rc<Scope> {
        Rc::new(Scope { vars: RefCell::new(Vars::Few { names: Default::default(), vars: Default::default(), len: 0 }), parent })
    }
    pub fn lookup(&self, name: &str) -> Option<VarRef> {
        let mut s = self;
        loop {
            if let Some(v) = s.vars.borrow().get(name) {
                return Some(v.clone());
            }
            s = s.parent.as_deref()?;
        }
    }
    /// The value of a variable (a copy), without handing out the variable itself.
    pub fn value_of(&self, name: &str) -> Option<Value> {
        let mut s = self;
        loop {
            if let Some(v) = s.vars.borrow().get(name) {
                return Some(v.borrow().get().clone());
            }
            s = s.parent.as_deref()?;
        }
    }
    pub fn collect_names(&self, out: &mut Vec<String>) {
        out.extend(self.entries().into_iter().map(|(k, _)| k));
        if let Some(p) = &self.parent {
            p.collect_names(out);
        }
    }
    pub fn entries(&self) -> Vec<(String, VarRef)> {
        match &*self.vars.borrow() {
            Vars::Few { names, vars, len } => (0..*len)
                .filter_map(|i| Some((names[i].as_deref()?.to_string(), vars[i].clone()?)))
                .collect(),
            Vars::Many(m) => m.iter().map(|(k, v)| (k.to_string(), v.clone())).collect(),
        }
    }
    pub fn get_local(&self, name: &str) -> Option<VarRef> {
        self.vars.borrow().get(name).cloned()
    }
    pub fn has_local(&self, name: &str) -> bool {
        self.vars.borrow().get(name).is_some()
    }
    pub fn insert(&self, name: &str, v: VarRef) {
        // replacing an existing name needs no new copy of the name
        let mut vars = self.vars.borrow_mut();
        if let Vars::Few { names, vars: slots, len } = &mut *vars {
            if let Some(i) = (0..*len).find(|&i| names[i].as_deref() == Some(name)) {
                slots[i] = Some(v);
                return;
            }
        }
        drop(vars);
        self.insert_rc(Rc::from(name), v);
    }
    /// Like `insert`, with a name that's already shared (function parameters, loop variables).
    pub fn insert_rc(&self, name: Rc<str>, v: VarRef) {
        let mut vars = self.vars.borrow_mut();
        match &mut *vars {
            Vars::Few { names, vars: slots, len } => {
                if let Some(i) = (0..*len).find(|&i| names[i].as_deref() == Some(&*name)) {
                    slots[i] = Some(v);
                } else if *len < FEW {
                    names[*len] = Some(name);
                    slots[*len] = Some(v);
                    *len += 1;
                } else {
                    let mut m = NameMap::default();
                    for i in 0..FEW {
                        if let (Some(k), Some(r)) = (names[i].take(), slots[i].take()) {
                            m.insert(k, r);
                        }
                    }
                    m.insert(name, v);
                    *vars = Vars::Many(m);
                }
            }
            Vars::Many(m) => {
                m.insert(name, v);
            }
        }
    }
}

/// What `spawn` returns: a live reference to a hidden variable that holds the object.
pub struct EntityHandle {
    pub id: u64,
    pub var: VarRef,
}

pub struct DynEntity {
    pub var: VarRef,
    pub alive: bool,
    /// lists it was put in with `spawn ... into list` (destroy takes it out again)
    groups: Vec<(VarRef, Vec<PathEl>)>,
}

/// Follows an entity handle to the object it points at.
pub fn deref_val(v: Value) -> Value {
    if let Value::Entity(h) = &v {
        return h.var.borrow().get().clone();
    }
    v
}

fn deref_var(var: VarRef) -> VarRef {
    let inner = if let Value::Entity(h) = var.borrow().get() { Some(h.var.clone()) } else { None };
    inner.unwrap_or(var)
}

/// Lists/dictionaries bigger than this are changed in place instead of keeping a copy per step
/// in the rewind history (copying 30,000 items on every change would be far too slow).
const BIG: usize = 256;

fn weight(v: &Value, budget: usize) -> usize {
    match v {
        Value::List(l) => {
            let mut n = l.len();
            for x in l.iter() {
                if n > budget {
                    break;
                }
                n += weight(x, budget - n.min(budget));
            }
            n
        }
        Value::Obj(o) => {
            let mut n = o.fields.len();
            for (_, x) in &o.fields {
                if n > budget {
                    break;
                }
                n += weight(x, budget - n.min(budget));
            }
            n
        }
        _ => 0,
    }
}

fn is_big(v: &Value) -> bool {
    weight(v, BIG) > BIG
}

/// Whether running this block can create a name in the block's own scope.
fn declares(stmts: &[Stmt]) -> bool {
    stmts.iter().any(|s| {
        matches!(
            s.kind,
            StmtKind::Assign(..) | StmtKind::Define(_) | StmtKind::Data(..) | StmtKind::Style(..) | StmtKind::Prefab(..) | StmtKind::Use { .. }
        )
    })
}

/// Whether `path` leads to an existing value (like get_path, but without building an error message).
fn path_exists(v: &Value, path: &[PathEl]) -> bool {
    let Some((first, rest)) = path.split_first() else { return true };
    let child = match (v, first) {
        (Value::Obj(o), PathEl::Field(f)) => o.get(f),
        (Value::Obj(o), PathEl::Index(k)) => match k {
            Value::Str(s) => o.get(s),
            other => o.get(&key_str(other)),
        },
        (Value::List(l), el) => {
            let i = match el {
                PathEl::Field(f) => axis(f).map(|a| a as f64),
                PathEl::Index(Value::Num(i)) => Some(*i),
                _ => None,
            };
            match i {
                Some(i) if i.fract() == 0.0 && i < l.len() as f64 && i >= -(l.len() as f64) => {
                    l.get(if i < 0.0 { (l.len() as f64 + i) as usize } else { i as usize })
                }
                _ => None,
            }
        }
        _ => None,
    };
    child.map_or(false, |c| path_exists(c, rest))
}

fn as_int(x: f64) -> Option<i64> {
    if x.fract() == 0.0 && x.abs() < 9.0e15 {
        Some(x as i64)
    } else {
        None
    }
}

/// `x % y` (never negative for a positive y). Whole numbers use integer math, which is far
/// quicker than the general version for decimals.
fn rem(x: f64, y: f64) -> f64 {
    const EXACT: f64 = 9.0e15;
    if x.fract() == 0.0 && y.fract() == 0.0 && x.abs() < EXACT && y.abs() < EXACT {
        (x as i64).rem_euclid(y as i64) as f64
    } else {
        x.rem_euclid(y)
    }
}

fn is_vec(l: &[Value]) -> bool {
    !l.is_empty() && l.iter().all(|v| matches!(v, Value::Num(_)))
}

fn num_of(v: &Value) -> f64 {
    if let Value::Num(n) = v { *n } else { 0.0 }
}

/// [0,0] for 2D objects, [0,0,0] for 3D ones.
fn zero_velocity(o: &Obj) -> Value {
    let dims = match o.get("position") {
        Some(Value::List(l)) if l.len() == 2 => 2,
        _ => 3,
    };
    Value::list(vec![Value::Num(0.0); dims])
}

fn physics_defaults(o: &mut Obj) {
    if matches!(o.get("physics"), Some(Value::Bool(true))) {
        if o.get("velocity").is_none() {
            o.set("velocity", zero_velocity(&o));
        }
        if o.get("grounded").is_none() {
            o.set("grounded", Value::Bool(false));
        }
    }
}

/// A function or `on` body that contains `wait`, paused between frames.
enum Frame {
    Block { stmts: Rc<Vec<Stmt>>, idx: usize, scope: Rc<Scope> },
    Each { var: String, items: Vec<Value>, i: usize, body: Rc<Vec<Stmt>>, scope: Rc<Scope> },
    While { cond: Expr, body: Rc<Vec<Stmt>>, scope: Rc<Scope> },
}

struct Task {
    frames: Vec<Frame>,
    wake: u64,
    file: u16,
}

/// What `play` and `stop` ask the engine to do.
#[allow(dead_code)] // read by the engine build
pub enum SoundCmd {
    Play { path: PathBuf, looping: bool, volume: f32, speed: f32 },
    /// None = stop every sound
    Stop(Option<PathBuf>),
}

pub enum Flow {
    Normal,
    Return(Value),
    Break,
    Continue,
}

/// A precomputed path ("ghost tracer"): one value per frame.
struct Tween {
    var: VarRef,
    path: Vec<PathEl>,
    frames: Vec<Value>,
    idx: usize,
}

fn overlaps(a: &[PathEl], b: &[PathEl]) -> bool {
    let n = a.len().min(b.len());
    path_eq(&a[..n], &b[..n])
}

fn ease(t: f64, kind: &str) -> f64 {
    match kind {
        "linear" => t,
        "ease_in" => t * t,
        "ease_out" => 1.0 - (1.0 - t) * (1.0 - t),
        _ => {
            if t < 0.5 {
                2.0 * t * t
            } else {
                1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
            }
        }
    }
}

pub fn lerp(a: &Value, b: &Value, t: f64, line: usize) -> R<Value> {
    Ok(match (a, b) {
        (Value::Num(x), Value::Num(y)) => Value::Num(x + (y - x) * t),
        (Value::Color(x), Value::Color(y)) => {
            let mut c = [0.0; 4];
            for i in 0..4 {
                c[i] = x[i] + (y[i] - x[i]) * t;
            }
            Value::Color(Rc::new(c))
        }
        (Value::List(x), Value::List(y)) if x.len() == y.len() => {
            let mut v = vec![];
            for (p, q) in x.iter().zip(y.iter()) {
                v.push(lerp(p, q, t, line)?);
            }
            Value::list(v)
        }
        _ => {
            return Err(EzaError::runtime(
                line,
                format!("can't tween from {} to {} (use numbers, colors or same-size lists)", a.type_name(), b.type_name()),
            ))
        }
    })
}

enum Undo {
    Restore(Value),
    Delta(f64),
}
struct Capture {
    var: VarRef,
    path: Vec<PathEl>,
    undo: Undo,
}
struct PersistLayer {
    caps: Vec<Capture>,
    remaining: Option<i64>,
    until: Option<Expr>,
    /// persist with no ending: undone when this `on` condition stops being true
    while_cond: Option<Expr>,
    scope: Rc<Scope>,
}

struct Handler {
    cond: Expr,
    body: Rc<Vec<Stmt>>,
    scope: Rc<Scope>,
    was_true: bool,
    has_wait: bool,
    /// where the `on` line is
    line: usize,
    file: u16,
    /// it hit an error while a game was running, so it's switched off
    broken: bool,
}

/// `on a touches b [as x, y]` / `on a stops touching b`: fires once per pair of objects.
struct TouchHandler {
    a: Expr,
    b: Expr,
    names: Option<(String, String)>,
    start: bool,
    body: Rc<Vec<Stmt>>,
    scope: Rc<Scope>,
    has_wait: bool,
    line: usize,
    file: u16,
    broken: bool,
    /// the pairs that were touching last frame
    pairs: HashSet<(String, String)>,
}

/// `on event "boss_dead" [as info]`
struct EventHandler {
    name: String,
    var: Option<String>,
    body: Rc<Vec<Stmt>>,
    scope: Rc<Scope>,
    has_wait: bool,
}

struct MimicJob {
    shadow: String,
    obj: Obj,
    body: Rc<Vec<Stmt>>,
    scope: Rc<Scope>,
    rng: u64,
}

/// What came after the script's name on the command line (`eza tool.eza a b` -> ["a", "b"]).
static SCRIPT_ARGS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();

pub fn set_script_args(args: Vec<String>) {
    let _ = SCRIPT_ARGS.set(args);
}

pub struct Interp {
    /// names the language provides (keyboard, mouse, args, ...), seen by the main file and every module
    base: Rc<Scope>,
    pub globals: Rc<Scope>,
    /// the top level of the file running now: the main script's globals, or a module's own names
    top: Rc<Scope>,
    /// modules loaded with `use`, by file (each runs only once)
    modules: HashMap<PathBuf, Rc<Module>>,
    /// modules whose top level is running right now (to catch two files using each other)
    loading: Vec<PathBuf>,
    /// scene-wide timeline: which variable changed at each step, and on which frame
    log: VecDeque<(VarRef, u64)>,
    gptr: usize,
    rng: u64,
    guard: Option<VarRef>,
    in_mimic: bool,
    mimic_steps: usize,
    each_depth: usize,
    capture: Option<Vec<Capture>>,
    persists: Vec<PersistLayer>,
    tweens: Vec<Tween>,
    handlers: Vec<Handler>,
    touch_handlers: Vec<TouchHandler>,
    event_handlers: Vec<EventHandler>,
    /// GUI labels with {values} in them, kept up to date every frame: (window variable, element path, the label, its scope)
    live_labels: Vec<(String, Vec<usize>, Expr, Rc<Scope>)>,
    /// sprite animations: the variable's address -> (the animation it is playing, the frame it started on)
    anims: HashMap<usize, (String, u64)>,
    current_on: Option<(Expr, Rc<Scope>)>,
    /// shadows that finished and are readable until the next frame
    shadows: Vec<String>,
    mimic_queue: Vec<MimicJob>,
    mimic_used: usize,
    pub frame: u64,
    /// engine mode: `rewind scene by N steps` means N frames
    pub frame_mode: bool,
    pub keys_pressed: HashSet<String>,
    pub keys_held: HashSet<String>,
    pub keys_released: HashSet<String>,
    /// variables bound to scene/stage objects (for raycast)
    pub scene_names: Vec<String>,
    /// names of variables holding `gui` windows, for the engine to draw
    pub gui_roots: Vec<String>,
    /// scene entities declared with `physics=true`
    pub bodies: Vec<String>,
    /// `eza test`: run `test` blocks and count the results
    pub test_mode: bool,
    pub tests_passed: usize,
    pub tests_failed: usize,
    include_depth: usize,
    tasks: Vec<Task>,
    /// errors from `on` blocks while a game runs (the engine shows them; the game keeps going)
    pub errors: Vec<EzaError>,
    /// `play` / `stop` requests waiting for the engine (only collected when there is a window)
    pub sound_cmds: Vec<SoundCmd>,
    /// `emit 30 from sparks [at p]`: (emitter name, count, position) for the engine
    pub bursts: Vec<(String, usize, Option<Vec<f64>>)>,
    /// set by `go to "file"`: the script to switch to
    pub go_to: Option<PathBuf>,
    /// objects made by `spawn`, keyed by id (the engine draws the live ones)
    pub entities: std::collections::BTreeMap<u64, DynEntity>,
    next_entity: u64,
    /// `load` keeps decoded images here, so the same picture is only read once
    image_cache: HashMap<PathBuf, Rc<ImageData>>,
    /// `database("x.db")` records, by file (written back after every change)
    pub dbs: HashMap<PathBuf, Vec<Value>>,
    included: HashSet<PathBuf>,
    /// the script this interpreter was started with
    #[allow(dead_code)]
    pub file: PathBuf,
    /// every file that has run (the main script, includes, modules); Origin::file indexes it
    pub files: Vec<String>,
    /// the file whose code is running right now
    cur_file: u16,
    dir: PathBuf,
    pub line: usize,
    depth: usize,
}

impl Interp {
    pub fn new(main_file: &Path) -> Self {
        let base = Scope::new(None);
        let globals = Scope::new(Some(base.clone()));
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E3779B97F4A7C15);
        let mut it = Interp {
            base: base.clone(),
            globals: globals.clone(),
            top: globals.clone(),
            modules: HashMap::new(),
            loading: vec![],
            log: VecDeque::new(),
            gptr: 0,
            rng: seed | 1,
            guard: None,
            in_mimic: false,
            mimic_steps: 0,
            each_depth: 0,
            capture: None,
            persists: vec![],
            tweens: vec![],
            handlers: vec![],
            touch_handlers: vec![],
            event_handlers: vec![],
            anims: HashMap::new(),
            live_labels: vec![],
            current_on: None,
            shadows: vec![],
            mimic_queue: vec![],
            mimic_used: 0,
            frame: 0,
            frame_mode: false,
            keys_pressed: HashSet::new(),
            keys_held: HashSet::new(),
            keys_released: HashSet::new(),
            gui_roots: vec![],
            scene_names: vec![],
            bodies: vec![],
            test_mode: false,
            tests_passed: 0,
            tests_failed: 0,
            include_depth: 0,
            tasks: vec![],
            sound_cmds: vec![],
            errors: vec![],
            bursts: vec![],
            go_to: None,
            entities: std::collections::BTreeMap::new(),
            next_entity: 1,
            image_cache: HashMap::new(),
            dbs: HashMap::new(),
            included: HashSet::new(),
            file: main_file.to_path_buf(),
            files: vec![main_file.display().to_string()],
            cur_file: 0,
            dir: main_file.parent().map(|p| p.to_path_buf()).unwrap_or_default(),
            line: 0,
            depth: 0,
        };
        if let Ok(c) = main_file.canonicalize() {
            it.included.insert(c);
        }
        base.insert("keyboard", it.new_var(Value::obj(Obj::new("Keyboard"))));
        let mut mouse = Obj::new("Mouse");
        mouse.set("position", Value::list(vec![Value::Num(0.0), Value::Num(0.0)]));
        base.insert("mouse", it.new_var(Value::obj(mouse)));
        let mut screen = Obj::new("Screen");
        screen.set("width", Value::Num(1280.0));
        screen.set("height", Value::Num(720.0));
        base.insert("screen", it.new_var(Value::obj(screen)));
        // a dictionary, so `global.get("score", 0)` and `global.has("name")` work after `go to`
        base.insert("global", it.new_var(Value::obj(Obj::new("dict"))));
        let mut sound = Obj::new("Sound");
        sound.set("volume", Value::Num(1.0));
        base.insert("sound", it.new_var(Value::obj(sound)));
        base.insert("pi", it.new_var(Value::Num(std::f64::consts::PI)));
        let args = SCRIPT_ARGS.get().map(|a| a.iter().map(|s| Value::Str(s.clone())).collect()).unwrap_or_default();
        base.insert("args", it.new_var(Value::list(args)));
        it
    }

    fn rt<T>(&self, msg: impl Into<String>) -> R<T> {
        Err(EzaError::runtime(self.line, msg).in_file(self.file_name(self.cur_file)))
    }

    /// The place in the code running right now, stored with every change.
    fn origin(&self) -> Origin {
        Origin { line: self.line as u32, file: self.cur_file, frame: self.frame as u32 }
    }

    pub fn file_name(&self, id: u16) -> &str {
        self.files.get(id as usize).map(|s| s.as_str()).unwrap_or("")
    }

    /// The number for a file name in `files` (added if it's new).
    fn file_id(&mut self, name: &str) -> u16 {
        match self.files.iter().position(|f| f == name) {
            Some(i) => i as u16,
            None => {
                self.files.push(name.to_string());
                (self.files.len() - 1) as u16
            }
        }
    }

    pub fn random(&mut self) -> f64 {
        // xorshift64*: deterministic, so mimic can snapshot the seed
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        (x.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64
    }

    fn new_var(&self, v: Value) -> VarRef {
        Rc::new(RefCell::new(Var::new(v, self.in_mimic, self.origin())))
    }

    pub fn make_func(&self, name: &str, body: Rc<Vec<Stmt>>, scope: &Rc<Scope>) -> Value {
        let def = Rc::new(FuncDef::new(name.to_string(), vec![], body));
        Value::Func(Rc::new(Func { def, closure: scope.clone(), file: self.cur_file }))
    }

    pub fn run_source(&mut self, src: &str, file: &str) -> R<()> {
        let g = self.globals.clone();
        self.run_in(src, file, &g)
    }

    fn run_in(&mut self, src: &str, file: &str, scope: &Rc<Scope>) -> R<()> {
        let toks = lexer::lex(src).map_err(|e| e.in_file(file))?;
        let prog = parser::Parser::new(toks).program().map_err(|e| e.in_file(file))?;
        let id = self.file_id(file);
        let outer = std::mem::replace(&mut self.cur_file, id);
        let r = self.exec_block(&prog, scope).map_err(|e| e.in_file(file));
        self.cur_file = outer;
        r?;
        Ok(())
    }

    /// `use "enemies.eza"`: runs the file once in its own scope and returns it as a module.
    fn use_module(&mut self, rel: &str) -> R<Value> {
        let mut full = self.resolve_path(rel);
        if full.extension().is_none() {
            full.set_extension("eza");
        }
        let key = full.canonicalize().unwrap_or_else(|_| full.clone());
        if let Some(m) = self.modules.get(&key) {
            return Ok(Value::Module(m.clone()));
        }
        if self.loading.contains(&key) {
            return self.rt(format!(
                "\"{}\" is already being loaded - two modules can't use each other. Move what they share into a third file that both use.",
                rel
            ));
        }
        let src = match std::fs::read_to_string(&full) {
            Ok(s) => s,
            Err(e) => return self.rt(format!("can't use \"{}\": {}", rel, e)),
        };
        let name = full.file_stem().and_then(|s| s.to_str()).unwrap_or("module").to_string();
        let scope = Scope::new(Some(self.base.clone()));
        // the module's own paths, includes and top level, then back to this file's
        let new_dir = full.parent().map(|d| d.to_path_buf()).unwrap_or_default();
        let old_dir = std::mem::replace(&mut self.dir, new_dir);
        let old_top = std::mem::replace(&mut self.top, scope.clone());
        self.loading.push(key.clone());
        self.include_depth += 1;
        let r = self.run_in(&src, &full.display().to_string(), &scope);
        self.include_depth -= 1;
        self.loading.pop();
        self.top = old_top;
        self.dir = old_dir;
        r?;
        let m = Rc::new(Module { name, scope });
        self.modules.insert(key, m.clone());
        Ok(Value::Module(m))
    }

    /// `enemies.spawn` on a module: one of the names its file created.
    fn module_get(&self, m: &Module, name: &str) -> R<Value> {
        if name.starts_with('_') {
            return self.rt(format!("'{}' is private to the module {} (names starting with _ stay inside their file)", name, m.name));
        }
        match m.scope.get_local(name) {
            Some(v) => Ok(v.borrow().get().clone()),
            None => {
                let names = m.exports();
                let hint = crate::suggest::closest(name, &names).map(|c| format!(" - did you mean '{}'?", c)).unwrap_or_default();
                self.rt(format!("the module {} has no '{}'{}", m.name, name, hint))
            }
        }
    }

    pub fn commit(&mut self, var: &VarRef, v: Value) {
        var.borrow_mut().set_at(v, self.origin());
        if self.in_mimic {
            return;
        }
        self.log.truncate(self.gptr);
        self.log.push_back((var.clone(), self.frame));
        if self.log.len() > HISTORY {
            self.log.pop_front();
        }
        self.gptr = self.log.len();
    }

    #[allow(dead_code)] // used by the engine build
    pub fn global(&self, name: &str) -> Option<Value> {
        self.globals.lookup(name).map(|v| v.borrow().get().clone())
    }

    fn bind_global(&mut self, name: &str, v: Value) {
        match self.globals.lookup(name) {
            Some(var) => self.commit(&var, v),
            None => {
                let var = self.new_var(v);
                self.globals.insert(name, var);
            }
        }
    }

    pub fn exec_block(&mut self, stmts: &[Stmt], scope: &Rc<Scope>) -> R<Flow> {
        for s in stmts {
            match self.exec(s, scope)? {
                Flow::Normal => {}
                f => return Ok(f),
            }
        }
        Ok(Flow::Normal)
    }

    fn exec_child(&mut self, stmts: &[Stmt], scope: &Rc<Scope>) -> R<Flow> {
        // a block that creates no names of its own can share its parent's scope (no new one to make)
        if !declares(stmts) {
            return self.exec_block(stmts, scope);
        }
        let sc = Scope::new(Some(scope.clone()));
        self.exec_block(stmts, &sc)
    }

    fn exec(&mut self, s: &Stmt, scope: &Rc<Scope>) -> R<Flow> {
        self.line = s.line;
        match &s.kind {
            StmtKind::Assign(name, e) => {
                let v = self.eval(e, scope)?;
                if scope.has_local(name) {
                    return self.rt(format!("'{0}' already exists - use 'change {0} to ...' to update it", name));
                }
                scope.insert(name, self.new_var(v));
            }
            StmtKind::Change(t, mode, e) => self.change(t, *mode, e, scope)?,
            StmtKind::Unpack { names, value, create } => {
                let v = deref_val(self.eval(value, scope)?);
                let items = self.unpack(&v, names)?;
                if *create {
                    if let Some(n) = names.iter().find(|n| scope.has_local(n)) {
                        return self.rt(format!("'{0}' already exists - use 'change {0} to ...' to update it", n));
                    }
                    for (n, item) in names.iter().zip(items) {
                        scope.insert(n, self.new_var(item));
                    }
                } else {
                    // the same as  change a to ...  for each name (history, rewind and mimic rules included)
                    let tmp = Scope::new(Some(scope.clone()));
                    tmp.insert("_unpacked", self.new_var(Value::list(items)));
                    for (i, n) in names.iter().enumerate() {
                        let item = Expr::Index(Box::new(Expr::Ident("_unpacked".into())), Box::new(Expr::Num(i as f64)));
                        self.change(&Expr::Ident(n.clone()), ChangeMode::To, &item, &tmp)?;
                    }
                }
            }
            StmtKind::Expr(e) => {
                self.eval(e, scope)?;
            }
            StmtKind::If(arms, els) => {
                for (c, b) in arms {
                    if self.eval(c, scope)?.truthy() {
                        return self.exec_child(b, scope);
                    }
                }
                if let Some(b) = els {
                    return self.exec_child(b, scope);
                }
            }
            StmtKind::Each(name, e, body) if name.contains(',') => {
                let names: Vec<String> = each_names(name).into_iter().map(String::from).collect();
                let src = self.eval(e, scope)?;
                let items = self.each_items(src, true)?;
                self.each_depth += 1;
                let mut result = Ok(Flow::Normal);
                for item in items {
                    let sc = Scope::new(Some(scope.clone()));
                    let parts = match self.unpack(&deref_val(item), &names) {
                        Ok(p) => p,
                        Err(err) => {
                            result = Err(err);
                            break;
                        }
                    };
                    for (n, part) in names.iter().zip(parts) {
                        sc.insert(n, self.new_var(part));
                    }
                    match self.exec_block(body, &sc) {
                        Ok(Flow::Normal | Flow::Continue) => {}
                        Ok(Flow::Break) => break,
                        other => {
                            result = other;
                            break;
                        }
                    }
                }
                self.each_depth -= 1;
                return result;
            }
            StmtKind::Each(name, e, body) => {
                let src = self.eval(e, scope)?;
                // `each fruit in fruits` + `change fruit to ...` updates the list itself
                let writeback = matches!(src, Value::List(_)) && matches!(e, Expr::Ident(_) | Expr::Field(..) | Expr::Index(..));
                let items: Vec<Value> = match src {
                    Value::List(l) => (*l).clone(),
                    Value::Num(n) => (0..n.max(0.0) as i64).map(|i| Value::Num(i as f64)).collect(),
                    Value::Str(s) => s.chars().map(|c| Value::Str(c.to_string())).collect(),
                    Value::Obj(o) if o.type_name == "dict" => o.fields.iter().map(|(k, _)| Value::Str(k.clone())).collect(),
                    Value::Obj(o) if o.type_name == "stack" || o.type_name == "queue" => match o.get("items") {
                        Some(Value::List(l)) => (**l).clone(),
                        _ => vec![],
                    },
                    v => return self.rt(format!("can't loop over {}", v.type_name())),
                };
                self.each_depth += 1;
                let mut result = Ok(Flow::Normal);
                let fresh_each_time = declares(body);
                let mut reuse: Option<(Rc<Scope>, VarRef)> = None;
                let name: Rc<str> = Rc::from(name.as_str());
                // for explaining errors: `enemy` is enemies[i]
                let source = if writeback {
                    self.place_of(e, scope).map(|(var, path, _)| Rc::new(Source { var, name: crate::diagnose::code(e), path }))
                } else {
                    None
                };
                for (i, item) in items.into_iter().enumerate() {
                    if self.in_mimic && self.each_depth == 1 {
                        self.mimic_steps += 1;
                    }
                    // the last round's scope and variable are reused unless something kept hold of
                    // them (a short function or `on` made inside the loop, or the rewind history)
                    let (sc, lv) = match reuse.take() {
                        Some((sc, lv)) if Rc::strong_count(&sc) == 1 && Rc::strong_count(&lv) == 2 => {
                            *lv.borrow_mut() = Var::new(item.clone(), self.in_mimic, self.origin());
                            (sc, lv)
                        }
                        Some((sc, _)) if Rc::strong_count(&sc) == 1 => {
                            let lv = self.new_var(item.clone());
                            sc.insert_rc(name.clone(), lv.clone());
                            (sc, lv)
                        }
                        _ => {
                            let sc = Scope::new(Some(scope.clone()));
                            let lv = self.new_var(item.clone());
                            sc.insert_rc(name.clone(), lv.clone());
                            (sc, lv)
                        }
                    };
                    if let Some(src) = &source {
                        lv.borrow_mut().source = Some((src.clone(), Some(i)));
                    }
                    let r = self.exec_block(body, &sc);
                    if !fresh_each_time {
                        reuse = Some((sc, lv.clone()));
                    }
                    if writeback {
                        let now = lv.borrow().get().clone();
                        if !equals(&item, &now) {
                            self.write_back(e, scope, i, now)?;
                        }
                    }
                    match r {
                        Ok(Flow::Normal | Flow::Continue) => {}
                        Ok(Flow::Break) => break,
                        other => {
                            result = other;
                            break;
                        }
                    }
                }
                self.each_depth -= 1;
                return result;
            }
            StmtKind::While(c, body) => {
                while self.eval(c, scope)?.truthy() {
                    match self.exec_child(body, scope)? {
                        Flow::Break => break,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                        _ => {}
                    }
                }
            }
            StmtKind::Define(def) => {
                let f = Value::Func(Rc::new(Func { def: def.clone(), closure: scope.clone(), file: self.cur_file }));
                scope.insert(&def.name, self.new_var(f));
            }
            StmtKind::Return(es) => {
                let v = match es.as_slice() {
                    [] => Value::None,
                    [one] => self.eval(one, scope)?,
                    many => {
                        let mut vals = Vec::with_capacity(many.len());
                        for e in many {
                            vals.push(self.eval(e, scope)?);
                        }
                        Value::list(vals)
                    }
                };
                return Ok(Flow::Return(v));
            }
            StmtKind::Rewind { target, steps } => self.rewind(target.as_deref(), steps.as_ref(), scope)?,
            StmtKind::Data(decl) => {
                let d = self.make_type(decl, scope)?;
                scope.insert(&decl.name, self.new_var(d));
            }
            StmtKind::Attempt(body, ename, handler) => match self.exec_child(body, scope) {
                Ok(f) => return Ok(f),
                Err(e) if e.is_switch() => return Err(e),
                Err(e) => {
                    let sc = Scope::new(Some(scope.clone()));
                    sc.insert(ename, self.new_var(Value::Str(e.msg.clone())));
                    return self.exec_block(handler, &sc);
                }
            },
            StmtKind::Include(path) => {
                let p = self.dir.join(path);
                let key = p.canonicalize().unwrap_or_else(|_| p.clone());
                if self.included.insert(key) {
                    let src = match std::fs::read_to_string(&p) {
                        Ok(s) => s,
                        Err(e) => return self.rt(format!("can't include \"{}\": {}", path, e)),
                    };
                    let new_dir = p.parent().map(|d| d.to_path_buf()).unwrap_or_default();
                    let old_dir = std::mem::replace(&mut self.dir, new_dir);
                    self.include_depth += 1;
                    // an include adds its names to the file it's in (inside a module: to that module)
                    let top = self.top.clone();
                    let r = self.run_in(&src, &p.display().to_string(), &top);
                    self.include_depth -= 1;
                    self.dir = old_dir;
                    r?;
                }
            }
            StmtKind::Use { path, alias } => {
                let m = self.use_module(path)?;
                match scope.get_local(alias) {
                    Some(var) if matches!(var.borrow().get(), Value::Module(_)) => var.borrow_mut().set(m),
                    Some(_) => return self.rt(format!("'{0}' already exists - pick another name with  use \"{1}\" as other_name", alias, path)),
                    None => scope.insert(alias, self.new_var(m)),
                }
            }
            StmtKind::Param(name, e) => {
                let v = self.eval(e, scope)?;
                self.bind_global(name, v);
            }
            StmtKind::Style(name, props, states) => {
                let mut o = Obj::new("style");
                for (k, e) in props {
                    let v = self.eval(e, scope)?;
                    o.set(k, v);
                }
                for (state, items) in states {
                    let key = match state.as_str() {
                        "hover" | "hovered" => "hover_look",
                        "press" | "pressed" | "click" | "active" => "press_look",
                        other => return self.rt(format!("unknown style state 'on {}' - use 'on hover' or 'on press'", other)),
                    };
                    let mut so = Obj::new("style");
                    for (k, e) in items {
                        let v = self.eval(e, scope)?;
                        so.set(k, v);
                    }
                    o.set(key, Value::obj(so));
                }
                scope.insert(name, self.new_var(Value::obj(o)));
            }
            StmtKind::Stage(node) => {
                let v = self.build_node(node, scope, None)?;
                self.finish_stage(v);
            }
            StmtKind::Scene(node) => {
                let v = self.build_node(node, scope, None)?;
                let v = self.finish_scene(v);
                self.bind_global("scene", v);
            }
            StmtKind::Gui(node) => {
                let (v, events) = layout::build_gui(self, node, scope)?;
                let name = match &v {
                    Value::Obj(o) => match o.get("label") {
                        Some(Value::Str(l)) => l.replace(|c: char| !c.is_alphanumeric(), "_"),
                        _ => "gui".to_string(),
                    },
                    _ => "gui".to_string(),
                };
                if !self.gui_roots.contains(&name) {
                    self.gui_roots.push(name.clone());
                }
                self.bind_inputs(&v);
                self.bind_global(&name, v);
                // text "Coins: {coins}" keeps showing the current value
                self.live_labels.retain(|(root, _, _, _)| *root != name);
                let mut live = vec![];
                live_label_paths(node, vec![], &mut live);
                for (path, e) in live {
                    self.live_labels.push((name.clone(), path, e, scope.clone()));
                }
                // `on button.hover ...` inside an element: point `button` (or `self`) at that element
                for (path, kind, stmts) in events {
                    let mut target = Expr::Ident(name.clone());
                    for i in path {
                        target = Expr::Index(
                            Box::new(Expr::Field(Box::new(target), "children".into())),
                            Box::new(Expr::Num(i as f64)),
                        );
                    }
                    let g = self.globals.clone();
                    for st in &stmts {
                        let st = st.subst(&kind, &target).subst("self", &target);
                        self.exec(&st, &g)?;
                    }
                }
            }
            StmtKind::On(e, body) => self.handlers.push(Handler {
                cond: e.clone(),
                body: body.clone(),
                scope: scope.clone(),
                was_true: false,
                has_wait: block_has_wait(body),
                line: s.line,
                file: self.cur_file,
                broken: false,
            }),
            StmtKind::OnTouch { a, b, names, start, body } => self.touch_handlers.push(TouchHandler {
                a: a.clone(),
                b: b.clone(),
                names: names.clone(),
                start: *start,
                body: body.clone(),
                scope: scope.clone(),
                has_wait: block_has_wait(body),
                line: s.line,
                file: self.cur_file,
                broken: false,
                pairs: HashSet::new(),
            }),
            StmtKind::OnEvent { name, var, body } => self.event_handlers.push(EventHandler {
                name: name.clone(),
                var: var.clone(),
                body: body.clone(),
                scope: scope.clone(),
                has_wait: block_has_wait(body),
            }),
            StmtKind::Trigger(name, value) => {
                let v = match value {
                    Some(e) => self.eval(e, scope)?,
                    None => Value::None,
                };
                self.trigger(name, v)?;
            }
            StmtKind::Persist { body, steps, until } => {
                let prev = self.capture.replace(vec![]);
                let r = self.exec_child(body, scope);
                let caps = std::mem::replace(&mut self.capture, prev).unwrap_or_default();
                r?;
                let remaining = match steps {
                    Some(e) => Some(self.eval(e, scope)?.as_num(s.line)? as i64),
                    None => None,
                };
                let (mut while_cond, mut sc) = (None, scope.clone());
                if remaining.is_none() && until.is_none() {
                    match &self.current_on {
                        Some((c, on_scope)) => {
                            while_cond = Some(c.clone());
                            sc = on_scope.clone();
                        }
                        None => {
                            return self.rt("persist needs an ending like 'for 45 steps' or 'until <condition>' (or put it inside an 'on' block)")
                        }
                    }
                }
                self.persists.push(PersistLayer { caps, remaining, until: until.clone(), while_cond, scope: sc });
            }
            StmtKind::Mimic(shadow, src, body) => self.mimic(shadow, src, body, scope)?,
            StmtKind::Save { value, path, append } => self.save(value, path, *append, scope)?,
            StmtKind::Push(value, target) => {
                let v = self.eval(value, scope)?;
                self.mutate(target, scope, |it, slot| {
                    match slot {
                        Value::List(l) => Rc::make_mut(l).push(v),
                        Value::Obj(o) if o.type_name == "stack" || o.type_name == "queue" => {
                            let ob = Rc::make_mut(o);
                            match ob.fields.iter().position(|(k, x)| k == "items" && matches!(x, Value::List(_))) {
                                Some(i) => {
                                    if let Value::List(l) = &mut ob.fields[i].1 {
                                        Rc::make_mut(l).push(v);
                                    }
                                }
                                None => ob.set("items", Value::list(vec![v])),
                            }
                        }
                        other => return it.rt(format!("push needs a list, stack or queue, but this is {}", other.type_name())),
                    }
                    Ok(Value::None)
                })?;
            }
            StmtKind::Test(name, body) => {
                // tests only run with `eza test`, and only in the file being tested
                if self.test_mode && self.include_depth == 0 {
                    match self.exec_child(body, scope) {
                        Ok(_) => {
                            self.tests_passed += 1;
                            println!("  PASS  {}", name);
                        }
                        Err(e) if e.is_switch() => return Err(e),
                        Err(e) => {
                            self.tests_failed += 1;
                            let e = e.in_file(self.file_name(self.cur_file));
                            println!("  FAIL  {}\n{}\n", name, crate::indent(&e.to_string(), 8));
                        }
                    }
                }
            }
            StmtKind::Expect(e) => self.expect(e, scope)?,
            StmtKind::Play(path, props) => {
                let p = self.eval(path, scope)?.display();
                let full = self.resolve_path(&p);
                if !full.is_file() {
                    return self.rt(format!("can't find the sound \"{}\" (paths are relative to the script)", p));
                }
                let (mut looping, mut volume, mut speed) = (false, 1.0, 1.0);
                for (k, e) in props {
                    let v = self.eval(e, scope)?;
                    match k.as_str() {
                        "loop" | "looping" | "repeat" => looping = v.truthy(),
                        "volume" => volume = v.as_num(s.line)?.max(0.0) as f32,
                        "speed" | "pitch" => speed = v.as_num(s.line)?.max(0.01) as f32,
                        other => return self.rt(format!("play doesn't have a '{}' setting (use loop, volume or speed)", other)),
                    }
                }
                if self.frame_mode {
                    self.sound_cmds.push(SoundCmd::Play { path: full, looping, volume, speed });
                }
            }
            StmtKind::Stop(target) => {
                let path = match target {
                    Some(e) => {
                        let p = self.eval(e, scope)?.display();
                        Some(self.resolve_path(&p))
                    }
                    None => None,
                };
                if self.frame_mode {
                    self.sound_cmds.push(SoundCmd::Stop(path));
                }
            }
            StmtKind::Emit { count, from, at } => {
                let n = self.eval(count, scope)?.as_num(s.line)?.clamp(0.0, 5000.0) as usize;
                let name = match deref_val(self.eval(from, scope)?) {
                    Value::Obj(o) if o.type_name == "particles" => match o.get("name") {
                        Some(Value::Str(n)) => n.clone(),
                        _ => return self.rt("emit needs particles with a name, like:  particles name=\"sparks\""),
                    },
                    v => return self.rt(format!("emit needs particles, but this is {}", v.type_name())),
                };
                let at = match at {
                    Some(e) => match self.eval(e, scope)? {
                        Value::List(l) => Some(l.iter().map(|x| x.as_num(s.line)).collect::<R<Vec<f64>>>()?),
                        v => return self.rt(format!("emit ... at needs a position like 10,20 but got {}", v.repr())),
                    },
                    None => None,
                };
                if self.frame_mode {
                    self.bursts.push((name, n, at));
                }
            }
            StmtKind::Go(e) => {
                let p = self.eval(e, scope)?.display();
                let mut full = self.resolve_path(&p);
                if full.extension().is_none() {
                    full.set_extension("eza");
                }
                if !full.is_file() {
                    return self.rt(format!("can't go to \"{}\" - there's no such script next to this one", p));
                }
                self.go_to = Some(full);
                return Err(EzaError::switch(s.line));
            }
            StmtKind::Prefab(name, node) => {
                let p = Value::Prefab(Rc::new(PrefabDef { name: name.clone(), node: node.clone(), scope: scope.clone() }));
                scope.insert(name, self.new_var(p));
            }
            StmtKind::Destroy(e) => self.destroy(e, scope)?,
            StmtKind::Wait { .. } => {
                return self.rt(
                    "wait only works inside a function or an 'on' block (and inside if/each/while there). It can't be used at the top level or inside attempt, persist or mimic.",
                )
            }
            StmtKind::Break => return Ok(Flow::Break),
            StmtKind::Continue => return Ok(Flow::Continue),
            StmtKind::Tween { target, value, steps, ease: kind } => {
                let (name, path) = self.lvalue(target, scope)?;
                let Some(var) = scope.lookup(&name) else {
                    return self.rt(self.did_you_mean(format!("'{0}' doesn't exist yet - create it first with '{0} = ...'.", name), &name, scope));
                };
                let var = if path.is_empty() { var } else { deref_var(var) };
                let to = self.eval(value, scope)?;
                let n = self.eval(steps, scope)?.as_num(s.line)?.max(1.0) as usize;
                let from = get_path(var.borrow().get(), &path, s.line)?;
                // interruption handshake: the old path dies, the new one starts from where we are now
                self.cancel_tweens(&var, &path);
                let mut frames = Vec::with_capacity(n);
                for k in 1..=n {
                    frames.push(lerp(&from, &to, ease(k as f64 / n as f64, kind), s.line)?);
                }
                self.set_animating(&var, true);
                self.tweens.push(Tween { var, path, frames, idx: 0 });
            }
            StmtKind::Tick(n) => {
                let n = match n {
                    Some(e) => self.eval(e, scope)?.as_num(s.line)? as i64,
                    None => 1,
                };
                for _ in 0..n {
                    self.tick()?;
                }
            }
        }
        Ok(Flow::Normal)
    }

    fn lvalue<'e>(&mut self, e: &'e Expr, scope: &Rc<Scope>) -> R<(&'e str, Vec<PathEl>)> {
        match e {
            Expr::Ident(n) => Ok((n.as_str(), vec![])),
            Expr::Field(inner, f) => {
                let (n, mut p) = self.lvalue(inner, scope)?;
                p.push(PathEl::Field(f.clone()));
                Ok((n, p))
            }
            Expr::Index(inner, idx) => {
                let (n, mut p) = self.lvalue(inner, scope)?;
                p.push(PathEl::Index(self.eval(idx, scope)?));
                Ok((n, p))
            }
            _ => self.rt("this can't be changed"),
        }
    }

    fn change(&mut self, target: &Expr, mode: ChangeMode, e: &Expr, scope: &Rc<Scope>) -> R<()> {
        let (name, path) = self.lvalue(target, scope)?;
        let var = match scope.lookup(name) {
            Some(v) => v,
            None => {
                let err = self.rt::<()>(self.did_you_mean(format!("'{0}' doesn't exist yet - create it first with '{0} = ...'.", name), name, scope)).unwrap_err();
                return Err(self.explain_missing(err, name, scope));
            }
        };
        let var = if path.is_empty() { var } else { deref_var(var) };
        self.module_guard(&var, name, &path)?;
        if let Some(g) = &self.guard {
            if !Rc::ptr_eq(g, &var) && !var.borrow().sandbox {
                return Err(EzaError::syntax(
                    self.line,
                    format!("Cannot modify global variable '{}' inside an isolated simulation block.", name),
                ));
            }
        }
        let v = self.eval(e, scope)?;
        if !self.tweens.is_empty() {
            self.cancel_tweens(&var, &path);
        }
        // the most common change of all: a plain number variable, changed to or by a number
        if path.is_empty() && self.capture.is_none() {
            let old = match var.borrow().get() {
                Value::Num(n) => Some(*n),
                _ => None,
            };
            if let (Some(old), Value::Num(d)) = (old, &v) {
                let new = if mode == ChangeMode::By { old + d } else { *d };
                self.commit(&var, Value::Num(new));
                return Ok(());
            }
        }
        // big lists/dictionaries are changed in place; everything else becomes a new history step
        let big = is_big(var.borrow().get());
        let mut root = if big { var.borrow_mut().take() } else { var.borrow().get().clone() };
        let res = self.apply_change(&mut root, &path, mode, v, name, &var);
        if big {
            var.borrow_mut().put(root);
            res
        } else {
            res?;
            self.commit(&var, root);
            Ok(())
        }
    }

    fn apply_change(&mut self, root: &mut Value, path: &[PathEl], mode: ChangeMode, v: Value, name: &str, var: &VarRef) -> R<()> {
        let line = self.line;
        // `change enemy.strategy to "retreat"` may add a new property / dictionary key
        let new_key = matches!(path.last(), Some(PathEl::Field(_)) | Some(PathEl::Index(Value::Str(_))));
        if mode == ChangeMode::To && new_key && !path_exists(root, path) {
            let (last, parent_path) = path.split_last().unwrap();
            let key = match last {
                PathEl::Field(f) => f.clone(),
                PathEl::Index(k) => key_str(k),
            };
            match path_mut(root, parent_path, line)? {
                Value::Obj(o) => Rc::make_mut(o).set(&key, v),
                other => return self.rt(format!("can't add '{}' to {}", key, other.type_name())),
            }
            if let Some(caps) = &mut self.capture {
                caps.push(Capture { var: var.clone(), path: path.to_vec(), undo: Undo::Restore(Value::None) });
            }
            return Ok(());
        }
        let slot = path_mut(root, path, line)?;
        if self.capture.is_some() {
            let old = slot.clone();
            let undo = match (mode, &v, &old) {
                (ChangeMode::By, Value::Num(d), Value::Num(_)) => Undo::Delta(*d),
                _ => Undo::Restore(old),
            };
            let caps = self.capture.as_mut().unwrap();
            let dup = matches!(undo, Undo::Restore(_)) && caps.iter().any(|c| Rc::ptr_eq(&c.var, var) && path_eq(&c.path, path));
            if !dup {
                caps.push(Capture { var: var.clone(), path: path.to_vec(), undo });
            }
        }
        match mode {
            ChangeMode::To => *slot = v,
            ChangeMode::By => {
                // `change list by item` appends in place (vectors of the same size add instead)
                let vector_add = matches!((&*slot, &v), (Value::List(a), Value::List(b)) if is_vec(a) && is_vec(b) && a.len() == b.len());
                match slot {
                    Value::List(l) if !vector_add => Rc::make_mut(l).push(v),
                    _ => {
                        let new = self.add_by(slot, &v, name)?;
                        *slot = new;
                    }
                }
            }
        }
        Ok(())
    }

    /// `change enemies.count to 5` from outside the module: modules keep their variables to themselves.
    fn module_guard(&self, var: &VarRef, name: &str, path: &[PathEl]) -> R<()> {
        if let (Value::Module(m), Some(PathEl::Field(f))) = (var.borrow().get(), path.first()) {
            return self.rt(format!(
                "a module's variables can only be changed by its own code - add a function to {}.eza that changes '{}', then call {}.that_function()",
                m.name, f, name
            ));
        }
        Ok(())
    }

    fn did_you_mean(&self, base: String, name: &str, scope: &Rc<Scope>) -> String {
        let mut names = vec![];
        scope.collect_names(&mut names);
        names.extend(methods::BUILTINS.iter().map(|s| s.to_string()));
        match crate::suggest::closest(name, &names) {
            Some(c) => format!("{} Did you mean '{}'?", base, c),
            None => base,
        }
    }

    /// Changes the value at `target` in place with `f`, which returns (new value, result).
    /// Used by `push` and `pop`; follows the same rules as `change` (mimic guard, history).
    fn mutate(
        &mut self,
        target: &Expr,
        scope: &Rc<Scope>,
        f: impl FnOnce(&Self, &mut Value) -> R<Value>,
    ) -> R<Value> {
        let (name, path) = self.lvalue(target, scope)?;
        let Some(var) = scope.lookup(&name) else {
            return self.rt(self.did_you_mean(format!("'{}' doesn't exist yet.", name), &name, scope));
        };
        let var = if path.is_empty() { var } else { deref_var(var) };
        self.module_guard(&var, name, &path)?;
        if let Some(g) = &self.guard {
            if !Rc::ptr_eq(g, &var) && !var.borrow().sandbox {
                return Err(EzaError::syntax(
                    self.line,
                    format!("Cannot modify global variable '{}' inside an isolated simulation block.", name),
                ));
            }
        }
        let big = is_big(var.borrow().get());
        let mut root = if big { var.borrow_mut().take() } else { var.borrow().get().clone() };
        let line = self.line;
        let res = path_mut(&mut root, &path, line).and_then(|slot| f(self, slot));
        if big {
            var.borrow_mut().put(root);
            res
        } else {
            let out = res?;
            self.commit(&var, root);
            Ok(out)
        }
    }

    // ---------- 2D ----------

    /// Fills in a sprite or tilemap: defaults, textures (a variable or a file path), size and tile grid.
    fn finish_sprite(&mut self, o: &mut Obj, scope: &Rc<Scope>) -> R<()> {
        let defaults: [(&str, Value); 7] = [
            ("position", Value::list(vec![Value::Num(0.0), Value::Num(0.0)])),
            ("rotation", Value::Num(0.0)),
            ("scale", Value::list(vec![Value::Num(1.0), Value::Num(1.0)])),
            ("visible", Value::Bool(true)),
            ("flip_x", Value::Bool(false)),
            ("flip_y", Value::Bool(false)),
            ("layer", Value::Num(0.0)),
        ];
        for (k, v) in defaults {
            if o.get(k).is_none() {
                o.set(k, v);
            }
        }
        let is_map = o.type_name == "tilemap";
        // texture=hero (a variable holding an image) or texture="assets/hero.png" (loaded and cached)
        let key = if is_map { ["tiles", "source", "texture"].into_iter().find(|k| o.get(k).is_some()) } else { Some("texture") };
        if let Some(key) = key {
            if let Some(Value::Str(s)) = o.get(key).cloned() {
                let img = match scope.lookup(&s).map(|v| v.borrow().get().clone()) {
                    Some(v @ Value::Image(_)) => v,
                    _ => self.load(&s)?,
                };
                if !matches!(img, Value::Image(_)) {
                    return self.rt(format!("{}=\"{}\" isn't an image (use a .png, .jpg, .gif or .bmp file)", key, s));
                }
                o.set("texture", img);
            }
        }
        let num = |o: &Obj, k: &str| match o.get(k) {
            Some(Value::Num(n)) => Some(*n),
            _ => None,
        };
        let tex_size = match o.get("texture") {
            Some(Value::Image(i)) => Some([i.width as f64, i.height as f64]),
            _ => None,
        };
        if is_map {
            let ts = num(o, "tile_size").unwrap_or(32.0);
            o.set("tile_size", Value::Num(ts));
            let layout = match o.get("layout").cloned() {
                Some(Value::Str(s)) => match scope.lookup(&s).map(|v| v.borrow().get().clone()) {
                    Some(Value::Str(text)) => text,
                    _ if s.contains('\n') => s,
                    _ => match self.load(&s)? {
                        Value::Str(text) => text,
                        _ => return self.rt("a tilemap layout must be a text file"),
                    },
                },
                Some(v) => v.display(),
                None => String::new(),
            };
            let grid = parse_tile_layout(&layout);
            let cols = grid.iter().map(|r| r.len()).max().unwrap_or(0);
            o.set("rows", Value::Num(grid.len() as f64));
            o.set("columns", Value::Num(cols as f64));
            o.set("width", Value::Num(cols as f64 * ts));
            o.set("height", Value::Num(grid.len() as f64 * ts));
            let grid = grid.into_iter().map(|r| Value::list(r.into_iter().map(|i| Value::Num(i as f64)).collect())).collect();
            o.set("grid", Value::list(grid));
            if o.get("solid").is_none() {
                o.set("solid", Value::Bool(true));
            }
            return Ok(());
        }
        // sprite sheets: frame_size=32,32 slices the texture into frames
        let frame = match o.get("frame_size") {
            Some(Value::Num(n)) => Some([*n, *n]),
            v => crate::two_d::vec2(v),
        };
        if let Some(f) = frame {
            o.set("frame_size", Value::list(vec![Value::Num(f[0]), Value::Num(f[1])]));
            if o.get("frame").is_none() {
                o.set("frame", Value::Num(0.0));
            }
        }
        let natural = frame.or(tex_size).unwrap_or([32.0, 32.0]);
        if num(o, "width").is_none() {
            o.set("width", Value::Num(natural[0]));
        }
        if num(o, "height").is_none() {
            o.set("height", Value::Num(natural[1]));
        }
        Ok(())
    }

    /// Adds a `stage` block's objects to the 2D world (several stage blocks add up).
    fn finish_stage(&mut self, v: Value) {
        let Value::Obj(mut new) = v else { return };
        // particles in a stage live in 2D
        if let Some(Value::List(kids)) = new.get("children").cloned() {
            let kids: Vec<Value> = kids
                .iter()
                .map(|k| match k {
                    Value::Obj(o) if o.type_name == "particles" => {
                        let mut o = (**o).clone();
                        if let Some(Value::List(p)) = o.get("position").cloned() {
                            o.set("position", Value::list(p.iter().take(2).cloned().collect()));
                        }
                        Value::obj(o)
                    }
                    other => other.clone(),
                })
                .collect();
            Rc::make_mut(&mut new).set("children", Value::list(kids));
        }
        let mut stage = match self.global("stage") {
            Some(Value::Obj(old)) => {
                let mut old = (*old).clone();
                let mut kids = match old.get("children") {
                    Some(Value::List(l)) => (**l).clone(),
                    _ => vec![],
                };
                if let Some(Value::List(l)) = new.get("children") {
                    kids.extend(l.iter().cloned());
                }
                for (k, val) in &new.fields {
                    if k != "children" && k != "kind" {
                        old.set(k, val.clone());
                    }
                }
                old.set("children", Value::list(kids));
                old
            }
            _ => (*new).clone(),
        };
        if stage.get("camera").is_none() {
            let mut cam = Obj::new("camera2d");
            cam.set("position", Value::list(vec![Value::Num(0.0), Value::Num(0.0)]));
            cam.set("zoom", Value::Num(1.0));
            cam.set("rotation", Value::Num(0.0));
            stage.set("camera", Value::obj(cam));
        }
        if stage.get("gravity").is_none() {
            stage.set("gravity", Value::Num(-980.0));
        }
        // sprites with name="hero" become variables; named physics sprites get simulated
        if let Some(Value::List(kids)) = new.get("children") {
            for k in kids.iter() {
                if let Value::Obj(o) = k {
                    if let Some(Value::Str(name)) = o.get("name") {
                        if matches!(o.get("physics"), Some(Value::Bool(true))) && !self.bodies.contains(name) {
                            self.bodies.push(name.clone());
                        }
                        if !self.scene_names.contains(name) && o.type_name != "particles" {
                            self.scene_names.push(name.clone());
                        }
                        self.bind_global(name, k.clone());
                    }
                }
            }
        }
        self.bind_global("stage", Value::obj(stage));
    }

    /// Everything a ray can hit: (what `hit.object` should be, the object's data).
    fn world_objects(&self) -> Vec<(Value, Rc<Obj>)> {
        let mut out: Vec<(Value, Rc<Obj>)> = vec![];
        for name in &self.scene_names {
            if let Some(v @ Value::Obj(_)) = self.global(name) {
                if let Value::Obj(o) = &v {
                    out.push((v.clone(), o.clone()));
                }
            }
        }
        for (id, d) in &self.entities {
            if d.alive {
                if let Value::Obj(o) = d.var.borrow().get() {
                    out.push((Value::Entity(Rc::new(EntityHandle { id: *id, var: d.var.clone() })), o.clone()));
                }
            }
        }
        // unnamed objects declared in the scene/stage
        fn walk(v: &Value, names: &[String], out: &mut Vec<(Value, Rc<Obj>)>) {
            let Value::Obj(o) = v else { return };
            let named = matches!(o.get("name"), Some(Value::Str(n)) if names.contains(n)) || names.contains(&o.type_name);
            let skip = NOISES.contains(&o.type_name.as_str()) || ["scene", "stage", "camera", "light"].contains(&o.type_name.as_str());
            if !named && !skip && o.get("position").is_some() {
                out.push((v.clone(), o.clone()));
            }
            if let Some(Value::List(ch)) = o.get("children") {
                for c in ch.iter() {
                    walk(c, names, out);
                }
            }
        }
        for root in ["scene", "stage"] {
            if let Some(t) = self.global(root) {
                walk(&t, &self.scene_names, &mut out);
            }
        }
        out
    }

    /// raycast(from=..., direction=..., distance=...) in 2D or 3D. Returns none, or a hit with
    /// .object, .point and .distance (plus .tile and .cell for tilemaps).
    pub fn raycast(&self, from: Vec<f64>, dir: Vec<f64>, max: f64) -> R<Value> {
        let n = from.len();
        if !(n == 2 || n == 3) || dir.len() != n {
            return self.rt("raycast needs a start point and direction that are both 2D ([x, y]) or both 3D");
        }
        let len = dir.iter().map(|d| d * d).sum::<f64>().sqrt();
        if len == 0.0 {
            return self.rt("raycast's direction can't be [0, 0]");
        }
        let dir: Vec<f64> = dir.iter().map(|d| d / len).collect();
        let mut best: Option<(f64, Value, Option<(i64, usize, usize)>)> = None;
        let mut consider = |t: f64, v: &Value, tile: Option<(i64, usize, usize)>| {
            if t <= max && best.as_ref().map_or(true, |b| t < b.0) {
                best = Some((t, v.clone(), tile));
            }
        };
        for (val, o) in self.world_objects() {
            if matches!(o.get("visible"), Some(Value::Bool(false))) || matches!(o.get("destroyed"), Some(Value::Bool(true))) {
                continue;
            }
            if n == 2 {
                if o.type_name == "tilemap" {
                    if matches!(o.get("solid"), Some(Value::Bool(false))) {
                        continue;
                    }
                    let end = [from[0] + dir[0] * max, from[1] + dir[1] * max];
                    let lo = [from[0].min(end[0]), from[1].min(end[1])];
                    let hi = [from[0].max(end[0]), from[1].max(end[1])];
                    for t in crate::two_d::tiles_in(&o, lo, hi) {
                        if let Some(d) = crate::two_d::ray_box(&from, &dir, &t.center, &t.half) {
                            consider(d, &val, Some((t.index, t.col, t.row)));
                        }
                    }
                } else if let Some((c, h)) = crate::two_d::aabb2d(&o) {
                    if let Some(d) = crate::two_d::ray_box(&from, &dir, &c, &h) {
                        consider(d, &val, None);
                    }
                }
            } else if !crate::two_d::is_2d(&o) {
                if let Some((c, h)) = methods::aabb(&o) {
                    if let Some(d) = crate::two_d::ray_box(&from, &dir, &c, &h) {
                        consider(d, &val, None);
                    }
                }
            }
        }
        let Some((t, object, tile)) = best else { return Ok(Value::None) };
        let mut hit = Obj::new("hit");
        hit.set("object", object);
        hit.set("point", Value::list(from.iter().zip(&dir).map(|(f, d)| Value::Num(f + d * t)).collect()));
        hit.set("distance", Value::Num(t));
        if let Some((index, col, row)) = tile {
            hit.set("tile", Value::Num(index as f64));
            hit.set("cell", Value::list(vec![Value::Num(col as f64), Value::Num(row as f64)]));
        }
        Ok(Value::obj(hit))
    }

    /// `expect a == b` - on failure, shows both sides' values.
    fn expect(&mut self, e: &Expr, scope: &Rc<Scope>) -> R<()> {
        if let Expr::Binary(op, l, r) = e {
            if op.is_comparison() {
                let (a, b) = (self.eval(l, scope)?, self.eval(r, scope)?);
                if self.binop(op.as_str(), &a, &b)?.truthy() {
                    return Ok(());
                }
                let err = self.rt::<()>(format!("expect failed: {} {} {} is not true", a.repr(), op.as_str(), b.repr())).unwrap_err();
                return Err(self.explain_expect(err, l, r, &a, &b, scope));
            }
        }
        if self.eval(e, scope)?.truthy() {
            Ok(())
        } else {
            self.rt("expect failed: the condition was false")
        }
    }

    // ---------- time-travel debugger ----------

    /// (frame the timeline is showing, newest recorded frame)
    #[allow(dead_code)] // used by the engine build
    pub fn debug_cursor(&self) -> (u64, u64) {
        let cur = if self.gptr > 0 { self.log[self.gptr - 1].1 } else { 0 };
        (cur, self.log.back().map_or(0, |l| l.1))
    }

    /// Moves the whole world `delta` frames back (negative) or forward (positive) along the
    /// recorded timeline. Nothing is lost until the game makes a new change.
    #[allow(dead_code)] // used by the engine build
    pub fn scrub(&mut self, delta: i64) {
        let (cur, max) = self.debug_cursor();
        if delta < 0 {
            let target = cur.saturating_sub(delta.unsigned_abs());
            while self.gptr > 0 && self.log[self.gptr - 1].1 > target {
                self.gptr -= 1;
                self.log[self.gptr].0.borrow_mut().rewind(1);
            }
        } else {
            let target = cur.saturating_add(delta as u64).min(max);
            while self.gptr < self.log.len() && self.log[self.gptr].1 <= target {
                self.log[self.gptr].0.borrow_mut().forward(1);
                self.gptr += 1;
            }
        }
    }

    /// One line per global variable, `*` marking those that changed on the shown frame.
    #[allow(dead_code)] // used by the engine build
    pub fn debug_watch(&self, limit: usize) -> Vec<String> {
        let (cur, _) = self.debug_cursor();
        let mut changed = vec![];
        let mut i = self.gptr;
        while i > 0 && self.log[i - 1].1 == cur {
            changed.push(Rc::as_ptr(&self.log[i - 1].0));
            i -= 1;
        }
        let skip = ["keyboard", "mouse", "screen", "global", "pi", "scene", "stage", "sound"];
        let mut rows = self.globals.entries();
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        let mut out = vec![];
        for (name, var) in rows {
            if skip.contains(&name.as_str()) || self.gui_roots.contains(&name) {
                continue;
            }
            let v = var.borrow().get().clone();
            if matches!(v, Value::Func(_) | Value::Native(_) | Value::Data(_) | Value::Prefab(_) | Value::Module(_)) {
                continue;
            }
            let mark = if changed.contains(&Rc::as_ptr(&var)) { "*" } else { " " };
            let mut text = v.repr();
            if text.chars().count() > 90 {
                text = text.chars().take(87).collect::<String>() + "...";
            }
            out.push(format!("{} {} = {}", mark, name, text));
            if out.len() >= limit {
                break;
            }
        }
        out
    }

    // ---------- files ----------

    /// Paths are relative to the script's folder.
    pub fn resolve_path(&self, p: &str) -> PathBuf {
        let pb = Path::new(p);
        if pb.is_absolute() {
            pb.to_path_buf()
        } else {
            self.dir.join(pb)
        }
    }

    pub fn load(&mut self, p: &str) -> R<Value> {
        let full = self.resolve_path(p);
        let lower = p.to_lowercase();
        if [".png", ".jpg", ".jpeg", ".gif", ".bmp"].iter().any(|e| lower.ends_with(e)) {
            if let Some(img) = self.image_cache.get(&full) {
                return Ok(Value::Image(img.clone()));
            }
            let img = image::open(&full).map_err(|e| EzaError::runtime(self.line, format!("can't load the image \"{}\": {}", p, e)))?;
            let rgba = img.to_rgba8();
            let data = Rc::new(ImageData { width: rgba.width(), height: rgba.height(), rgba: rgba.into_raw(), path: p.to_string() });
            self.image_cache.insert(full, data.clone());
            return Ok(Value::Image(data));
        }
        let text = std::fs::read_to_string(&full).map_err(|e| EzaError::runtime(self.line, format!("can't open \"{}\": {}", p, e)))?;
        let text = text.strip_prefix('\u{feff}').map(|s| s.to_string()).unwrap_or(text);
        if lower.ends_with(".json") {
            return crate::json::from_json(&text).map_err(|m| EzaError::runtime(self.line, format!("\"{}\" isn't valid JSON: {}", p, m)));
        }
        if lower.ends_with(".csv") {
            return Ok(crate::tools::from_csv(&text));
        }
        Ok(Value::Str(text))
    }

    fn save(&mut self, value: &Expr, path: &Expr, append: bool, scope: &Rc<Scope>) -> R<()> {
        use std::io::Write as _;
        let v = self.eval(value, scope)?;
        let p = self.eval(path, scope)?.display();
        let full = self.resolve_path(&p);
        if let Some(dir) = full.parent() {
            if !dir.as_os_str().is_empty() {
                let _ = std::fs::create_dir_all(dir);
            }
        }
        let fail = |e: String| EzaError::runtime(self.line, format!("can't write \"{}\": {}", p, e));
        if append {
            // each append adds one line
            let mut text = v.display();
            text.push('\n');
            let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&full).map_err(|e| fail(e.to_string()))?;
            return f.write_all(text.as_bytes()).map_err(|e| fail(e.to_string()));
        }
        if p.to_lowercase().ends_with(".csv") {
            let text = crate::tools::to_csv(self, &v)?;
            return std::fs::write(&full, text).map_err(|e| fail(e.to_string()));
        }
        match &v {
            Value::Str(s) => std::fs::write(&full, s).map_err(|e| fail(e.to_string())),
            Value::Image(img) => image::save_buffer(&full, &img.rgba, img.width, img.height, image::ColorType::Rgba8)
                .map_err(|e| fail(e.to_string())),
            other => {
                let text = crate::json::to_json(other).map_err(|m| EzaError::runtime(self.line, m))?;
                std::fs::write(&full, text).map_err(|e| fail(e.to_string()))
            }
        }
    }

    // ---------- background tasks (wait) ----------

    fn spawn_task(&mut self, body: Rc<Vec<Stmt>>, scope: Rc<Scope>) -> R<()> {
        let mut t = Task { frames: vec![Frame::Block { stmts: body, idx: 0, scope }], wake: self.frame, file: self.cur_file };
        if !self.run_task(&mut t)? {
            self.tasks.push(t);
        }
        Ok(())
    }

    /// Runs a task until it finishes (true) or hits a `wait` (false).
    fn run_task(&mut self, t: &mut Task) -> R<bool> {
        let outer = std::mem::replace(&mut self.cur_file, t.file);
        let r = self.run_task_steps(t);
        self.cur_file = outer;
        r.map_err(|e| e.in_file(self.file_name(t.file)))
    }

    fn run_task_steps(&mut self, t: &mut Task) -> R<bool> {
        enum Act {
            Pop,
            Push(Frame),
            While(Expr, Rc<Vec<Stmt>>, Rc<Scope>),
            Run(Rc<Vec<Stmt>>, usize, Rc<Scope>),
        }
        loop {
            let act = match t.frames.last_mut() {
                None => return Ok(true),
                Some(Frame::Block { stmts, idx, scope }) => {
                    if *idx >= stmts.len() {
                        Act::Pop
                    } else {
                        *idx += 1;
                        Act::Run(stmts.clone(), *idx - 1, scope.clone())
                    }
                }
                Some(Frame::Each { var, items, i, body, scope }) => {
                    if *i >= items.len() {
                        Act::Pop
                    } else {
                        let sc = Scope::new(Some(scope.clone()));
                        if var.contains(',') {
                            let names: Vec<String> = each_names(var).into_iter().map(String::from).collect();
                            let parts = self.unpack(&deref_val(items[*i].clone()), &names)?;
                            for (n, part) in names.iter().zip(parts) {
                                sc.insert(n, self.new_var(part));
                            }
                        } else {
                            sc.insert(var.as_str(), self.new_var(items[*i].clone()));
                        }
                        *i += 1;
                        Act::Push(Frame::Block { stmts: body.clone(), idx: 0, scope: sc })
                    }
                }
                Some(Frame::While { cond, body, scope }) => Act::While(cond.clone(), body.clone(), scope.clone()),
            };
            match act {
                Act::Pop => {
                    t.frames.pop();
                }
                Act::Push(f) => t.frames.push(f),
                Act::While(c, body, scope) => {
                    if self.eval(&c, &scope)?.truthy() {
                        let sc = Scope::new(Some(scope));
                        t.frames.push(Frame::Block { stmts: body, idx: 0, scope: sc });
                    } else {
                        t.frames.pop();
                    }
                }
                Act::Run(stmts, i, scope) => {
                    let s = &stmts[i];
                    self.line = s.line;
                    let flow = match &s.kind {
                        StmtKind::Wait { steps, seconds } => {
                            let n = self.eval(steps, &scope)?.as_num(s.line)?;
                            let n = if *seconds { n * 60.0 } else { n };
                            t.wake = self.frame + n.round().max(1.0) as u64;
                            return Ok(false);
                        }
                        StmtKind::If(arms, els)
                            if arms.iter().any(|(_, b)| block_has_wait(b))
                                || els.as_ref().map_or(false, |b| block_has_wait(b)) =>
                        {
                            let mut chosen: Option<&Vec<Stmt>> = None;
                            for (c, b) in arms {
                                if self.eval(c, &scope)?.truthy() {
                                    chosen = Some(b);
                                    break;
                                }
                            }
                            if let Some(b) = chosen.or(els.as_ref()) {
                                let sc = Scope::new(Some(scope.clone()));
                                t.frames.push(Frame::Block { stmts: Rc::new(b.clone()), idx: 0, scope: sc });
                            }
                            Flow::Normal
                        }
                        StmtKind::Each(name, e, body) if block_has_wait(body) => {
                            let items: Vec<Value> = match self.eval(e, &scope)? {
                                src if name.contains(',') => self.each_items(src, true)?,
                                Value::List(l) => (*l).clone(),
                                Value::Num(n) => (0..n.max(0.0) as i64).map(|i| Value::Num(i as f64)).collect(),
                                Value::Str(s) => s.chars().map(|c| Value::Str(c.to_string())).collect(),
                                Value::Obj(o) if o.type_name == "dict" => o.fields.iter().map(|(k, _)| Value::Str(k.clone())).collect(),
                    Value::Obj(o) if o.type_name == "stack" || o.type_name == "queue" => match o.get("items") {
                        Some(Value::List(l)) => (**l).clone(),
                        _ => vec![],
                    },
                    v => return self.rt(format!("can't loop over {}", v.type_name())),
                            };
                            t.frames.push(Frame::Each {
                                var: name.clone(),
                                items,
                                i: 0,
                                body: Rc::new(body.clone()),
                                scope: scope.clone(),
                            });
                            Flow::Normal
                        }
                        StmtKind::While(c, body) if block_has_wait(body) => {
                            t.frames.push(Frame::While { cond: c.clone(), body: Rc::new(body.clone()), scope: scope.clone() });
                            Flow::Normal
                        }
                        _ => self.exec(s, &scope)?,
                    };
                    match flow {
                        Flow::Normal => {}
                        Flow::Return(_) => t.frames.clear(),
                        Flow::Break | Flow::Continue => {
                            let is_break = matches!(flow, Flow::Break);
                            while let Some(f) = t.frames.last() {
                                if matches!(f, Frame::Each { .. } | Frame::While { .. }) {
                                    if is_break {
                                        t.frames.pop();
                                    }
                                    break;
                                }
                                t.frames.pop();
                            }
                        }
                    }
                }
            }
        }
    }

    // ---------- spawn / destroy ----------

    fn spawn(&mut self, prefab: &Expr, at: &Option<Box<Expr>>, props: &[(String, Expr)], into: &Option<Box<Expr>>, scope: &Rc<Scope>) -> R<Value> {
        if self.in_mimic {
            return self.rt("can't spawn inside a mimic block (it would change the real world)");
        }
        let p = self.eval(prefab, scope)?;
        let Value::Prefab(def) = &p else {
            return self.rt(format!("spawn needs a prefab (declare one with 'prefab Name'), got {}", p.type_name()));
        };
        let def = def.clone();
        let mut over = vec![];
        for (k, e) in props {
            over.push((k.clone(), self.eval(e, scope)?));
        }
        let at_v = match at {
            Some(e) => Some(self.eval(e, scope)?),
            None => None,
        };
        let mut o = match self.build_node(&def.node, &def.scope, None)? {
            Value::Obj(o) => (*o).clone(),
            _ => return self.rt("this prefab didn't build an object"),
        };
        if let Some(a) = at_v {
            if !matches!(&a, Value::List(l) if (l.len() == 2 || l.len() == 3) && l.iter().all(|x| matches!(x, Value::Num(_)))) {
                return self.rt("'at' needs a position like 5,0,3 (or 120,40 in 2D), or a vector");
            }
            o.set("position", a);
        }
        for (k, v) in over {
            o.set(&k, v);
        }
        o.set("prefab", Value::Str(def.name.clone()));
        physics_defaults(&mut o);
        let id = self.next_entity;
        self.next_entity += 1;
        let var = self.new_var(Value::obj(o));
        let handle = Value::Entity(Rc::new(EntityHandle { id, var: var.clone() }));
        let mut groups = vec![];
        if let Some(target) = into {
            // spawn Coin into coins: add it to that list too
            let (name, path) = self.lvalue(target, scope)?;
            let Some(list_var) = scope.lookup(name) else {
                return self.rt(format!("'{0}' doesn't exist yet - make an empty list first:  {0} = []", name));
            };
            let list_var = if path.is_empty() { list_var } else { deref_var(list_var) };
            let root = list_var.borrow().get().clone();
            let mut items = match get_path(&root, &path, self.line)? {
                Value::List(l) => (*l).clone(),
                other => return self.rt(format!("into needs a list, but {} is {}", crate::diagnose::code(target), other.type_name())),
            };
            items.push(handle.clone());
            let nr = set_path(root, &path, Value::list(items), self.line)?;
            self.commit(&list_var, nr);
            groups.push((list_var, path));
        }
        self.entities.insert(id, DynEntity { var, alive: true, groups });
        Ok(handle)
    }

    /// The live objects spawned from one prefab (`Bullet.all`), oldest first.
    fn live_copies(&self, prefab: &str) -> Vec<Value> {
        self.entities
            .iter()
            .filter(|(_, d)| d.alive && matches!(d.var.borrow().get(), Value::Obj(o) if matches!(o.get("prefab"), Some(Value::Str(p)) if p == prefab)))
            .map(|(id, d)| Value::Entity(Rc::new(EntityHandle { id: *id, var: d.var.clone() })))
            .collect()
    }

    /// Marks a spawned object dead and takes it out of the lists it was spawned `into`.
    fn kill_entity(&mut self, id: u64) -> R<()> {
        let groups = match self.entities.get_mut(&id) {
            Some(d) if d.alive => {
                d.alive = false;
                std::mem::take(&mut d.groups)
            }
            _ => return Ok(()),
        };
        for (var, path) in groups {
            let root = var.borrow().get().clone();
            let Ok(Value::List(l)) = get_path(&root, &path, self.line) else { continue };
            if !l.iter().any(|v| matches!(v, Value::Entity(h) if h.id == id)) {
                continue;
            }
            let kept: Vec<Value> = l.iter().filter(|v| !matches!(v, Value::Entity(h) if h.id == id)).cloned().collect();
            let nr = set_path(root, &path, Value::list(kept), self.line)?;
            self.commit(&var, nr);
        }
        Ok(())
    }

    fn destroy(&mut self, e: &Expr, scope: &Rc<Scope>) -> R<()> {
        if self.in_mimic {
            return Err(EzaError::syntax(self.line, "destroy can't be used inside a mimic block (it would change the real world)"));
        }
        match self.eval(e, scope)? {
            Value::Entity(h) => {
                self.kill_entity(h.id)?;
                if !self.frame_mode {
                    self.entities.retain(|_, d| d.alive);
                }
            }
            // destroy Bullet.all / destroy coins: every spawned object in the list
            Value::List(items) if items.iter().all(|v| matches!(v, Value::Entity(_))) => {
                for v in items.iter() {
                    if let Value::Entity(h) = v {
                        self.kill_entity(h.id)?;
                    }
                }
                if !self.frame_mode {
                    self.entities.retain(|_, d| d.alive);
                }
            }
            Value::Obj(_) => {
                // something declared in the scene: hide it, and stop physics and collisions for it
                let (name, path) = self.lvalue(e, scope)?;
                let Some(var) = scope.lookup(&name) else { return Ok(()) };
                for (field, val) in [("visible", false), ("destroyed", true)] {
                    let mut p = path.clone();
                    p.push(PathEl::Field(field.into()));
                    let root = var.borrow().get().clone();
                    let nr = set_path(root, &p, Value::Bool(val), self.line)?;
                    self.commit(&var, nr);
                }
                self.bodies.retain(|b| *b != name);
            }
            Value::List(_) => return self.rt("destroy takes a list only when it holds spawned objects; for things declared in the scene, destroy them one at a time with each"),
            other => return self.rt(format!("can't destroy {}", other.type_name())),
        }
        Ok(())
    }

    #[allow(dead_code)] // used by the engine build
    pub fn entity_value(&self, id: u64) -> Option<Value> {
        self.entities.get(&id).map(|d| d.var.borrow().get().clone())
    }

    #[allow(dead_code)] // used by the engine build
    pub fn entity_states(&self) -> Vec<(u64, bool)> {
        self.entities.iter().map(|(i, d)| (*i, d.alive)).collect()
    }

    #[allow(dead_code)] // used by the engine build
    pub fn prune_dead(&mut self) {
        self.entities.retain(|_, d| d.alive);
    }

    /// Every variable holding a `physics=true` object: scene entities plus spawned ones.
    pub fn physics_vars(&self) -> Vec<VarRef> {
        let mut v: Vec<VarRef> = self.bodies.iter().filter_map(|n| self.globals.lookup(n)).collect();
        for d in self.entities.values() {
            if d.alive && matches!(d.var.borrow().get(), Value::Obj(o) if matches!(o.get("physics"), Some(Value::Bool(true)))) {
                v.push(d.var.clone());
            }
        }
        v
    }

    /// Spawned objects that asked to be obstacles with `solid=true`.
    pub fn solid_values(&self) -> Vec<Value> {
        self.entities
            .values()
            .filter(|d| d.alive)
            .map(|d| d.var.borrow().get().clone())
            .filter(|v| matches!(v, Value::Obj(o) if matches!(o.get("solid"), Some(Value::Bool(true)))))
            .collect()
    }

    fn write_back(&mut self, e: &Expr, scope: &Rc<Scope>, i: usize, v: Value) -> R<()> {
        let (name, mut path) = self.lvalue(e, scope)?;
        let Some(var) = scope.lookup(&name) else { return Ok(()) };
        // the purity guard still applies: a mimic can't edit the real world through a loop
        if let Some(g) = &self.guard {
            if !Rc::ptr_eq(g, &var) && !var.borrow().sandbox {
                return Ok(());
            }
        }
        path.push(PathEl::Index(Value::Num(i as f64)));
        let root = var.borrow().get().clone();
        if get_path(&root, &path, self.line).is_err() {
            return Ok(()); // the list got shorter while looping
        }
        let nr = set_path(root, &path, v, self.line)?;
        self.commit(&var, nr);
        Ok(())
    }

    fn cancel_tweens(&mut self, var: &VarRef, path: &[PathEl]) {
        let before = self.tweens.len();
        self.tweens.retain(|t| !(Rc::ptr_eq(&t.var, var) && overlaps(&t.path, path)));
        if self.tweens.len() != before && !self.tweens.iter().any(|t| Rc::ptr_eq(&t.var, var)) {
            self.set_animating(var, false);
        }
    }

    /// Flips `.tweening` on an object in place (not a new history step).
    fn set_animating(&self, var: &VarRef, on: bool) {
        if let Value::Obj(o) = var.borrow_mut().get_mut() {
            Rc::make_mut(o).set("tweening", Value::Bool(on));
        }
    }

    /// Loads and runs a script's top level, following `go to` into other scripts.
    /// From the previous script, `global`, `sound` (the volume) and sounds it just asked for are carried over.
    pub fn start(path: &Path, prev: Option<Interp>, frame_mode: bool) -> R<Interp> {
        let (mut path, mut prev) = (path.to_path_buf(), prev);
        loop {
            let name = path.display().to_string();
            let src = std::fs::read_to_string(&path).map_err(|e| EzaError::runtime(0, format!("can't open {}: {}", name, e)))?;
            let mut it = Interp::new(&path);
            it.frame_mode = frame_mode;
            if let Some(mut old) = prev.take() {
                for keep in ["global", "sound"] {
                    if let Some(v) = old.global(keep) {
                        let var = it.new_var(v);
                        it.base.insert(keep, var);
                    }
                }
                it.sound_cmds = std::mem::take(&mut old.sound_cmds);
            }
            match it.run_source(&src, &name) {
                Ok(()) => return Ok(it),
                Err(e) if e.is_switch() => {
                    path = it.go_to.take().unwrap_or(path);
                    prev = Some(it);
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// `textbox`, `slider`, `checkbox` and `dropdown` with name="x" keep their value in the variable x.
    fn bind_inputs(&mut self, v: &Value) {
        let Value::Obj(o) = v else { return };
        let kind = o.type_name.as_str();
        if let (true, Some(Value::Str(name))) = (layout::INPUT_KINDS.contains(&kind), o.get("name")) {
            let given = match kind {
                "checkbox" => o.get("checked").or(o.get("value")).map(|v| Value::Bool(v.truthy())),
                "textbox" => o.get("text").or(o.get("value")).map(|v| Value::Str(v.display())),
                _ => o.get("value").cloned(),
            };
            let fallback = match kind {
                "checkbox" => Value::Bool(false),
                "slider" => o.get("min").cloned().unwrap_or(Value::Num(0.0)),
                "dropdown" => match o.get("options") {
                    Some(Value::List(l)) => l.first().cloned().unwrap_or(Value::Str(String::new())),
                    Some(other) => other.clone(),
                    None => Value::Str(String::new()),
                },
                _ => Value::Str(String::new()),
            };
            match given {
                Some(g) => self.bind_global(name, g),
                None if self.global(name).is_none() => self.bind_global(name, fallback),
                None => {}
            }
        }
        if let Some(Value::List(ch)) = o.get("children") {
            for c in ch.iter() {
                self.bind_inputs(c);
            }
        }
    }

    /// Called by the engine when someone types, drags or clicks an input: a normal, rewindable change.
    #[allow(dead_code)]
    pub fn set_var(&mut self, name: &str, v: Value) {
        if self.global(name).is_some_and(|old| equals(&old, &v)) {
            return;
        }
        self.bind_global(name, v);
    }

    /// Called by the engine: flips a flag like `hover` on a GUI element without making a history step.
    #[allow(dead_code)]
    pub fn set_gui_field(&mut self, root: &str, path: &[usize], field: &str, v: Value) {
        let Some(var) = self.globals.lookup(root) else { return };
        let mut p = vec![];
        for &i in path {
            p.push(PathEl::Field("children".into()));
            p.push(PathEl::Index(Value::Num(i as f64)));
        }
        p.push(PathEl::Field(field.into()));
        let cur = var.borrow().get().clone();
        if get_path(&cur, &p, 0).map_or(false, |old| equals(&old, &v)) {
            return;
        }
        if let Ok(new) = set_path(cur, &p, v, 0) {
            *var.borrow_mut().get_mut() = new;
        }
    }

    fn add_by(&self, old: &Value, by: &Value, name: &str) -> R<Value> {
        Ok(match (old, by) {
            (Value::Num(a), Value::Num(b)) => Value::Num(a + b),
            (Value::Str(a), b) => Value::Str(format!("{}{}", a, b.display())),
            (Value::List(a), Value::List(b)) if is_vec(a) && is_vec(b) && a.len() == b.len() => {
                Value::list(a.iter().zip(b.iter()).map(|(p, q)| Value::Num(num_of(p) + num_of(q))).collect())
            }
            (Value::List(l), b) => {
                let mut v = (**l).clone();
                v.push(b.clone());
                Value::list(v)
            }
            (a, b) => {
                return self.rt(format!("can't change '{}' ({}) by {} ({})", name, a.type_name(), b.repr(), b.type_name()))
            }
        })
    }

    fn rewind(&mut self, target: Option<&str>, steps: Option<&Expr>, scope: &Rc<Scope>) -> R<()> {
        let n = match steps {
            Some(e) => Some(self.eval(e, scope)?.as_num(self.line)?.max(0.0) as usize),
            None => None,
        };
        match target {
            Some(name) => {
                let var = match scope.lookup(name) {
                    Some(v) => v,
                    None => return self.rt(format!("can't rewind '{}' - it doesn't exist", name)),
                };
                let mut v = var.borrow_mut();
                match n {
                    None => v.idx = 0,
                    Some(n) => {
                        if v.rewind(n) {
                            println!("[Notice] Clamped {} rewind at its original creation value.", name);
                        }
                    }
                }
            }
            None if self.frame_mode && n.is_some() => {
                // engine: steps are frames, counted back from the newest recorded frame
                let n = n.unwrap() as u64;
                let top = if self.gptr > 0 { self.log[self.gptr - 1].1 } else { 0 };
                let target = top.saturating_sub(n);
                while self.gptr > 0 && self.log[self.gptr - 1].1 > target {
                    self.gptr -= 1;
                    self.log[self.gptr].0.borrow_mut().rewind(1);
                }
            }
            None => {
                let n = n.unwrap_or(self.gptr);
                let mut done = 0;
                while done < n && self.gptr > 0 {
                    self.gptr -= 1;
                    self.log[self.gptr].0.borrow_mut().rewind(1);
                    done += 1;
                }
                if done < n {
                    println!("[Notice] Clamped scene rewind at the beginning of history.");
                }
            }
        }
        Ok(())
    }

    fn mimic_budget(&self) -> usize {
        match self.global("mimic_budget") {
            Some(Value::Num(n)) => n.max(1.0) as usize,
            _ => 1000,
        }
    }

    fn mimic(&mut self, shadow: &str, src: &Expr, body: &Rc<Vec<Stmt>>, scope: &Rc<Scope>) -> R<()> {
        if self.mimic_queue.iter().any(|j| j.shadow == shadow) {
            return Ok(()); // this shadow is still being simulated
        }
        let obj = match deref_val(self.eval(src, scope)?) {
            Value::Obj(o) => (*o).clone(),
            v => return self.rt(format!("mimic needs an object to copy (a scene entity or data), got {}", v.type_name())),
        };
        // the RNG seed is snapshotted now, so the prediction matches what the real game would do
        let job = MimicJob { shadow: shadow.to_string(), obj, body: body.clone(), scope: scope.clone(), rng: self.rng };
        if self.mimic_used < self.mimic_budget() {
            return self.run_mimic(job);
        }
        // over this frame's budget: wait with ready = false and finish on a later frame
        let mut waiting = job.obj.clone();
        waiting.set("ready", Value::Bool(false));
        waiting.set("mimic_steps", Value::Num(0.0));
        waiting.set("mimic_collided", Value::Bool(false));
        let v = self.new_var(Value::obj(waiting));
        self.globals.insert(shadow, v);
        self.mimic_queue.push(job);
        Ok(())
    }

    fn run_mimic(&mut self, job: MimicJob) -> R<()> {
        let mut o = job.obj;
        o.set("ready", Value::Bool(false));
        o.set("mimic_steps", Value::Num(0.0));
        o.set("mimic_collided", Value::Bool(false));
        let var = self.new_var(Value::obj(o));
        self.globals.insert(&job.shadow, var.clone());

        let saved = (self.guard.replace(var.clone()), self.in_mimic, self.mimic_steps, self.each_depth, self.rng);
        self.in_mimic = true;
        self.mimic_steps = 0;
        self.each_depth = 0;
        self.rng = job.rng;
        let r = self.exec_child(&job.body, &job.scope);
        let steps = self.mimic_steps;
        // restoring rng = the simulation never disturbs the real game's randomness
        (self.guard, self.in_mimic, self.mimic_steps, self.each_depth, self.rng) = saved;
        r?;
        self.mimic_used += steps.max(1);

        let mut fin = match var.borrow().get() {
            Value::Obj(o) => (**o).clone(),
            _ => Obj::new("Shadow"),
        };
        let mut state = fin.clone();
        state.fields.retain(|(k, _)| !k.starts_with("mimic_") && k != "ready");
        fin.set("mimic_steps", Value::Num(steps as f64));
        fin.set("mimic_state", Value::obj(state));
        fin.set("ready", Value::Bool(true));
        var.borrow_mut().set(Value::obj(fin));
        self.shadows.push(job.shadow);
        Ok(())
    }

    /// `a, b = value`: the parts, one per name.
    fn unpack(&self, v: &Value, names: &[String]) -> R<Vec<Value>> {
        let what = names.join(", ");
        match v {
            Value::List(items) if items.len() == names.len() => Ok((**items).clone()),
            Value::List(items) => self.rt(format!(
                "{} needs a list of {} things, but this one has {}",
                what,
                names.len(),
                items.len()
            )),
            other => self.rt(format!("{} needs a list of {} things to unpack, but got {}", what, names.len(), other.type_name())),
        }
    }

    /// What `each` goes through. With several names, a dictionary gives [key, value] pairs.
    fn each_items(&self, src: Value, pairs: bool) -> R<Vec<Value>> {
        Ok(match src {
            Value::List(l) => (*l).clone(),
            Value::Num(n) => (0..n.max(0.0) as i64).map(|i| Value::Num(i as f64)).collect(),
            Value::Str(s) => s.chars().map(|c| Value::Str(c.to_string())).collect(),
            Value::Obj(o) if o.type_name == "dict" && pairs => {
                o.fields.iter().map(|(k, v)| Value::list(vec![Value::Str(k.clone()), v.clone()])).collect()
            }
            Value::Obj(o) if o.type_name == "dict" => o.fields.iter().map(|(k, _)| Value::Str(k.clone())).collect(),
            Value::Obj(o) if o.type_name == "stack" || o.type_name == "queue" => match o.get("items") {
                Some(Value::List(l)) => (**l).clone(),
                _ => vec![],
            },
            v => return self.rt(format!("can't loop over {}", v.type_name())),
        })
    }

    /// Advance one frame: clear last frame's shadows, age persist layers, fire `on` handlers.
    pub fn tick(&mut self) -> R<()> {
        for name in std::mem::take(&mut self.shadows) {
            // its one readable frame is over; a stub keeps `if shadow.ready` safe
            let mut stub = Obj::new("Shadow");
            stub.set("ready", Value::Bool(false));
            let v = self.new_var(Value::obj(stub));
            self.globals.insert(&name, v);
        }
        self.frame += 1;
        // time-slicing: queued simulations finish as this frame's budget allows
        self.mimic_used = 0;
        while !self.mimic_queue.is_empty() && self.mimic_used < self.mimic_budget() {
            let job = self.mimic_queue.remove(0);
            self.run_mimic(job)?;
        }
        let mut i = 0;
        while i < self.tweens.len() {
            let t = &mut self.tweens[i];
            let val = t.frames[t.idx].clone();
            t.idx += 1;
            let (var, path, done) = (t.var.clone(), t.path.clone(), t.idx >= t.frames.len());
            let root = var.borrow().get().clone();
            let nr = set_path(root, &path, val, self.line)?;
            self.commit(&var, nr);
            if done {
                self.tweens.remove(i);
                if !self.tweens.iter().any(|t| Rc::ptr_eq(&t.var, &var)) {
                    self.set_animating(&var, false);
                }
            } else {
                i += 1;
            }
        }
        let mut i = 0;
        while i < self.persists.len() {
            let mut expired = false;
            if let Some(r) = &mut self.persists[i].remaining {
                *r -= 1;
                expired = *r <= 0;
            }
            if !expired {
                let sc = self.persists[i].scope.clone();
                if let Some(c) = self.persists[i].until.clone() {
                    expired = self.eval(&c, &sc)?.truthy();
                }
                if let Some(c) = self.persists[i].while_cond.clone() {
                    expired = expired || !self.eval(&c, &sc)?.truthy();
                }
            }
            if expired {
                let layer = self.persists.remove(i);
                for c in layer.caps.into_iter().rev() {
                    let root = c.var.borrow().get().clone();
                    if matches!(c.undo, Undo::Restore(Value::None)) && matches!(c.path.last(), Some(PathEl::Field(_))) {
                        // the property didn't exist before the persist: flags go back to false, anything else goes away
                        let nr = if matches!(get_path(&root, &c.path, self.line), Ok(Value::Bool(_))) {
                            set_path(root, &c.path, Value::Bool(false), self.line)?
                        } else {
                            remove_path(root, &c.path, self.line)?
                        };
                        self.commit(&c.var, nr);
                        continue;
                    }
                    let new = match c.undo {
                        Undo::Restore(v) => v,
                        Undo::Delta(d) => match get_path(&root, &c.path, self.line)? {
                            Value::Num(n) => Value::Num(n - d),
                            other => other,
                        },
                    };
                    let nr = set_path(root, &c.path, new, self.line)?;
                    self.commit(&c.var, nr);
                }
            } else {
                i += 1;
            }
        }
        self.animate_sprites()?;
        // `on scene.ticks` runs every frame; other `on` conditions fire once each time they become true
        let n = self.handlers.len();
        for i in 0..n {
            if self.handlers[i].broken {
                continue;
            }
            let (file, line, cond) = (self.handlers[i].file, self.handlers[i].line, self.handlers[i].cond.clone());
            let outer = std::mem::replace(&mut self.cur_file, file);
            let r = self.run_handler(i);
            self.cur_file = outer;
            if let Err(e) = r {
                if e.is_switch() {
                    return Err(e);
                }
                let mut e = e.in_file(self.file_name(file));
                e.context.push(format!(
                    "on frame {} ({:.1} s in), inside  on {}  (line {})",
                    self.frame,
                    self.frame as f64 / 60.0,
                    crate::diagnose::code(&cond),
                    line
                ));
                if !self.frame_mode {
                    return Err(e);
                }
                // in a running game, only this `on` block stops; the rest carries on
                self.handlers[i].broken = true;
                e.context.push("that `on` block is switched off now; the rest of the game keeps running".to_string());
                self.errors.push(e);
            }
        }
        for i in 0..self.touch_handlers.len() {
            if self.touch_handlers[i].broken {
                continue;
            }
            let (file, line) = (self.touch_handlers[i].file, self.touch_handlers[i].line);
            let outer = std::mem::replace(&mut self.cur_file, file);
            let r = self.run_touch(i);
            self.cur_file = outer;
            if let Err(e) = r {
                if e.is_switch() {
                    return Err(e);
                }
                let h = &self.touch_handlers[i];
                let what = format!("{} {} {}", crate::diagnose::code(&h.a), if h.start { "touches" } else { "stops touching" }, crate::diagnose::code(&h.b));
                let mut e = e.in_file(self.file_name(file));
                e.context.push(format!("on frame {} ({:.1} s in), inside  on {}  (line {})", self.frame, self.frame as f64 / 60.0, what, line));
                if !self.frame_mode {
                    return Err(e);
                }
                self.touch_handlers[i].broken = true;
                e.context.push("that `on` block is switched off now; the rest of the game keeps running".to_string());
                self.errors.push(e);
            }
        }
        // background tasks (functions / handlers using `wait`) wake up here
        let mut tasks = std::mem::take(&mut self.tasks);
        let mut keep = vec![];
        for mut t in tasks.drain(..) {
            if t.wake > self.frame {
                keep.push(t);
                continue;
            }
            match self.run_task(&mut t) {
                Ok(true) => {}
                Ok(false) => keep.push(t),
                Err(e) if e.is_switch() => return Err(e),
                Err(mut e) => {
                    e.context.push(format!("on frame {} ({:.1} s in), in something that uses wait", self.frame, self.frame as f64 / 60.0));
                    if !self.frame_mode {
                        return Err(e);
                    }
                    self.errors.push(e);
                }
            }
        }
        keep.extend(std::mem::take(&mut self.tasks));
        self.tasks = keep;
        crate::physics::step(self)?;
        crate::physics::step2d(self)?;
        self.refresh_labels()?;
        Ok(())
    }

    /// GUI labels written with {values} show the values as they are now.
    fn refresh_labels(&mut self) -> R<()> {
        let mut i = 0;
        while i < self.live_labels.len() {
            let (root, path, e, sc) = self.live_labels[i].clone();
            let Some(var) = self.globals.lookup(&root) else {
                i += 1;
                continue;
            };
            let text = match self.eval(&e, &sc) {
                Ok(v) => Value::Str(v.display()),
                Err(err) => {
                    // stop updating this label, and say why
                    self.live_labels.remove(i);
                    let mut err = err;
                    err.context.push(format!("while updating the text \"{}\" in {}", crate::diagnose::code(&e), root));
                    if !self.frame_mode {
                        return Err(err);
                    }
                    self.errors.push(err);
                    continue;
                }
            };
            let mut p = vec![];
            for k in &path {
                p.push(PathEl::Field("children".into()));
                p.push(PathEl::Index(Value::Num(*k as f64)));
            }
            p.push(PathEl::Field("label".into()));
            let current = var.borrow().get().clone();
            match get_path(&current, &p, self.line) {
                Ok(old) if equals(&old, &text) => {}
                Ok(_) => {
                    let nr = set_path(current, &p, text, self.line)?;
                    self.commit(&var, nr);
                }
                Err(_) => {
                    self.live_labels.remove(i);
                    continue;
                }
            }
            i += 1;
        }
        Ok(())
    }

    /// Checks one `on` block's condition and runs it if it fires.
    fn run_handler(&mut self, i: usize) -> R<()> {
        let (cond, body, sc, has_wait) = {
            let h = &self.handlers[i];
            (h.cond.clone(), h.body.clone(), h.scope.clone(), h.has_wait)
        };
        let every_frame = matches!(&cond, Expr::Field(_, f) if f == "ticks");
        let now = every_frame || self.eval(&cond, &sc)?.truthy();
        let fire = every_frame || (now && !self.handlers[i].was_true);
        self.handlers[i].was_true = now;
        if fire {
            let prev = self.current_on.replace((cond, sc.clone()));
            let r = if has_wait {
                // a handler with `wait` runs in the background so it can pause between frames
                let child = Scope::new(Some(sc.clone()));
                self.spawn_task(body.clone(), child)
            } else {
                self.exec_child(&body, &sc).map(|_| ())
            };
            self.current_on = prev;
            r?;
        }
        Ok(())
    }

    /// One side of `on a touches b`: (a key that stays the same while it lives, the value to hand the block, its data).
    fn touch_side(&mut self, e: &Expr, scope: &Rc<Scope>) -> R<Vec<(String, Value, Rc<Obj>)>> {
        let v = self.eval(e, scope)?;
        let mut out = vec![];
        let alive = |o: &Obj| !matches!(o.get("destroyed"), Some(Value::Bool(true)));
        let add = |it: &Self, v: &Value, fallback: String, out: &mut Vec<(String, Value, Rc<Obj>)>| match v {
            Value::Entity(h) => {
                if it.entities.get(&h.id).map_or(false, |d| d.alive) {
                    if let Value::Obj(o) = h.var.borrow().get() {
                        out.push((format!("#{}", h.id), v.clone(), o.clone()));
                    }
                }
            }
            Value::Obj(o) if alive(o) => {
                let key = match o.get("name") {
                    Some(Value::Str(n)) => format!("n:{}", n),
                    _ => fallback,
                };
                out.push((key, v.clone(), o.clone()));
            }
            _ => {}
        };
        match &v {
            Value::Prefab(def) => {
                for c in self.live_copies(&def.name) {
                    add(self, &c, String::new(), &mut out);
                }
            }
            Value::List(items) => {
                for (i, x) in items.iter().enumerate() {
                    if !matches!(x, Value::Obj(_) | Value::Entity(_)) {
                        return self.rt(format!("touches needs objects, but {} has {} in it", crate::diagnose::code(e), x.type_name()));
                    }
                    add(self, x, format!("{}[{}]", crate::diagnose::code(e), i), &mut out);
                }
            }
            Value::Obj(_) | Value::Entity(_) => add(self, &v, crate::diagnose::code(e), &mut out),
            other => {
                return self.rt(format!(
                    "touches needs an object, a list of objects or a prefab (for every copy), but {} is {}",
                    crate::diagnose::code(e),
                    other.type_name()
                ))
            }
        }
        Ok(out)
    }

    /// Checks which pairs touch now and runs the block for each pair that just started (or stopped) touching.
    fn run_touch(&mut self, i: usize) -> R<()> {
        let (a, b, sc) = {
            let h = &self.touch_handlers[i];
            (h.a.clone(), h.b.clone(), h.scope.clone())
        };
        let left = self.touch_side(&a, &sc)?;
        let right = self.touch_side(&b, &sc)?;
        let mut now = HashSet::new();
        let mut found: Vec<(String, String, Value, Value)> = vec![];
        for (ka, va, oa) in &left {
            for (kb, vb, ob) in &right {
                if ka == kb {
                    continue;
                }
                if methods::collides(self, oa, &Value::Obj(ob.clone()))? {
                    now.insert((ka.clone(), kb.clone()));
                    found.push((ka.clone(), kb.clone(), va.clone(), vb.clone()));
                }
            }
        }
        let start = self.touch_handlers[i].start;
        let before = std::mem::replace(&mut self.touch_handlers[i].pairs, now.clone());
        let fire: Vec<(Value, Value)> = if start {
            found.into_iter().filter(|(ka, kb, _, _)| !before.contains(&(ka.clone(), kb.clone()))).map(|(_, _, x, y)| (x, y)).collect()
        } else {
            // pairs that touched last frame and don't now, while both are still around
            let mut out = vec![];
            for (ka, kb) in before.difference(&now) {
                let x = left.iter().find(|(k, _, _)| k == ka);
                let y = right.iter().find(|(k, _, _)| k == kb);
                if let (Some(x), Some(y)) = (x, y) {
                    out.push((x.1.clone(), y.1.clone()));
                }
            }
            out
        };
        for (x, y) in fire {
            let (names, body, has_wait) = {
                let h = &self.touch_handlers[i];
                (h.names.clone(), h.body.clone(), h.has_wait)
            };
            let child = Scope::new(Some(sc.clone()));
            if let Some((n1, n2)) = names {
                child.insert(&n1, self.new_var(x));
                if !n2.is_empty() {
                    child.insert(&n2, self.new_var(y));
                }
            }
            if has_wait {
                self.spawn_task(body, child)?;
            } else {
                self.exec_block(&body, &child)?;
            }
        }
        Ok(())
    }

    /// `trigger "boss_dead" with info`: runs every `on event "boss_dead"` block right away.
    fn trigger(&mut self, name: &str, v: Value) -> R<()> {
        let matching: Vec<(Option<String>, Rc<Vec<Stmt>>, Rc<Scope>, bool)> = self
            .event_handlers
            .iter()
            .filter(|h| h.name == name)
            .map(|h| (h.var.clone(), h.body.clone(), h.scope.clone(), h.has_wait))
            .collect();
        for (var, body, sc, has_wait) in matching {
            let child = Scope::new(Some(sc));
            if let Some(n) = var {
                child.insert(&n, self.new_var(v.clone()));
            }
            if has_wait {
                self.spawn_task(body, child)?;
            } else {
                self.exec_block(&body, &child)?;
            }
        }
        Ok(())
    }

    /// Sprites with `animation=[1, 2, 3]` (or the name of one of their `animations`) step through
    /// those frames at `fps` frames per second (default 8), looping unless `loop=false`.
    fn animate_sprites(&mut self) -> R<()> {
        let mut vars: Vec<VarRef> = self.scene_names.iter().filter_map(|n| self.globals.lookup(n)).collect();
        vars.extend(self.entities.values().filter(|d| d.alive).map(|d| d.var.clone()));
        for var in vars {
            let Value::Obj(o) = var.borrow().get().clone() else { continue };
            if o.type_name != "sprite" {
                continue; // on other objects `animation` is just a property
            }
            let Some(anim) = o.get("animation").cloned() else { continue };
            let key = Rc::as_ptr(&var) as usize;
            if matches!(anim, Value::None) {
                self.anims.remove(&key);
                continue;
            }
            let frames: Vec<f64> = match &anim {
                Value::List(l) => l.iter().filter_map(|v| if let Value::Num(n) = v { Some(*n) } else { None }).collect(),
                Value::Num(n) => vec![*n],
                Value::Str(name) => match o.get("animations") {
                    Some(Value::Obj(d)) => match d.get(name) {
                        Some(Value::List(l)) => l.iter().filter_map(|v| if let Value::Num(n) = v { Some(*n) } else { None }).collect(),
                        Some(Value::Num(n)) => vec![*n],
                        _ => {
                            let names: Vec<String> = d.fields.iter().map(|(k, _)| k.clone()).collect();
                            let hint = crate::suggest::closest(name, &names).map(|c| format!(" - did you mean \"{}\"?", c)).unwrap_or_default();
                            return self.rt(format!("{} has no animation called \"{}\"{} (it has: {})", o.type_name, name, hint, names.join(", ")));
                        }
                    },
                    _ => return self.rt(format!("animation=\"{}\" needs the sprite to have animations={{{}: [0, 1, 2]}}", name, name)),
                },
                other => return self.rt(format!("animation needs a list of frame numbers like [0, 1, 2], got {}", other.type_name())),
            };
            if frames.is_empty() {
                continue;
            }
            let id = anim.repr();
            let start = match self.anims.get(&key) {
                Some((k, st)) if *k == id => *st,
                _ => {
                    self.anims.insert(key, (id, self.frame));
                    self.frame
                }
            };
            let fps = match o.get("fps") {
                Some(Value::Num(f)) if *f > 0.0 => *f,
                _ => 8.0,
            };
            let looping = !matches!(o.get("loop"), Some(Value::Bool(false)));
            let step = ((self.frame - start) as f64 * fps / 60.0).floor() as usize;
            let (idx, done) = if looping { (step % frames.len(), false) } else { (step.min(frames.len() - 1), step >= frames.len()) };
            let frame = Value::Num(frames[idx]);
            let was_done = matches!(o.get("animation_done"), Some(Value::Bool(true)));
            if o.get("frame").map_or(false, |f| equals(f, &frame)) && was_done == done {
                continue;
            }
            let mut n = (*o).clone();
            n.set("frame", frame);
            n.set("animation_done", Value::Bool(done));
            self.commit(&var, Value::obj(n));
        }
        Ok(())
    }

    /// Errors from `on` blocks in a running game, since the engine last looked.
    #[allow(dead_code)] // used by the engine build
    pub fn take_errors(&mut self) -> Vec<EzaError> {
        std::mem::take(&mut self.errors)
    }

    // ---------- scene ----------

    fn build_node(&mut self, n: &DeclNode, scope: &Rc<Scope>, noise: Option<Value>) -> R<Value> {
        let kind = n.kind.as_str();
        let is_noise = NOISES.contains(&kind);
        let is_mesh = MESHES.contains(&kind);
        let mut o = Obj::new(kind);
        o.set("kind", Value::Str(kind.to_string()));
        if kind == "particles" {
            // a 3D position here; a stage turns it into 2D
            o.set("position", Value::list(vec![Value::Num(0.0), Value::Num(0.0), Value::Num(0.0)]));
            o.set("visible", Value::Bool(true));
        } else if !is_noise && !SPRITE_KINDS.contains(&kind) && kind != "stage" {
            let v3 = |a: f64| Value::list(vec![Value::Num(a), Value::Num(a), Value::Num(a)]);
            o.set("position", v3(0.0));
            o.set("rotation", v3(0.0));
            o.set("scale", v3(1.0));
            o.set("visible", Value::Bool(true));
            if !is_mesh && kind != "scene" {
                o.set("speed", Value::Num(1.0));
                o.set("health", Value::Num(100.0));
            }
        }
        if let Some(l) = &n.label {
            let v = self.eval(l, scope)?;
            o.set("label", v);
        }
        for (k, e) in &n.props {
            let v = self.eval(e, scope)?;
            o.set(k, v);
        }
        // sprite "hero" ... is the same as sprite name="hero" (like gui window "hud")
        if o.get("name").is_none() && kind != "scene" && kind != "stage" {
            if let Some(Value::Str(l)) = o.get("label").cloned() {
                o.set("name", Value::Str(l.replace(|c: char| !c.is_alphanumeric(), "_")));
            }
        }
        if SPRITE_KINDS.contains(&kind) {
            self.finish_sprite(&mut o, scope)?;
        }
        if matches!(o.get("physics"), Some(Value::Bool(true))) {
            if o.get("velocity").is_none() {
                o.set("velocity", zero_velocity(&o));
            }
            o.set("grounded", Value::Bool(false));
        }
        let num = |o: &Obj, k: &str, d: f64| match o.get(k) {
            Some(Value::Num(x)) => *x,
            _ => d,
        };
        if is_mesh {
            if let Some(nz) = &noise {
                o.set("noise", nz.clone());
            }
            let seg = num(&o, "seg", 1.0);
            let verts = match kind {
                "cube" => 24.0,
                "plane" | "sphere" => (seg + 1.0) * (seg + 1.0),
                _ => 0.0,
            };
            o.set("vertexCount", Value::Num(verts));
            let half = num(&o, "width", 1.0) / 2.0;
            let mut b = Obj::new("Bounds");
            b.set("min", Value::Num(-half));
            b.set("max", Value::Num(half));
            o.set("bounds", Value::obj(b));
        }
        let child_noise = if is_noise {
            let mut nz = Obj::new("Noise");
            nz.set("type", Value::Str(kind.to_string()));
            nz.set("freq", Value::Num(num(&o, "freq", 1.0)));
            nz.set("amp", Value::Num(num(&o, "amp", 1.0)));
            Some(Value::obj(nz))
        } else {
            noise
        };
        let mut children = vec![];
        for c in &n.children {
            children.push(self.build_node(c, scope, child_noise.clone())?);
        }
        o.set("children", Value::list(children));
        if let Some(h) = &n.handler {
            o.set("on_click", self.make_func("on_click", h.clone(), scope));
        }
        Ok(Value::obj(o))
    }

    /// Computes scene.bounds and binds named entities (player, enemy, ...) as variables.
    fn finish_scene(&mut self, v: Value) -> Value {
        fn walk(v: &Value, ext: &mut f64, ents: &mut Vec<(String, Value)>) {
            if let Value::Obj(o) = v {
                if let Some(Value::List(p)) = o.get("position") {
                    for c in p.iter() {
                        if let Value::Num(x) = c {
                            *ext = ext.max(x.abs());
                        }
                    }
                }
                if let Some(Value::Num(w)) = o.get("width") {
                    *ext = ext.max(w / 2.0);
                }
                if let (Some(Value::Str(n)), false) = (o.get("name"), o.type_name == "scene") {
                    ents.push((n.clone(), v.clone()));
                } else if !MESHES.contains(&o.type_name.as_str())
                    && !NOISES.contains(&o.type_name.as_str())
                    && o.type_name != "scene"
                {
                    ents.push((o.type_name.clone(), v.clone()));
                }
                if let Some(Value::List(ch)) = o.get("children") {
                    for c in ch.iter() {
                        walk(c, ext, ents);
                    }
                }
            }
        }
        let (mut ext, mut ents) = (0.0, vec![]);
        walk(&v, &mut ext, &mut ents);
        for (name, e) in ents {
            let is_particles = matches!(&e, Value::Obj(o) if o.type_name == "particles");
            if !self.scene_names.contains(&name) && !is_particles {
                self.scene_names.push(name.clone());
            }
            if let Value::Obj(o) = &e {
                let is_body = matches!(o.get("physics"), Some(Value::Bool(true))) && !MESHES.contains(&o.type_name.as_str());
                if is_body && !self.bodies.contains(&name) {
                    self.bodies.push(name.clone());
                }
            }
            self.bind_global(&name, e);
        }
        let Value::Obj(mut o) = v else { return v };
        let mut b = Obj::new("Bounds");
        b.set("min", Value::Num(-ext));
        b.set("max", Value::Num(ext));
        Rc::make_mut(&mut o).set("bounds", Value::obj(b));
        Value::Obj(o)
    }

    // ---------- expressions ----------

    pub fn eval(&mut self, e: &Expr, scope: &Rc<Scope>) -> R<Value> {
        Ok(match e {
            Expr::Num(n) => Value::Num(*n),
            Expr::Str(s) => Value::Str(s.clone()),
            Expr::Bool(b) => Value::Bool(*b),
            Expr::None => Value::None,
            Expr::Color(h) => Value::Color(Rc::new(parse_hex(h))),
            Expr::List(items) => {
                let mut v = Vec::with_capacity(items.len());
                for i in items {
                    v.push(self.eval(i, scope)?);
                }
                Value::list(v)
            }
            Expr::Ident(n) => match scope.value_of(n) {
                Some(v) => v,
                None if methods::BUILTINS.contains(&n.as_str()) => Value::Native(n.clone()),
                None => {
                    let err = self.rt::<()>(self.did_you_mean(format!("'{0}' doesn't exist yet - create it with '{0} = ...'.", n), n, scope)).unwrap_err();
                    return Err(self.explain_missing(err, n, scope));
                }
            },
            Expr::Field(obj, name) => {
                let o = self.eval(obj, scope)?;
                if let Value::Prefab(def) = &o {
                    match name.as_str() {
                        "all" => return Ok(Value::list(self.live_copies(&def.name))),
                        "count" => return Ok(Value::Num(self.live_copies(&def.name).len() as f64)),
                        _ => return self.rt(format!("a prefab only has .all (its live copies) and .count, not '.{}'", name)),
                    }
                }
                let o = if let Value::Entity(h) = &o {
                    if name == "alive" {
                        return Ok(Value::Bool(self.entities.get(&h.id).map_or(false, |e| e.alive)));
                    }
                    deref_val(o.clone())
                } else {
                    o
                };
                match &o {
                    Value::Obj(ob) => {
                        if let Some(v) = ob.get(name) {
                            return Ok(v.clone());
                        }
                        // `goblin.is_dead` works without () like the built-in methods, when it needs no arguments
                        if let Some((f, owner)) = ob.class.as_ref().and_then(|c| c.method(name)) {
                            if !f.def.params.is_empty() {
                                return self.rt(format!("{}.{} needs {} argument(s): {}.{}(...)", ob.type_name, name, f.def.params.len(), obj_text(obj), name));
                            }
                            return self.call_method(&f, owner, o.clone(), Some(obj), scope, vec![], vec![]);
                        }
                    }
                    Value::Module(m) => return self.module_get(m, name),
                    _ => {}
                }
                match methods::call(self, o, name, vec![], false) {
                    Ok(v) => v,
                    Err(err) => return Err(self.explain_field(err, obj, name, e, scope)),
                }
            }
            Expr::Index(obj, idx) => {
                let o = deref_val(self.eval(obj, scope)?);
                let i = self.eval(idx, scope)?;
                let key = PathEl::Index(i);
                match get_one(&o, &key, self.line) {
                    Ok(v) => v,
                    Err(err) => {
                        let PathEl::Index(i) = key else { unreachable!() };
                        return Err(self.explain_index(err, obj, idx, &o, &i, scope));
                    }
                }
            }
            Expr::Call(callee, args) => self.call(callee, args, scope, e)?,
            Expr::Unary(op, x) => {
                let v = self.eval(x, scope)?;
                match *op {
                    "not" => Value::Bool(!v.truthy()),
                    "~" => match as_int(v.as_num(self.line)?) {
                        Some(i) => Value::Num(!i as f64),
                        None => return self.rt("'~' needs a whole number"),
                    },
                    _ => match &v {
                        Value::List(l) if is_vec(l) => Value::list(l.iter().map(|p| Value::Num(-num_of(p))).collect()),
                        _ => Value::Num(-v.as_num(self.line)?),
                    },
                }
            }
            // `a and b` / `a or b` give back one of the two values, so  name = saved or "Guest"  works
            Expr::Binary(Op::And, l, r) => {
                let a = self.eval(l, scope)?;
                if !a.truthy() {
                    return Ok(a);
                }
                self.eval(r, scope)?
            }
            Expr::Binary(Op::Or, l, r) => {
                let a = self.eval(l, scope)?;
                if a.truthy() {
                    return Ok(a);
                }
                self.eval(r, scope)?
            }
            Expr::Binary(Op::In, l, r) => {
                let x = self.eval(l, scope)?;
                if let Expr::Range(a, b) = &**r {
                    // hp in 1 to 50: between the two, both included (any number, not just whole ones)
                    let (a, b) = (self.eval(a, scope)?, self.eval(b, scope)?);
                    let (Value::Num(a), Value::Num(b), Value::Num(n)) = (&a, &b, &x) else {
                        return self.rt(format!("'in ... to ...' needs numbers, but this checks {} in {} to {}", x.type_name(), a.type_name(), b.type_name()));
                    };
                    return Ok(Value::Bool(*n >= a.min(*b) && *n <= a.max(*b)));
                }
                let within = deref_val(self.eval(r, scope)?);
                Value::Bool(match &within {
                    Value::List(items) => items.iter().any(|v| equals(v, &x)),
                    Value::Str(s) => match &x {
                        Value::Str(part) => s.contains(part.as_str()),
                        other => return self.rt(format!("'in' on text looks for text inside it, but this is {}", other.type_name())),
                    },
                    Value::Obj(o) if o.type_name == "dict" => o.get(&key_str(&x)).is_some(),
                    Value::Obj(o) if o.type_name == "stack" || o.type_name == "queue" => {
                        matches!(o.get("items"), Some(Value::List(items)) if items.iter().any(|v| equals(v, &x)))
                    }
                    other => {
                        return self.rt(format!(
                            "'in' looks inside a list, text, dictionary or a range like 1 to 10, but {} is {}",
                            crate::diagnose::code(r),
                            other.type_name()
                        ))
                    }
                })
            }
            Expr::Range(a, b) => {
                let (a, b) = (self.eval(a, scope)?, self.eval(b, scope)?);
                let (Value::Num(a), Value::Num(b)) = (&a, &b) else {
                    return self.rt(format!("'to' makes a range of numbers, like 1 to 10, but got {} and {}", a.type_name(), b.type_name()));
                };
                let (a, b) = (*a, *b);
                let count = (b - a).abs().floor() as usize + 1;
                if count > 10_000_000 {
                    return self.rt(format!("{} to {} would be {} numbers - that's too many", fmt_num(a), fmt_num(b), count));
                }
                let step = if b >= a { 1.0 } else { -1.0 };
                Value::list((0..count).map(|i| Value::Num(a + step * i as f64)).collect())
            }
            Expr::Binary(op, l, r) => {
                let a = self.eval(l, scope)?;
                let b = self.eval(r, scope)?;
                // the most common case by far: plain numbers
                if let (Value::Num(x), Value::Num(y)) = (&a, &b) {
                    let (x, y) = (*x, *y);
                    let quick = match op {
                        Op::Add => Some(Value::Num(x + y)),
                        Op::Sub => Some(Value::Num(x - y)),
                        Op::Mul => Some(Value::Num(x * y)),
                        Op::Div if y != 0.0 => Some(Value::Num(x / y)),
                        Op::Rem if y != 0.0 => Some(Value::Num(rem(x, y))),
                        Op::Lt => Some(Value::Bool(x < y)),
                        Op::Gt => Some(Value::Bool(x > y)),
                        Op::Le => Some(Value::Bool(x <= y)),
                        Op::Ge => Some(Value::Bool(x >= y)),
                        Op::Eq => Some(Value::Bool(x == y)),
                        Op::Ne => Some(Value::Bool(x != y)),
                        _ => None,
                    };
                    if let Some(v) = quick {
                        return Ok(v);
                    }
                }
                match self.binop(op.as_str(), &a, &b) {
                    Ok(v) => v,
                    Err(err) => return Err(self.explain_binop(err, *op, l, r, &a, &b, e, scope)),
                }
            }
            Expr::Lambda(d) => Value::Func(Rc::new(Func { def: d.clone(), closure: scope.clone(), file: self.cur_file })),
            Expr::Spawn { prefab, at, props, into } => self.spawn(prefab, at, props, into, scope)?,
            Expr::Dict(items) => {
                let mut o = Obj::new("dict");
                for (k, e) in items {
                    let v = self.eval(e, scope)?;
                    o.set(k, v);
                }
                Value::obj(o)
            }
            Expr::Load(p) => {
                let path = self.eval(p, scope)?.display();
                self.load(&path)?
            }
            Expr::Pop(target) => self.mutate(target, scope, |it, slot| {
                // stacks and lists give the newest item, queues the oldest
                let from_front = matches!(slot, Value::Obj(o) if o.type_name == "queue");
                let list: &mut Rc<Vec<Value>> = match slot {
                    Value::List(l) => l,
                    Value::Obj(o) if o.type_name == "stack" || o.type_name == "queue" => {
                        let ob = Rc::make_mut(o);
                        match ob.fields.iter().position(|(k, x)| k == "items" && matches!(x, Value::List(_))) {
                            Some(i) => match &mut ob.fields[i].1 {
                                Value::List(l) => l,
                                _ => unreachable!(),
                            },
                            None => return it.rt("there's nothing to pop - it's empty"),
                        }
                    }
                    other => return it.rt(format!("pop needs a list, stack or queue, but this is {}", other.type_name())),
                };
                if list.is_empty() {
                    return it.rt("there's nothing to pop - it's empty");
                }
                let items = Rc::make_mut(list);
                Ok(if from_front { items.remove(0) } else { items.pop().unwrap() })
            })?,
        })
    }

    fn binop(&self, op: &str, a: &Value, b: &Value) -> R<Value> {
        use Value::*;
        Ok(match (op, a, b) {
            ("+", Num(x), Num(y)) => Num(x + y),
            ("+", Str(x), _) => Str(format!("{}{}", x, b.display())),
            ("+", _, Str(y)) => Str(format!("{}{}", a.display(), y)),
            ("+" | "-", List(x), List(y)) if is_vec(x) && is_vec(y) && x.len() == y.len() => {
                let sign = if op == "+" { 1.0 } else { -1.0 };
                Value::list(x.iter().zip(y.iter()).map(|(p, q)| Num(num_of(p) + sign * num_of(q))).collect())
            }
            ("*", List(x), Num(n)) | ("*", Num(n), List(x)) if is_vec(x) => {
                Value::list(x.iter().map(|p| Num(num_of(p) * n)).collect())
            }
            ("/", List(x), Num(n)) if is_vec(x) => {
                if *n == 0.0 {
                    return self.rt("can't divide by zero");
                }
                Value::list(x.iter().map(|p| Num(num_of(p) / n)).collect())
            }
            ("+", List(x), List(y)) => {
                let mut v = (**x).clone();
                v.extend(y.iter().cloned());
                Value::list(v)
            }
            ("-", Num(x), Num(y)) => Num(x - y),
            ("*", Num(x), Num(y)) => Num(x * y),
            ("*", Str(s), Num(n)) => Str(s.repeat(n.max(0.0) as usize)),
            ("/", Num(_), Num(y)) if *y == 0.0 => return self.rt("can't divide by zero"),
            ("/", Num(x), Num(y)) => Num(x / y),
            ("%", Num(_), Num(y)) if *y == 0.0 => return self.rt("can't divide by zero"),
            ("%", Num(x), Num(y)) => Num(rem(*x, *y)),
            ("&" | "|" | "^" | "<<" | ">>", Num(x), Num(y)) => {
                let (Some(p), Some(q)) = (as_int(*x), as_int(*y)) else {
                    return self.rt(format!("'{}' needs whole numbers", op));
                };
                if (op == "<<" || op == ">>") && !(0..64).contains(&q) {
                    return self.rt("a shift amount must be between 0 and 63");
                }
                Num(match op {
                    "&" => p & q,
                    "|" => p | q,
                    "^" => p ^ q,
                    "<<" => p << q,
                    _ => p >> q,
                } as f64)
            }
            ("==", _, _) => Bool(equals(a, b)),
            ("!=", _, _) => Bool(!equals(a, b)),
            ("<" | ">" | "<=" | ">=", _, _) => {
                let Some(o) = cmp_values(a, b) else {
                    return self.rt(format!("can't compare {} and {}", a.type_name(), b.type_name()));
                };
                Bool(match op {
                    "<" => o.is_lt(),
                    ">" => o.is_gt(),
                    "<=" => o.is_le(),
                    _ => o.is_ge(),
                })
            }
            _ => return self.rt(format!("can't use '{}' on {} and {}", op, a.type_name(), b.type_name())),
        })
    }

    fn call(&mut self, callee: &Expr, args: &[Arg], scope: &Rc<Scope>, whole: &Expr) -> R<Value> {
        let line = self.line;
        let r = self.call_inner(callee, args, scope);
        match r {
            Err(err) if !err.explained && !err.is_switch() && err.line == line && err.trace.is_empty() => {
                self.line = line;
                match callee {
                    // thing.name(...): a method that isn't there
                    Expr::Field(obj, name) if err.msg.contains("has no method") || err.msg.contains("has no property") => {
                        Err(self.explain_field(err, obj, name, whole, scope))
                    }
                    _ => Err(self.explain_call(err, callee, whole, scope)),
                }
            }
            other => other,
        }
    }

    fn call_inner(&mut self, callee: &Expr, args: &[Arg], scope: &Rc<Scope>) -> R<Value> {
        let (mut vals, mut named, mut aliases) = (Vec::with_capacity(args.len()), vec![], vec![]);
        for a in args {
            let v = self.eval(&a.value, scope)?;
            match &a.name {
                Some(n) => named.push((n.clone(), v)),
                None => {
                    // only objects and lists are passed by reference, so only they need their variable
                    if let (Expr::Ident(n), Value::Obj(_) | Value::List(_)) = (&a.value, &v) {
                        aliases.resize(vals.len(), None);
                        aliases.push(scope.lookup(n));
                    }
                    vals.push(v);
                }
            }
        }
        if let Expr::Field(obj, name) = callee {
            // super.take_damage(5): the parent type's version, on the same self
            if let Expr::Ident(s) = &**obj {
                if s == "super" {
                    if let (Some(Value::Data(parent)), Some(me)) = (scope.value_of("super"), scope.lookup("self")) {
                        let Some((f, owner)) = parent.method(name) else {
                            return self.rt(format!("{} has no function '{}'", parent.name, name));
                        };
                        return self.call_func(&f, vals, named, aliases, Some((me, owner)));
                    }
                }
            }
            let o = deref_val(self.eval(obj, scope)?);
            if let Value::Obj(ob) = &o {
                if let Some(f @ (Value::Func(_) | Value::Native(_) | Value::Data(_))) = ob.get(name) {
                    return self.call_value(f.clone(), vals, named, aliases);
                }
                if let Some((f, owner)) = ob.class.as_ref().and_then(|c| c.method(name)) {
                    return self.call_method(&f, owner, o.clone(), Some(obj), scope, vals, named);
                }
            }
            if let Value::Module(m) = &o {
                let f = self.module_get(m, name)?;
                return self.call_value(f, vals, named, aliases);
            }
            // list.add(x) / list.remove(x) / dict.remove(key) change the list or dictionary they're called on
            let changes_it = match (&o, name.as_str()) {
                (Value::List(_), "add" | "remove") => true,
                (Value::Obj(ob), "remove") => ob.type_name == "dict",
                _ => false,
            };
            let r = methods::call(self, o, name, vals, true)?;
            if changes_it && root_name_of(obj).is_some() {
                self.store_back(obj, scope, r.clone())?;
            }
            return Ok(r);
        }
        let f = self.eval(callee, scope)?;
        self.call_value(f, vals, named, aliases)
    }

    /// Makes the type declared by `data` / `class`, with its parent's fields and methods underneath.
    fn make_type(&mut self, decl: &DataDecl, scope: &Rc<Scope>) -> R<Value> {
        let parent = match &decl.parent {
            None => None,
            Some(p) => match scope.value_of(p) {
                Some(Value::Data(d)) => Some(d),
                Some(other) => return self.rt(format!("'{}' isn't a data type (it's {}), so {} can't be built from it", p, other.type_name(), decl.name)),
                None => return self.rt(self.did_you_mean(format!("there's no data type called '{}' to build {} from.", p, decl.name), p, scope)),
            },
        };
        let mut proto = Obj::new(&decl.name);
        if let Some(p) = &parent {
            proto.fields = p.fields.clone();
        }
        for (k, e) in &decl.fields {
            let v = self.eval(e, scope)?;
            proto.set(k, v);
        }
        let methods = decl.methods.iter().map(|d| Rc::new(Func { def: d.clone(), closure: scope.clone(), file: self.cur_file })).collect();
        Ok(Value::Data(Rc::new(DataDef { name: decl.name.clone(), fields: proto.fields, methods, parent })))
    }

    /// `goblin.take_damage(5)`: runs the method with `self` set to the object, then stores the
    /// changed object back where it came from (goblin, list[2], player.pet, ...).
    #[allow(clippy::too_many_arguments)]
    fn call_method(
        &mut self,
        f: &Rc<Func>,
        owner: Rc<DataDef>,
        me: Value,
        receiver: Option<&Expr>,
        scope: &Rc<Scope>,
        vals: Vec<Value>,
        named: Vec<(String, Value)>,
    ) -> R<Value> {
        let me_var = self.new_var(me.clone());
        if let Some(src) = receiver.and_then(|e| self.place_of(e, scope).map(|(var, path, _)| (var, path, crate::diagnose::code(e)))) {
            me_var.borrow_mut().source = Some((Rc::new(Source { var: src.0, path: src.1, name: src.2 }), None));
        }
        let result = self.call_func(f, vals, named, vec![], Some((me_var.clone(), owner)))?;
        let after = me_var.borrow().get().clone();
        let changed = match (&me, &after) {
            (Value::Obj(a), Value::Obj(b)) => !Rc::ptr_eq(a, b),
            _ => true,
        };
        if changed {
            if let Some(e) = receiver {
                self.store_back(e, scope, after)?;
            }
        }
        Ok(result)
    }

    /// Puts a new value at a place like `goblin` or `enemies[2].pet` (a normal, rewindable change).
    /// Places that can't be changed, like the result of a function call, are left alone.
    fn store_back(&mut self, e: &Expr, scope: &Rc<Scope>, v: Value) -> R<()> {
        if root_name_of(e).is_none() {
            return Ok(());
        }
        let (name, path) = self.lvalue(e, scope)?;
        let Some(var) = scope.lookup(name) else { return Ok(()) };
        let var = if path.is_empty() { var } else { deref_var(var) };
        self.module_guard(&var, name, &path)?;
        if let Some(g) = &self.guard {
            if !Rc::ptr_eq(g, &var) && !var.borrow().sandbox {
                return Err(EzaError::syntax(self.line, format!("Cannot modify global variable '{}' inside an isolated simulation block.", name)));
            }
        }
        let root = var.borrow().get().clone();
        let new = set_path(root, &path, v, self.line)?;
        self.commit(&var, new);
        Ok(())
    }

    pub fn call_value(
        &mut self,
        f: Value,
        vals: Vec<Value>,
        named: Vec<(String, Value)>,
        aliases: Vec<Option<VarRef>>,
    ) -> R<Value> {
        match f {
            Value::Func(func) => self.call_func(&func, vals, named, aliases, None),
            Value::Data(d) => {
                let mut o = Obj::new(&d.name);
                o.fields = d.fields.clone();
                o.class = Some(d.clone());
                if vals.len() > o.fields.len() {
                    return self.rt(format!("{} only has {} fields", d.name, o.fields.len()));
                }
                for (i, v) in vals.into_iter().enumerate() {
                    o.fields[i].1 = v;
                }
                for (n, v) in named {
                    if o.get(&n).is_none() {
                        return self.rt(format!("{} has no field '{}'", d.name, n));
                    }
                    o.set(&n, v);
                }
                let made = Value::obj(o);
                // `define setup` runs on every new object, after its fields are filled in
                match d.method("setup") {
                    Some((setup, owner)) => {
                        let me = self.new_var(made);
                        self.call_func(&setup, vec![], vec![], vec![], Some((me.clone(), owner)))?;
                        let out = me.borrow().get().clone();
                        Ok(out)
                    }
                    None => Ok(made),
                }
            }
            Value::Native(n) => methods::builtin(self, &n, vals, named),
            other => self.rt(format!("{} is not a function", other.repr())),
        }
    }

    /// Runs a function. `me` is (the object's variable, the type the method belongs to) for methods,
    /// which then see `self` (and `super` when that type is built from another).
    fn call_func(
        &mut self,
        func: &Rc<Func>,
        vals: Vec<Value>,
        named: Vec<(String, Value)>,
        aliases: Vec<Option<VarRef>>,
        me: Option<(VarRef, Rc<DataDef>)>,
    ) -> R<Value> {
        let def = &func.def;
        if vals.len() > def.params.len() {
            return self.rt(format!("{} expects {} argument(s) but got {}", def.name, def.params.len(), vals.len()));
        }
        if self.depth >= MAX_DEPTH {
            return self.rt("too much recursion (a function keeps calling itself)");
        }
        let sc = Scope::new(Some(func.closure.clone()));
        if let Some((var, owner)) = me {
            sc.insert("self", var);
            if let Some(p) = &owner.parent {
                sc.insert("super", self.new_var(Value::Data(p.clone())));
            }
        }
        let mut vals = vals.into_iter();
        for (i, p) in def.param_names.iter().enumerate() {
            if let Some(v) = vals.next() {
                // objects and lists are passed by reference, so functions can change them
                match (aliases.get(i).cloned().flatten(), &v) {
                    (Some(var), Value::Obj(_) | Value::List(_)) => sc.insert_rc(p.clone(), var),
                    _ => sc.insert_rc(p.clone(), self.new_var(v)),
                }
            } else if let Some((_, v)) = named.iter().find(|(n, _)| **n == **p) {
                sc.insert_rc(p.clone(), self.new_var(v.clone()));
            } else if let Some(Some(d)) = def.defaults.get(i) {
                // its default value, worked out now (it can use the parameters before it)
                let v = self.eval(d, &sc)?;
                sc.insert_rc(p.clone(), self.new_var(v));
            } else {
                return self.rt(format!("{} is missing the argument '{}'", def.name, p));
            }
        }
        if def.has_wait {
            // a function that waits runs in the background; the caller carries on immediately
            let caller_file = std::mem::replace(&mut self.cur_file, func.file);
            let r = self.spawn_task(def.body.clone(), sc);
            self.cur_file = caller_file;
            r?;
            return Ok(Value::None);
        }
        let saved_line = self.line;
        let caller_file = std::mem::replace(&mut self.cur_file, func.file);
        self.depth += 1;
        let r = self.exec_block(&def.body, &sc);
        self.depth -= 1;
        self.cur_file = caller_file;
        let r = r.map_err(|mut e| {
            // errors inside the function belong to the file it was written in
            if e.file.is_empty() {
                e.file = self.file_name(func.file).to_string();
            }
            // stack trace: remember which function the error passed through
            if e.trace.len() < 50 {
                e.trace.push((def.name.clone(), saved_line));
            }
            e
        });
        self.line = saved_line;
        Ok(match r? {
            Flow::Return(v) => v,
            _ => Value::None,
        })
    }
}

/// A short way to show the object part of `x.method` in a message.
fn obj_text(e: &Expr) -> String {
    match e {
        Expr::Ident(n) => n.clone(),
        Expr::Field(inner, f) => format!("{}.{}", obj_text(inner), f),
        _ => "it".to_string(),
    }
}

/// The variable at the start of `a.b[c]`, if it is that kind of expression.
fn root_name_of(e: &Expr) -> Option<&str> {
    match e {
        Expr::Ident(n) => Some(n),
        Expr::Field(inner, _) | Expr::Index(inner, _) => root_name_of(inner),
        _ => None,
    }
}

/// Tilemap layout text: one row per line. Either one character per tile (`.` or space = empty,
/// 0-9 then a-z = tiles 0-35), or numbers separated by commas (-1 or . = empty).
fn parse_tile_layout(text: &str) -> Vec<Vec<i64>> {
    let mut rows = vec![];
    for line in text.lines() {
        let line = line.trim_end();
        if line.trim().is_empty() {
            continue;
        }
        let row: Vec<i64> = if line.contains(',') {
            line.split(',')
                .map(|t| t.trim().parse::<i64>().unwrap_or(-1))
                .collect()
        } else {
            line.chars()
                .map(|c| match c {
                    '0'..='9' => c as i64 - '0' as i64,
                    'a'..='z' => c as i64 - 'a' as i64 + 10,
                    'A'..='Z' => c as i64 - 'A' as i64 + 10,
                    _ => -1,
                })
                .collect()
        };
        rows.push(row);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn num(it: &Interp, name: &str) -> f64 {
        match it.global(name) {
            Some(Value::Num(n)) => n,
            _ => panic!("{} is not a number", name),
        }
    }

    #[test]
    fn time_travel_scrubs_the_whole_world() {
        let mut it = Interp::new(Path::new("test.eza"));
        it.frame_mode = true;
        it.run_source("a = 0\nb = 100\non scene.ticks\n    change a by 1\n    change b by -1\n", "test.eza").unwrap();
        for _ in 0..50 {
            it.tick().unwrap();
        }
        assert_eq!((num(&it, "a"), num(&it, "b")), (50.0, 50.0));
        assert_eq!(it.debug_cursor(), (50, 50));
        it.scrub(-20);
        assert_eq!((num(&it, "a"), num(&it, "b")), (30.0, 70.0));
        assert_eq!(it.debug_cursor(), (30, 50));
        it.scrub(5);
        assert_eq!(num(&it, "a"), 35.0);
        it.scrub(i64::MIN / 2);
        assert_eq!((num(&it, "a"), num(&it, "b")), (0.0, 100.0));
        it.scrub(i64::MAX / 2);
        assert_eq!(num(&it, "a"), 50.0);
        // resuming after scrubbing back erases the old future
        it.scrub(-10);
        it.tick().unwrap();
        assert_eq!(num(&it, "a"), 41.0);
        assert!(it.debug_watch(10).iter().any(|l| l.starts_with("* a = 41")));
    }
}

/// GUI elements whose label has {values} in it: (path of child indexes, the label).
fn live_label_paths(n: &DeclNode, path: Vec<usize>, out: &mut Vec<(Vec<usize>, Expr)>) {
    if let Some(l) = &n.label {
        if !matches!(l, Expr::Str(_)) {
            out.push((path.clone(), l.clone()));
        }
    }
    for (i, c) in n.children.iter().enumerate() {
        let mut p = path.clone();
        p.push(i);
        live_label_paths(c, p, out);
    }
}
