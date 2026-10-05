use crate::ast::{DeclNode, FuncDef};
use crate::error::{EzaError, R};
use crate::interp::{EntityHandle, Scope};
use crate::suggest;
use std::cmp::Ordering;
use std::rc::Rc;

#[derive(Clone)]
pub struct Obj {
    pub type_name: String,
    pub fields: Vec<(String, Value)>,
    /// the `data` type it was made from, which holds its methods
    pub class: Option<Rc<DataDef>>,
}

impl Obj {
    pub fn new(t: &str) -> Self {
        Obj { type_name: t.to_string(), fields: vec![], class: None }
    }
    pub fn get(&self, k: &str) -> Option<&Value> {
        self.fields.iter().find(|(n, _)| n == k).map(|(_, v)| v)
    }
    pub fn set(&mut self, k: &str, v: Value) {
        match self.fields.iter_mut().find(|(n, _)| n == k) {
            Some(slot) => slot.1 = v,
            None => self.fields.push((k.to_string(), v)),
        }
    }
}

pub struct Func {
    pub def: Rc<FuncDef>,
    pub closure: Rc<Scope>,
}

pub struct PrefabDef {
    pub name: String,
    pub node: DeclNode,
    pub scope: Rc<Scope>,
}

/// A loaded picture: RGBA pixels, row by row from the top-left.
pub struct ImageData {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub path: String,
}

/// A file loaded with `use`: its own top-level names, reached from outside as `module.name`.
pub struct Module {
    pub name: String,
    pub scope: Rc<Scope>,
}

impl Module {
    /// The names other files can use (names starting with _ stay private to the module).
    pub fn exports(&self) -> Vec<String> {
        let mut names: Vec<String> = self.scope.entries().into_iter().map(|(k, _)| k).filter(|k| !k.starts_with('_')).collect();
        names.sort();
        names
    }
}

pub struct DataDef {
    pub name: String,
    pub fields: Vec<(String, Value)>,
    pub methods: Vec<Rc<Func>>,
    /// `data Boss from Enemy`: Enemy
    pub parent: Option<Rc<DataDef>>,
}

impl DataDef {
    /// The method with this name, looking in the parent types too, and the type that defines it.
    pub fn method(self: &Rc<Self>, name: &str) -> Option<(Rc<Func>, Rc<DataDef>)> {
        let mut d = self;
        loop {
            if let Some(f) = d.methods.iter().find(|f| f.def.name == name) {
                return Some((f.clone(), d.clone()));
            }
            d = d.parent.as_ref()?;
        }
    }
    /// Is this type `name`, or built from it?
    pub fn is_a(&self, name: &str) -> bool {
        self.name == name || self.parent.as_ref().is_some_and(|p| p.is_a(name))
    }
}

#[derive(Clone)]
pub enum Value {
    None,
    Bool(bool),
    Num(f64),
    Str(String),
    List(Rc<Vec<Value>>),
    Obj(Rc<Obj>),
    /// r, g, b in 0..255, a in 0..1
    Color(Rc<[f64; 4]>),
    Func(Rc<Func>),
    Data(Rc<DataDef>),
    Native(String),
    /// an object made by `spawn`: a live reference, so changes through any copy are seen by the engine
    Entity(Rc<EntityHandle>),
    Prefab(Rc<PrefabDef>),
    Image(Rc<ImageData>),
    Module(Rc<Module>),
}

