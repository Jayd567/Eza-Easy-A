use std::rc::Rc;

#[derive(Debug, Clone)]
pub enum Expr {
    Num(f64),
    Str(String),
    Bool(bool),
    None,
    Color(String),
    List(Vec<Expr>),
    Ident(String),
    Field(Box<Expr>, String),
    Index(Box<Expr>, Box<Expr>),
    Call(Box<Expr>, Vec<Arg>),
    Unary(&'static str, Box<Expr>),
    Binary(Op, Box<Expr>, Box<Expr>),
    Lambda(Rc<FuncDef>),
    /// spawn Goblin at 5,0,3 health=50 [into enemies]
    Spawn { prefab: Box<Expr>, at: Option<Box<Expr>>, props: Vec<(String, Expr)>, into: Option<Box<Expr>> },
    /// {theme: "dark", volume: 7}
    Dict(Vec<(String, Expr)>),
    /// load "file.json" / "picture.png" / "notes.txt"
    Load(Box<Expr>),
    /// pop stack (takes the top item; queues give the front item)
    Pop(Box<Expr>),
}

/// A two-sided operator, worked out once when the script is read (not compared as text each time).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

impl Op {
    pub fn from_str(s: &str) -> Option<Op> {
        Some(match s {
            "+" => Op::Add,
            "-" => Op::Sub,
            "*" => Op::Mul,
            "/" => Op::Div,
            "%" => Op::Rem,
            "==" => Op::Eq,
            "!=" => Op::Ne,
            "<" => Op::Lt,
            ">" => Op::Gt,
            "<=" => Op::Le,
            ">=" => Op::Ge,
            "and" => Op::And,
            "or" => Op::Or,
            "&" => Op::BitAnd,
            "|" => Op::BitOr,
            "^" => Op::BitXor,
            "<<" => Op::Shl,
            ">>" => Op::Shr,
            _ => return None,
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Op::Add => "+",
            Op::Sub => "-",
            Op::Mul => "*",
            Op::Div => "/",
            Op::Rem => "%",
            Op::Eq => "==",
            Op::Ne => "!=",
            Op::Lt => "<",
            Op::Gt => ">",
            Op::Le => "<=",
            Op::Ge => ">=",
            Op::And => "and",
            Op::Or => "or",
            Op::BitAnd => "&",
            Op::BitOr => "|",
            Op::BitXor => "^",
            Op::Shl => "<<",
            Op::Shr => ">>",
        }
    }
    pub fn is_comparison(self) -> bool {
        matches!(self, Op::Eq | Op::Ne | Op::Lt | Op::Gt | Op::Le | Op::Ge)
    }
}

#[derive(Debug, Clone)]
pub struct Arg {
    pub name: Option<String>,
    pub value: Expr,
}

#[derive(Debug)]
pub struct FuncDef {
    pub name: String,
    pub params: Vec<String>,
    pub body: Rc<Vec<Stmt>>,
    /// contains `wait`, so calling it starts a background task
    pub has_wait: bool,
    /// the parameter names, shared (so a call doesn't copy each name into its scope)
    pub param_names: Vec<Rc<str>>,
    /// the line of its `define` (0 if unknown)
    pub line: usize,
}

impl FuncDef {
    pub fn new(name: String, params: Vec<String>, body: Rc<Vec<Stmt>>) -> Self {
        let has_wait = block_has_wait(&body);
        let param_names = params.iter().map(|p| Rc::from(p.as_str())).collect();
        FuncDef { name, params, body, has_wait, param_names, line: 0 }
    }
    pub fn at(mut self, line: usize) -> Self {
        self.line = line;
        self
    }
}

/// True if `wait` appears directly in this block or inside its if/each/while bodies.
pub fn block_has_wait(b: &[Stmt]) -> bool {
    b.iter().any(|s| match &s.kind {
        StmtKind::Wait { .. } => true,
        StmtKind::If(arms, els) => {
            arms.iter().any(|(_, b)| block_has_wait(b)) || els.as_ref().map_or(false, |b| block_has_wait(b))
        }
        StmtKind::Each(_, _, b) | StmtKind::While(_, b) => block_has_wait(b),
        _ => false,
    })
}

/// `data Boss from Enemy` (or `class`): fields with defaults, plus functions that work on `self`.
#[derive(Debug)]
pub struct DataDecl {
    pub name: String,
    pub parent: Option<String>,
    pub fields: Vec<(String, Expr)>,
    pub methods: Vec<Rc<FuncDef>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChangeMode {
    To,
    By,
}

/// A line inside a `scene` or `gui` block: `kind "label" key=value ... [then handler]`
#[derive(Debug, Clone)]
pub struct DeclNode {
    pub kind: String,
    pub label: Option<Expr>,
    pub props: Vec<(String, Expr)>,
    pub children: Vec<DeclNode>,
    pub handler: Option<Rc<Vec<Stmt>>>,
    /// `on ...` lines inside a GUI element; kept for the engine (Bevy) phase.
    #[allow(dead_code)]
    pub events: Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub struct Stmt {
    pub line: usize,
    pub kind: StmtKind,
}

#[derive(Debug, Clone)]
pub enum StmtKind {
    Assign(String, Expr),
    Change(Expr, ChangeMode, Expr),
    Expr(Expr),
    If(Vec<(Expr, Vec<Stmt>)>, Option<Vec<Stmt>>),
    Each(String, Expr, Vec<Stmt>),
    While(Expr, Vec<Stmt>),
    Define(Rc<FuncDef>),
    Return(Vec<Expr>),
    /// target None = whole scene; steps None = to beginning
    Rewind { target: Option<String>, steps: Option<Expr> },
    Data(Rc<DataDecl>),
    Attempt(Vec<Stmt>, String, Vec<Stmt>),
    Include(String),
    Param(String, Expr),
    Scene(DeclNode),
    /// a 2D world: sprites, tilemaps and a camera
    Stage(DeclNode),
    /// style neon  (properties, then `on hover` / `on press` overrides)
    Style(String, Vec<(String, Expr)>, Vec<(String, Vec<(String, Expr)>)>),
    Gui(DeclNode),
    On(Expr, Rc<Vec<Stmt>>),
    /// on hero touches coin [as a, b] / on Bullet stops touching Enemy - once per pair, when it starts (or stops)
    OnTouch { a: Expr, b: Expr, names: Option<(String, String)>, start: bool, body: Rc<Vec<Stmt>> },
    /// on event "boss_dead" [as info]
    OnEvent { name: String, var: Option<String>, body: Rc<Vec<Stmt>> },
    /// trigger "boss_dead" [with value]
    Trigger(String, Option<Expr>),
    Persist { body: Vec<Stmt>, steps: Option<Expr>, until: Option<Expr> },
    Mimic(String, Expr, Rc<Vec<Stmt>>),
    Tick(Option<Expr>),
    Break,
    Continue,
    Tween { target: Expr, value: Expr, steps: Expr, ease: String },
    Prefab(String, DeclNode),
    Destroy(Expr),
    Wait { steps: Expr, seconds: bool },
    Save { value: Expr, path: Expr, append: bool },
    /// push 5 to stack
    Push(Expr, Expr),
    /// test "name" + indented body (runs only with `eza test`)
    Test(String, Vec<Stmt>),
    Expect(Expr),
    /// play "jump.wav" loop=true volume=0.5 speed=1
    Play(Expr, Vec<(String, Expr)>),
    /// stop "music.ogg"  (None = stop all)
    Stop(Option<Expr>),
    /// emit 30 from sparks [at position]
    Emit { count: Expr, from: Expr, at: Option<Expr> },
    /// go to "level2.eza" - switch to another script
    Go(Expr),
    /// use "enemies.eza" [as foes] - load a module; its names are reached as `foes.name`
    Use { path: String, alias: String },
}

impl Expr {
    /// Replaces every `name` with `with` (e.g. `button` -> `pause_menu.children[1]`).
    pub fn subst(&self, name: &str, with: &Expr) -> Expr {
        let b = |e: &Expr| Box::new(e.subst(name, with));
        match self {
            Expr::Ident(n) if n == name => with.clone(),
            Expr::List(v) => Expr::List(v.iter().map(|e| e.subst(name, with)).collect()),
            Expr::Field(e, f) => Expr::Field(b(e), f.clone()),
            Expr::Index(e, i) => Expr::Index(b(e), b(i)),
            Expr::Call(c, args) => Expr::Call(
                b(c),
                args.iter().map(|a| Arg { name: a.name.clone(), value: a.value.subst(name, with) }).collect(),
            ),
            Expr::Unary(op, e) => Expr::Unary(op, b(e)),
            Expr::Binary(op, l, r) => Expr::Binary(*op, b(l), b(r)),
            other => other.clone(),
        }
    }
}

pub fn subst_all(v: &[Stmt], name: &str, with: &Expr) -> Vec<Stmt> {
    v.iter().map(|s| s.subst(name, with)).collect()
}

impl Stmt {
    pub fn subst(&self, name: &str, with: &Expr) -> Stmt {
        let e = |x: &Expr| x.subst(name, with);
        let all = |v: &[Stmt]| subst_all(v, name, with);
        let kind = match &self.kind {
            StmtKind::Assign(n, x) => StmtKind::Assign(n.clone(), e(x)),
            StmtKind::Change(t, m, x) => StmtKind::Change(e(t), *m, e(x)),
            StmtKind::Expr(x) => StmtKind::Expr(e(x)),
            StmtKind::If(arms, els) => StmtKind::If(
                arms.iter().map(|(c, b)| (e(c), all(b))).collect(),
                els.as_ref().map(|b| all(b)),
            ),
            StmtKind::Each(v, x, b) => StmtKind::Each(v.clone(), e(x), all(b)),
            StmtKind::While(c, b) => StmtKind::While(e(c), all(b)),
            StmtKind::Return(xs) => StmtKind::Return(xs.iter().map(e).collect()),
            StmtKind::Attempt(a, n, h) => StmtKind::Attempt(all(a), n.clone(), all(h)),
            StmtKind::On(x, b) => StmtKind::On(e(x), Rc::new(all(b))),
            StmtKind::Persist { body, steps, until } => StmtKind::Persist {
                body: all(body),
                steps: steps.as_ref().map(e),
                until: until.as_ref().map(e),
            },
            StmtKind::Tween { target, value, steps, ease } => {
                StmtKind::Tween { target: e(target), value: e(value), steps: e(steps), ease: ease.clone() }
            }
            other => other.clone(),
        };
        Stmt { line: self.line, kind }
    }
}