pub fn fmt_num(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        let s = format!("{:.10}", n);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

pub fn color_hex(c: [f64; 4]) -> String {
    let b = |x: f64| x.round().clamp(0.0, 255.0) as u8;
    let mut s = format!("#{:02X}{:02X}{:02X}", b(c[0]), b(c[1]), b(c[2]));
    if c[3] < 1.0 {
        s += &format!("{:02X}", b(c[3] * 255.0));
    }
    s
}

pub fn parse_hex(h: &str) -> [f64; 4] {
    let h: String = if h.len() <= 4 { h.chars().flat_map(|c| [c, c]).collect() } else { h.to_string() };
    let p = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(0) as f64;
    let a = if h.len() == 8 { p(6) / 255.0 } else { 1.0 };
    [p(0), p(2), p(4), a]
}

impl Value {
    pub fn obj(o: Obj) -> Value {
        Value::Obj(Rc::new(o))
    }
    pub fn list(v: Vec<Value>) -> Value {
        Value::List(Rc::new(v))
    }
    pub fn type_name(&self) -> String {
        match self {
            Value::None => "none".into(),
            Value::Bool(_) => "bool".into(),
            Value::Num(_) => "number".into(),
            Value::Str(_) => "text".into(),
            Value::List(_) => "list".into(),
            Value::Obj(o) => o.type_name.clone(),
            Value::Color(_) => "color".into(),
            Value::Func(_) | Value::Native(_) => "function".into(),
            Value::Entity(h) => match h.var.borrow().get() {
                Value::Obj(o) => o.type_name.clone(),
                _ => "entity".into(),
            },
            Value::Prefab(_) => "prefab".into(),
            Value::Image(_) => "image".into(),
            Value::Data(_) => "data".into(),
            Value::Module(_) => "module".into(),
        }
    }
    pub fn truthy(&self) -> bool {
        match self {
            Value::None => false,
            Value::Bool(b) => *b,
            Value::Num(n) => *n != 0.0,
            Value::Str(s) => !s.is_empty(),
            Value::List(l) => !l.is_empty(),
            _ => true,
        }
    }
    pub fn as_num(&self, line: usize) -> R<f64> {
        match self {
            Value::Num(n) => Ok(*n),
            v => Err(EzaError::runtime(line, format!("expected a number but got {} ({})", v.repr(), v.type_name()))),
        }
    }
    /// How `print` shows a value.
    pub fn display(&self) -> String {
        match self {
            Value::Str(s) => s.clone(),
            v => v.repr(),
        }
    }
    /// How a value looks inside a list or object (text gets quotes).
    pub fn repr(&self) -> String {
        if let Value::Obj(o) = self {
            match o.type_name.as_str() {
                "dict" => {
                    let key = |k: &str| {
                        if !k.is_empty() && k.chars().all(|c| c.is_alphanumeric() || c == '_') {
                            k.to_string()
                        } else {
                            format!("\"{}\"", k)
                        }
                    };
                    let body: Vec<String> = o.fields.iter().map(|(k, v)| format!("{}: {}", key(k), v.repr())).collect();
                    return format!("{{{}}}", body.join(", "));
                }
                "stack" | "queue" => {
                    let items = o.get("items").map(|v| v.repr()).unwrap_or_else(|| "[]".into());
                    return format!("{}{}", o.type_name, items);
                }
                "date" => return crate::tools::date_text(o),
                "database" => return format!("database({})", o.get("file").map(|f| f.display()).unwrap_or_default()),
                // what run(...) printed, so print(run("dir")) shows it
                "run_result" => return o.get("output").map(|f| f.display()).unwrap_or_default().trim_end().to_string(),
                _ => {}
            }
        }
        match self {
            Value::None => "none".into(),
            Value::Bool(b) => b.to_string(),
            Value::Num(n) => fmt_num(*n),
            Value::Str(s) => format!("\"{}\"", s),
            Value::List(l) => format!("[{}]", l.iter().map(|v| v.repr()).collect::<Vec<_>>().join(", ")),
            Value::Obj(o) => format!(
                "{}({})",
                o.type_name,
                o.fields.iter().map(|(k, v)| format!("{}: {}", k, v.repr())).collect::<Vec<_>>().join(", ")
            ),
            Value::Color(c) => color_hex(**c),
            Value::Func(f) => format!("<function {}>", f.def.name),
            Value::Native(n) => format!("<built-in {}>", n),
            Value::Data(d) => format!("<data {}>", d.name),
            Value::Entity(h) => match h.var.borrow().get() {
                Value::Obj(o) => match o.get("prefab") {
                    Some(Value::Str(p)) => format!("{}#{}", p, h.id),
                    _ => format!("{}#{}", o.type_name, h.id),
                },
                _ => format!("entity#{}", h.id),
            },
            Value::Prefab(p) => format!("<prefab {}>", p.name),
            Value::Image(i) => format!("<image {}x{} {}>", i.width, i.height, i.path),
            Value::Module(m) => format!("<module {}: {}>", m.name, m.exports().join(", ")),
        }
    }
}

pub fn equals(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::None, Value::None) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Num(x), Value::Num(y)) => x == y,
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Color(x), Value::Color(y)) => x == y,
        (Value::List(x), Value::List(y)) => {
            Rc::ptr_eq(x, y) || (x.len() == y.len() && x.iter().zip(y.iter()).all(|(p, q)| equals(p, q)))
        }
        (Value::Obj(x), Value::Obj(y)) if Rc::ptr_eq(x, y) => true,
        (Value::Obj(x), Value::Obj(y)) => {
            x.type_name == y.type_name
                && x.fields.len() == y.fields.len()
                && x.fields.iter().all(|(k, v)| y.get(k).map_or(false, |w| equals(v, w)))
        }
        (Value::Entity(x), Value::Entity(y)) => x.id == y.id,
        (Value::Image(x), Value::Image(y)) => Rc::ptr_eq(x, y) || (x.path == y.path && x.rgba == y.rgba),
        (Value::Func(x), Value::Func(y)) => Rc::ptr_eq(x, y),
        (Value::Native(x), Value::Native(y)) => x == y,
        (Value::Module(x), Value::Module(y)) => Rc::ptr_eq(x, y),
        _ => false,
    }
}

pub fn cmp_values(a: &Value, b: &Value) -> Option<Ordering> {
    match (a, b) {
        (Value::Num(x), Value::Num(y)) => x.partial_cmp(y),
        (Value::Str(x), Value::Str(y)) => Some(x.cmp(y)),
        // dates compare by when they are
        (Value::Obj(x), Value::Obj(y)) if x.type_name == "date" && y.type_name == "date" => match (x.get("timestamp"), y.get("timestamp")) {
            (Some(Value::Num(p)), Some(Value::Num(q))) => p.partial_cmp(q),
            _ => None,
        },
        _ => None,
    }
}

// ---------- paths: player.position.x / list[2] ----------

#[derive(Clone)]
pub enum PathEl {
    Field(String),
    Index(Value),
}

pub fn path_eq(a: &[PathEl], b: &[PathEl]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(p, q)| match (p, q) {
            (PathEl::Field(x), PathEl::Field(y)) => x == y,
            (PathEl::Index(x), PathEl::Index(y)) => equals(x, y),
            _ => false,
        })
}

/// `.x .y .z .w` work on lists, so positions like 0,0,0 read naturally.
pub fn axis(f: &str) -> Option<usize> {
    match f {
        "x" => Some(0),
        "y" => Some(1),
        "z" => Some(2),
        "w" => Some(3),
        _ => None,
    }
}

fn list_index(len: usize, i: f64, line: usize) -> R<usize> {
    let idx = if i < 0.0 { len as f64 + i } else { i };
    if idx < 0.0 || idx >= len as f64 || idx.fract() != 0.0 {
        return Err(EzaError::runtime(line, format!("index {} is out of range (length is {})", fmt_num(i), len)));
    }
    Ok(idx as usize)
}

pub fn get_one(v: &Value, p: &PathEl, line: usize) -> R<Value> {
    match (v, p) {
        (Value::Obj(o), PathEl::Index(k)) => {
            let key = key_str(k);
            o.get(&key).cloned().ok_or_else(|| {
                let keys: Vec<String> = o.fields.iter().map(|(k, _)| k.clone()).collect();
                let hint = suggest::closest(&key, &keys).map(|c| format!(" - did you mean \"{}\"?", c)).unwrap_or_default();
                EzaError::runtime(line, format!("there's no key \"{}\"{}", key, hint))
            })
        }
        (Value::Obj(o), PathEl::Field(f)) => o
            .get(f)
            .cloned()
            .ok_or_else(|| {
                let names: Vec<String> = o.fields.iter().map(|(k, _)| k.clone()).collect();
                let hint = suggest::closest(f, &names).map(|c| format!(" - did you mean '{}'?", c)).unwrap_or_default();
                EzaError::runtime(line, format!("{} has no property '{}'{}", o.type_name, f, hint))
            }),
        (Value::List(l), PathEl::Field(f)) if axis(f).is_some() => {
            Ok(l[list_index(l.len(), axis(f).unwrap() as f64, line)?].clone())
        }
        (Value::List(l), PathEl::Index(Value::Num(i))) => Ok(l[list_index(l.len(), *i, line)?].clone()),
        (Value::Str(s), PathEl::Index(Value::Num(i))) => {
            let chars: Vec<char> = s.chars().collect();
            Ok(Value::Str(chars[list_index(chars.len(), *i, line)?].to_string()))
        }
        (v, PathEl::Field(f)) => Err(EzaError::runtime(line, format!("{} has no property '{}'", v.type_name(), f))),
        (v, PathEl::Index(_)) => Err(EzaError::runtime(line, format!("can't use [ ] on {}", v.type_name()))),
    }
}

pub fn get_path(v: &Value, path: &[PathEl], line: usize) -> R<Value> {
    let mut cur = v.clone();
    for p in path {
        cur = get_one(&cur, p, line)?;
    }
    Ok(cur)
}

/// Returns a copy of `root` with the property at the end of `path` deleted.
pub fn remove_path(root: Value, path: &[PathEl], line: usize) -> R<Value> {
    let Some((PathEl::Field(f), parent_path)) = path.split_last() else { return Ok(root) };
    match get_path(&root, parent_path, line)? {
        Value::Obj(mut o) => {
            Rc::make_mut(&mut o).fields.retain(|(k, _)| k != f);
            set_path(root, parent_path, Value::Obj(o), line)
        }
        _ => Ok(root),
    }
}

/// Returns a copy of `root` with the value at `path` replaced (copy-on-write, so history snapshots stay intact).
pub fn set_path(root: Value, path: &[PathEl], new: Value, line: usize) -> R<Value> {
    if path.is_empty() {
        return Ok(new);
    }
    match (root, &path[0]) {
        (Value::Obj(mut o), PathEl::Field(f)) => {
            let child = o.get(f).cloned().unwrap_or(Value::None);
            let nc = set_path(child, &path[1..], new, line)?;
            Rc::make_mut(&mut o).set(f, nc);
            Ok(Value::Obj(o))
        }
        (Value::Obj(mut o), PathEl::Index(k)) => {
            let key = key_str(k);
            let child = o.get(&key).cloned().unwrap_or(Value::None);
            let nc = set_path(child, &path[1..], new, line)?;
            Rc::make_mut(&mut o).set(&key, nc);
            Ok(Value::Obj(o))
        }
        (Value::List(mut l), el) => {
            let i = match el {
                PathEl::Field(f) if axis(f).is_some() => axis(f).unwrap() as f64,
                PathEl::Index(Value::Num(i)) => *i,
                _ => return Err(EzaError::runtime(line, "lists can only be changed by [index] or .x/.y/.z")),
            };
            let idx = list_index(l.len(), i, line)?;
            let nc = set_path(l[idx].clone(), &path[1..], new, line)?;
            Rc::make_mut(&mut l)[idx] = nc;
            Ok(Value::List(l))
        }
        (v, _) => Err(EzaError::runtime(line, format!("can't change a property of {}", v.type_name()))),
    }
}

/// Dictionary keys are text; numbers like d[1] use "1".
pub fn key_str(k: &Value) -> String {
    match k {
        Value::Str(s) => s.clone(),
        other => other.display(),
    }
}

/// A mutable reference to the value at `path`. Containers along the way are copied only if
/// something else still shares them, so a list nobody else holds is changed in place.
pub fn path_mut<'a>(v: &'a mut Value, path: &[PathEl], line: usize) -> R<&'a mut Value> {
    let Some((first, rest)) = path.split_first() else { return Ok(v) };
    let child: &mut Value = match (v, first) {
        (Value::Obj(o), el @ (PathEl::Field(_) | PathEl::Index(_))) => {
            let key = match el {
                PathEl::Field(f) => f.clone(),
                PathEl::Index(k) => key_str(k),
            };
            let ob = Rc::make_mut(o);
            let Some(i) = ob.fields.iter().position(|(k, _)| *k == key) else {
                let names: Vec<String> = ob.fields.iter().map(|(k, _)| k.clone()).collect();
                let hint = suggest::closest(&key, &names).map(|c| format!(" - did you mean '{}'?", c)).unwrap_or_default();
                return Err(EzaError::runtime(line, format!("{} has no property '{}'{}", ob.type_name, key, hint)));
            };
            &mut ob.fields[i].1
        }
        (Value::List(l), el) => {
            let i = match el {
                PathEl::Field(f) if axis(f).is_some() => axis(f).unwrap() as f64,
                PathEl::Index(Value::Num(i)) => *i,
                _ => return Err(EzaError::runtime(line, "lists can only be changed by [index] or .x/.y/.z")),
            };
            let items = Rc::make_mut(l);
            let idx = list_index(items.len(), i, line)?;
            &mut items[idx]
        }
        (other, _) => return Err(EzaError::runtime(line, format!("can't change a property of {}", other.type_name()))),
    };
    path_mut(child, rest, line)
}
