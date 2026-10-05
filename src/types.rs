//! The type pass of `eza check`: works out what kind of value each name holds (a number, text,
//! a list, an Enemy...) from how the program makes and changes it, then finds code that is sure
//! to fail when it runs: `score - "5"`, `name.uper()`, `goblin.nmae`, `Enemy(hp=5, speeed=2)`,
//! `5()`, `count[0]`. Nobody writes types: a name whose kind can't be known for sure is skipped,
//! so this never complains about code that works.
use crate::ast::*;
use crate::error::{EzaError, Target};
use crate::interp::Interp;
use crate::value::Value;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

#[derive(Clone, PartialEq, Debug)]
enum Ty {
    Num,
    Str,
    Bool,
    List,
    Dict,
    Color,
    Nothing,
    Func,
    /// a data type itself (`Enemy`)
    Type(String),
    /// an object made from a data type (`Enemy(...)`)
    Obj(String),
    Prefab,
    Unknown,
}

impl Ty {
    fn known(&self) -> bool {
        *self != Ty::Unknown
    }
    fn describe(&self) -> String {
        match self {
            Ty::Num => "a number".into(),
            Ty::Str => "text".into(),
            Ty::Bool => "true/false".into(),
            Ty::List => "a list".into(),
            Ty::Dict => "a dictionary".into(),
            Ty::Color => "a color".into(),
            Ty::Nothing => "none".into(),
            Ty::Func => "a function".into(),
            Ty::Type(t) => format!("the type {}", t),
            Ty::Obj(t) => format!("{} {}", if t.starts_with(['A', 'E', 'I', 'O', 'U']) { "an" } else { "a" }, t),
            Ty::Prefab => "a prefab".into(),
            Ty::Unknown => "something".into(),
        }
    }
}

fn join(a: Option<Ty>, b: Option<Ty>) -> Option<Ty> {
    match (a, b) {
        (None, x) | (x, None) => x,
        (Some(x), Some(y)) if x == y => Some(x),
        _ => Some(Ty::Unknown),
    }
}

/// How a name gets its value somewhere in the program.
enum Ev<'a> {
    Is(&'a Expr),
    /// `change x by e`
    By(&'a Expr),
    /// `each x in e`
    Item(&'a Expr),
    Fixed(Ty),
}

#[derive(Default)]
struct Facts<'a> {
    evidence: HashMap<String, Vec<(Ev<'a>, usize, String)>>,
    datas: HashMap<String, Vec<Rc<DataDecl>>>,
    funcs: HashMap<String, Vec<Rc<FuncDef>>>,
    /// fields written anywhere with `change something.f ...` (objects can gain fields)
    written: HashSet<String>,
}

/// The blocks directly inside a statement.
fn blocks(s: &Stmt) -> Vec<&[Stmt]> {
    match &s.kind {
        StmtKind::If(arms, els) => arms.iter().map(|(_, b)| b.as_slice()).chain(els.iter().map(|b| b.as_slice())).collect(),
        StmtKind::Each(_, _, b) | StmtKind::While(_, b) | StmtKind::Test(_, b) => vec![b],
        StmtKind::Attempt(a, _, h) => vec![a, h],
        StmtKind::Define(d) => vec![&d.body],
        StmtKind::Data(d) => d.methods.iter().map(|m| m.body.as_slice()).collect(),
        StmtKind::On(_, b) | StmtKind::Mimic(_, _, b) => vec![b],
        StmtKind::OnTouch { body, .. } | StmtKind::OnEvent { body, .. } => vec![body],
        StmtKind::Persist { body, .. } => vec![body],
        StmtKind::Serve { pages, .. } => pages.iter().map(|p| p.body.as_slice()).collect(),
        _ => vec![],
    }
}

fn decl_blocks<'a>(n: &'a DeclNode, out: &mut Vec<&'a [Stmt]>) {
    if let Some(h) = &n.handler {
        out.push(h);
    }
    out.push(&n.events);
    n.children.iter().for_each(|c| decl_blocks(c, out));
}

fn decl_names(n: &DeclNode, out: &mut Vec<String>) {
    out.push(n.kind.clone());
    for (k, e) in &n.props {
        if k == "name" {
            if let Expr::Str(s) = e {
                out.push(s.clone());
            }
        }
    }
    if let Some(Expr::Str(l)) = &n.label {
        out.push(l.replace(|c: char| !c.is_alphanumeric(), "_"));
    }
    n.children.iter().for_each(|c| decl_names(c, out));
}

/// Lambdas inside an expression (their bodies are code too).
fn lambdas<'a>(e: &'a Expr, out: &mut Vec<&'a Rc<FuncDef>>) {
    match e {
        Expr::Lambda(d) => out.push(d),
        Expr::Field(o, _) | Expr::Unary(_, o) | Expr::Load(o) | Expr::Pop(o) => lambdas(o, out),
        Expr::Index(a, b) | Expr::Binary(_, a, b) => {
            lambdas(a, out);
            lambdas(b, out);
        }
        Expr::Call(c, args) => {
            lambdas(c, out);
            args.iter().for_each(|a| lambdas(&a.value, out));
        }
        Expr::List(v) => v.iter().for_each(|x| lambdas(x, out)),
        Expr::Dict(v) => v.iter().for_each(|(_, x)| lambdas(x, out)),
        Expr::Spawn { at, props, .. } => {
            at.iter().for_each(|x| lambdas(x, out));
            props.iter().for_each(|(_, x)| lambdas(x, out));
        }
        _ => {}
    }
}

/// The expressions a statement evaluates itself (not those in its blocks).
fn exprs(s: &Stmt) -> Vec<&Expr> {
    match &s.kind {
        StmtKind::Assign(_, e) | StmtKind::Expr(e) | StmtKind::Destroy(e) | StmtKind::Expect(e) | StmtKind::Param(_, e) | StmtKind::Go(e) => vec![e],
        StmtKind::Unpack { value, .. } => vec![value],
        StmtKind::Change(a, _, b) | StmtKind::Push(a, b) => vec![a, b],
        StmtKind::If(arms, _) => arms.iter().map(|(c, _)| c).collect(),
        StmtKind::Each(_, e, _) | StmtKind::While(e, _) | StmtKind::On(e, _) | StmtKind::Mimic(_, e, _) => vec![e],
        StmtKind::OnTouch { a, b, .. } => vec![a, b],
        StmtKind::Trigger(_, v) | StmtKind::Tick(v) | StmtKind::Stop(v) => v.iter().collect(),
        StmtKind::Return(v) => v.iter().collect(),
        StmtKind::Rewind { steps, .. } => steps.iter().collect(),
        StmtKind::Data(d) => d.fields.iter().map(|(_, e)| e).collect(),
        StmtKind::Persist { steps, until, .. } => steps.iter().chain(until.iter()).collect(),
        StmtKind::Tween { target, value, steps, .. } => vec![target, value, steps],
        StmtKind::Wait { steps, .. } => vec![steps],
        StmtKind::Save { value, path, .. } => vec![value, path],
        StmtKind::Play(p, props) => std::iter::once(p).chain(props.iter().map(|(_, e)| e)).collect(),
        StmtKind::Emit { count, from, at } => std::iter::once(count).chain(std::iter::once(from)).chain(at.iter()).collect(),
        _ => vec![],
    }
}

/// Every statement in a block, nested ones included (function bodies, `on` blocks, scene handlers, lambdas).
fn all_stmts<'a>(stmts: &'a [Stmt], out: &mut Vec<(&'a Stmt, Option<&'a Rc<FuncDef>>)>, func: Option<&'a Rc<FuncDef>>) {
    for s in stmts {
        out.push((s, func));
        match &s.kind {
            StmtKind::Define(d) => {
                all_stmts(&d.body, out, Some(d));
                continue;
            }
            StmtKind::Data(d) => {
                for m in &d.methods {
                    all_stmts(&m.body, out, Some(m));
                }
                continue;
            }
            StmtKind::Scene(n) | StmtKind::Stage(n) | StmtKind::Gui(n) | StmtKind::Prefab(_, n) => {
                let mut bs = vec![];
                decl_blocks(n, &mut bs);
                for b in bs {
                    all_stmts(b, out, func);
                }
            }
            _ => {}
        }
        for b in blocks(s) {
            all_stmts(b, out, func);
        }
        let mut ls = vec![];
        for e in exprs(s) {
            lambdas(e, &mut ls);
        }
        for d in ls {
            all_stmts(&d.body, out, Some(d));
        }
    }
}

fn always_returns(b: &[Stmt]) -> bool {
    match b.last().map(|s| &s.kind) {
        Some(StmtKind::Return(_)) => true,
        Some(StmtKind::If(arms, Some(els))) => arms.iter().all(|(_, b)| always_returns(b)) && always_returns(els),
        _ => false,
    }
}

struct Typer<'a> {
    facts: Facts<'a>,
    vars: HashMap<String, Option<Ty>>,
    rets: HashMap<String, Option<Ty>>,
    /// a throwaway interpreter, to ask the real built-in methods whether a name exists
    probe: Option<Interp>,
    diags: Vec<EzaError>,
    file: String,
    line: usize,
}

/// Runs the type pass over a program and the files it includes; returns the problems found.
pub fn check<'p>(progs: &'p [(String, Vec<Stmt>)]) -> Vec<EzaError> {
    let mut facts: Facts<'p> = Facts::default();
    let mut every: Vec<(&'p Stmt, Option<&'p Rc<FuncDef>>, &'p str)> = vec![];
    for (file, prog) in progs {
        let mut v = vec![];
        all_stmts(prog, &mut v, None);
        every.extend(v.into_iter().map(|(s, f)| (s, f, file.as_str())));
    }
    for name in ["keyboard", "mouse", "global", "screen", "scene", "stage", "self", "super", "sound", "args"] {
        facts.evidence.entry(name.into()).or_default().push((Ev::Fixed(Ty::Unknown), 0, String::new()));
    }
    facts.evidence.entry("pi".into()).or_default().push((Ev::Fixed(Ty::Num), 0, String::new()));
    for (s, func, file) in &every {
        let file = file.to_string();
        let put = |facts: &mut Facts<'p>, n: &str, ev: Ev<'p>| facts.evidence.entry(n.to_string()).or_default().push((ev, s.line, file.clone()));
        let _ = func;
        match &s.kind {
            StmtKind::Assign(n, Expr::Lambda(d)) => {
                put(&mut facts, n, Ev::Fixed(Ty::Func));
                facts.funcs.entry(n.clone()).or_default().push(d.clone());
            }
            StmtKind::Assign(n, e) => put(&mut facts, n, Ev::Is(e)),
            StmtKind::Change(target, mode, e) => match target {
                Expr::Ident(n) => put(&mut facts, n, if *mode == ChangeMode::To { Ev::Is(e) } else { Ev::By(e) }),
                Expr::Field(_, f) => {
                    facts.written.insert(f.clone());
                }
                _ => {}
            },
            StmtKind::Each(v, e, _) if !v.contains(',') => put(&mut facts, v, Ev::Item(e)),
            StmtKind::Each(v, _, _) => {
                for n in each_names(v) {
                    put(&mut facts, n, Ev::Fixed(Ty::Unknown));
                }
            }
            StmtKind::Unpack { names, .. } => {
                for n in names {
                    put(&mut facts, n, Ev::Fixed(Ty::Unknown));
                }
            }
            StmtKind::Serve { pages, .. } => {
                for p in pages {
                    put(&mut facts, "request", Ev::Fixed(Ty::Unknown));
                    put(&mut facts, "response", Ev::Fixed(Ty::Unknown));
                    // /hello/{name}: the name always holds text
                    for n in crate::server::path_params(&p.path) {
                        put(&mut facts, &n, Ev::Fixed(Ty::Str));
                    }
                }
            }
            StmtKind::Define(d) => {
                put(&mut facts, &d.name, Ev::Fixed(Ty::Func));
                facts.funcs.entry(d.name.clone()).or_default().push(d.clone());
            }
            StmtKind::Data(d) => {
                put(&mut facts, &d.name, Ev::Fixed(Ty::Type(d.name.clone())));
                facts.datas.entry(d.name.clone()).or_default().push(d.clone());
                for m in &d.methods {
                    for p in &m.params {
                        put(&mut facts, p, Ev::Fixed(Ty::Unknown));
                    }
                }
            }
            StmtKind::Prefab(n, _) => put(&mut facts, n, Ev::Fixed(Ty::Prefab)),
            StmtKind::Attempt(_, n, _) | StmtKind::Mimic(n, _, _) | StmtKind::Param(n, _) | StmtKind::Style(n, _, _) => put(&mut facts, n, Ev::Fixed(Ty::Unknown)),
            StmtKind::Use { alias, .. } => put(&mut facts, alias, Ev::Fixed(Ty::Unknown)),
            StmtKind::OnEvent { var: Some(v), .. } => put(&mut facts, v, Ev::Fixed(Ty::Unknown)),
            StmtKind::OnTouch { names: Some((x, y)), .. } => {
                put(&mut facts, x, Ev::Fixed(Ty::Unknown));
                put(&mut facts, y, Ev::Fixed(Ty::Unknown));
            }
            StmtKind::Scene(n) | StmtKind::Stage(n) | StmtKind::Gui(n) => {
                let mut names = vec![];
                decl_names(n, &mut names);
                for name in names {
                    put(&mut facts, &name, Ev::Fixed(Ty::Unknown));
                }
            }
            _ => {}
        }
    }
    // function and lambda parameters can hold anything
    for defs in facts.funcs.values() {
        for d in defs {
            for p in &d.params {
                facts.evidence.entry(p.clone()).or_default().push((Ev::Fixed(Ty::Unknown), 0, String::new()));
            }
        }
    }
    let mut lam = vec![];
    for (s, _, _) in &every {
        for e in exprs(s) {
            lambdas(e, &mut lam);
        }
    }
    let lam_params: Vec<String> = lam.iter().flat_map(|d| d.params.clone()).collect();
    for p in lam_params {
        facts.evidence.entry(p).or_default().push((Ev::Fixed(Ty::Unknown), 0, String::new()));
    }

    let mut t = Typer { facts, vars: HashMap::new(), rets: HashMap::new(), probe: None, diags: vec![], file: String::new(), line: 0 };
    t.solve();
    for (s, _, file) in &every {
        t.file = file.to_string();
        t.line = s.line;
        t.report_stmt(s);
    }
    t.diags
}

impl<'a> Typer<'a> {
    /// Works out every name's kind, repeating until nothing changes (names depend on each other).
    fn solve(&mut self) {
        let names: Vec<String> = self.facts.evidence.keys().cloned().collect();
        for _ in 0..8 {
            let mut changed = false;
            for n in &names {
                let mut ty: Option<Ty> = None;
                for (ev, _, _) in &self.facts.evidence[n] {
                    let t = match ev {
                        Ev::Fixed(t) => Some(t.clone()),
                        Ev::Is(e) => self.ty(e),
                        Ev::By(e) => match self.ty(e) {
                            None => None,
                            Some(Ty::Num) => Some(Ty::Num),
                            Some(Ty::List) => Some(Ty::List),
                            Some(_) => Some(Ty::Unknown),
                        },
                        Ev::Item(e) => match self.ty(e) {
                            None => None,
                            Some(Ty::Num) => Some(Ty::Num),
                            Some(Ty::Str) | Some(Ty::Dict) => Some(Ty::Str),
                            Some(_) => Some(Ty::Unknown),
                        },
                    };
                    ty = join(ty, t);
                }
                if self.vars.get(n) != Some(&ty) {
                    self.vars.insert(n.clone(), ty);
                    changed = true;
                }
            }
            let funcs: Vec<(String, Rc<FuncDef>)> =
                self.facts.funcs.iter().filter(|(_, d)| d.len() == 1).map(|(n, d)| (n.clone(), d[0].clone())).collect();
            for (n, d) in funcs {
                let r = self.returns(&d);
                if self.rets.get(&n) != Some(&r) {
                    self.rets.insert(n, r);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }

    /// What a function gives back.
    fn returns(&self, d: &FuncDef) -> Option<Ty> {
        if d.has_wait {
            return Some(Ty::Nothing);
        }
        let mut r: Option<Ty> = if always_returns(&d.body) { None } else { Some(Ty::Nothing) };
        let mut v = vec![];
        all_stmts(&d.body, &mut v, None);
        let mut any = false;
        for (s, f) in v {
            if f.is_some() {
                continue; // a function inside this one
            }
            if let StmtKind::Return(xs) = &s.kind {
                any = true;
                r = join(r, if xs.len() == 1 { self.ty(&xs[0]) } else if xs.is_empty() { Some(Ty::Nothing) } else { Some(Ty::List) });
            }
        }
        if !any {
            return Some(Ty::Nothing);
        }
        r
    }

    fn var(&self, n: &str) -> Option<Ty> {
        match self.vars.get(n) {
            Some(t) => t.clone(),
            None if crate::methods::BUILTINS.contains(&n) => Some(Ty::Func),
            None => Some(Ty::Unknown),
        }
    }

    fn data(&self, name: &str) -> Option<Rc<DataDecl>> {
        match self.facts.datas.get(name) {
            Some(v) if v.len() == 1 => Some(v[0].clone()),
            _ => None,
        }
    }

    /// The fields and functions of a data type, its parents' included. None if a parent isn't known.
    fn members(&self, name: &str) -> Option<(Vec<String>, Vec<Rc<FuncDef>>)> {
        let (mut fields, mut funcs) = (vec![], vec![]);
        let mut cur = Some(name.to_string());
        let mut depth = 0;
        while let Some(n) = cur {
            let d = self.data(&n)?;
            fields.extend(d.fields.iter().map(|(f, _)| f.clone()));
            funcs.extend(d.methods.iter().cloned());
            cur = d.parent.clone();
            depth += 1;
            if depth > 20 {
                return None;
            }
        }
        Some((fields, funcs))
    }

    /// The kind of value an expression gives (None = not worked out yet).
    fn ty(&self, e: &Expr) -> Option<Ty> {
        use Ty::*;
        Some(match e {
            Expr::Num(_) => Num,
            Expr::Str(_) => Str,
            Expr::Bool(_) => Bool,
            Expr::None => Nothing,
            Expr::Color(_) => Color,
            Expr::List(_) => List,
            Expr::Dict(_) => Dict,
            Expr::Lambda(_) => Func,
            Expr::Ident(n) => return self.var(n),
            Expr::Unary(op, x) => {
                let t = self.ty(x)?;
                match *op {
                    "not" => Bool,
                    "~" => Num,
                    _ => match t {
                        Num => Num,
                        List => List,
                        _ => Unknown,
                    },
                }
            }
            Expr::Binary(op, l, r) => {
                let (a, b) = (self.ty(l), self.ty(r));
                match op {
                    // `a or b` gives one of the two values
                    Op::And | Op::Or => match (a, b) {
                        (Some(Bool), Some(Bool)) => Bool,
                        (x, y) => return join(x, y),
                    },
                    Op::Eq | Op::Ne | Op::Lt | Op::Gt | Op::Le | Op::Ge | Op::In => Bool,
                    _ => {
                        let (a, b) = (a?, b?);
                        binary_result(*op, &a, &b).unwrap_or(Unknown)
                    }
                }
            }
            Expr::Call(callee, _) => match &**callee {
                Expr::Ident(n) => match self.var(n)? {
                    Type(t) => Obj(t),
                    Func if self.vars.get(n).is_none() => builtin_result(n),
                    Func => match self.rets.get(n) {
                        Some(r) => return r.clone(),
                        None => Unknown,
                    },
                    _ => Unknown,
                },
                Expr::Field(obj, m) => match self.ty(obj)? {
                    Str => method_result(&Str, m),
                    List => method_result(&List, m),
                    Num => method_result(&Num, m),
                    _ => Unknown,
                },
                _ => Unknown,
            },
            Expr::Field(obj, f) => match (self.ty(obj)?, f.as_str()) {
                (Prefab, "all") => List,
                (Prefab, "count") => Num,
                (Str, "length") | (List, "length") => Num,
                (Str, "upper" | "lower" | "trim") => Str,
                _ => Unknown,
            },
            Expr::Index(obj, _) => match self.ty(obj)? {
                Str => Str,
                _ => Unknown,
            },
            Expr::Range(..) => List,
            Expr::Spawn { .. } | Expr::Load(_) | Expr::Pop(_) => Unknown,
        })
    }

    fn known(&self, e: &Expr) -> Ty {
        self.ty(e).unwrap_or(Ty::Unknown)
    }

    fn report(&mut self, msg: String, at: &Expr, labels: Vec<(&Expr, String)>, notes: Vec<String>, help: Option<String>) {
        let mut d = EzaError::check(self.line, msg, false).in_file(&self.file);
        d.label(Target::Expr(at.clone()), "", true);
        for (e, m) in labels {
            d.label(Target::Expr(e.clone()), m, false);
        }
        d.notes.extend(notes);
        d.help.extend(help);
        self.diags.push(d);
    }

    /// "score is a number (made on line 3)" for a plain name.
    fn origin(&self, e: &Expr) -> Option<String> {
        let Expr::Ident(n) = e else { return None };
        let t = self.known(e);
        let (_, line, file) = self.facts.evidence.get(n)?.iter().find(|(ev, l, _)| *l > 0 && matches!(ev, Ev::Is(_)))?;
        let at = if *file == self.file { format!("line {}", line) } else { format!("line {} of {}", line, file) };
        Some(format!("{} is {} - it's made on {}", n, t.describe(), at))
    }

    fn report_stmt(&mut self, s: &Stmt) {
        for e in exprs(s) {
            self.check(e);
        }
        if let StmtKind::Unpack { names, value: list @ Expr::List(items), .. } = &s.kind {
            if items.len() != names.len() {
                self.report(
                    format!("{} needs a list of {} things, but this one has {}", names.join(", "), names.len(), items.len()),
                    list,
                    vec![],
                    vec![],
                    Some("give one value for each name".into()),
                );
            }
        }
        if let StmtKind::Each(_, e, _) = &s.kind {
            let t = self.known(e);
            if matches!(t, Ty::Bool | Ty::Nothing | Ty::Color | Ty::Func | Ty::Obj(_) | Ty::Type(_) | Ty::Prefab) {
                let help = match t {
                    Ty::Prefab => Some(format!("to go through every live copy:   each thing in {}.all", crate::diagnose::code(e))),
                    _ => Some("each goes through a list, a number (0, 1, 2 ...), text (letter by letter) or a dictionary's keys".into()),
                };
                let notes = self.origin(e).into_iter().collect();
                let msg = match e {
                    Expr::Ident(n) => format!("each can't go through {} - {} is {}", n, n, t.describe()),
                    _ => format!("each can't go through {}", t.describe()),
                };
                self.report(msg, e, vec![], notes, help);
            }
        }
    }

    /// Looks for sure mistakes in an expression; gives its kind (Unknown after a mistake, so one slip isn't reported twice).
    fn check(&mut self, e: &Expr) -> Ty {
        match e {
            Expr::Binary(op, l, r) => {
                let (a, b) = (self.check(l), self.check(r));
                if !a.known() || !b.known() || matches!(op, Op::And | Op::Or | Op::Eq | Op::Ne) {
                    return self.known(e);
                }
                if *op == Op::In {
                    if matches!(b, Ty::Num | Ty::Bool | Ty::Nothing | Ty::Color | Ty::Func) {
                        let notes = self.origin(r).into_iter().collect();
                        self.report(
                            format!("'in' looks inside a list, text, dictionary or a range like 1 to 10, but {} is {}", crate::diagnose::code(r), b.describe()),
                            r,
                            vec![],
                            notes,
                            Some(format!("to check a number is between two others:   {} in 1 to 10", crate::diagnose::code(l))),
                        );
                        return Ty::Unknown;
                    }
                    return self.known(e);
                }
                let ok = if op.is_comparison() {
                    matches!((&a, &b), (Ty::Num, Ty::Num) | (Ty::Str, Ty::Str))
                } else {
                    binary_result(*op, &a, &b).is_some()
                };
                if ok {
                    return self.known(e);
                }
                let verb = match op {
                    Op::Add => format!("can't add {} and {}", a.describe(), b.describe()),
                    Op::Sub => format!("can't subtract {} from {}", b.describe(), a.describe()),
                    Op::Mul => format!("can't multiply {} by {}", a.describe(), b.describe()),
                    Op::Div => format!("can't divide {} by {}", a.describe(), b.describe()),
                    _ if op.is_comparison() => format!("can't compare {} with {} using '{}'", a.describe(), b.describe(), op.as_str()),
                    _ => format!("can't use '{}' on {} and {}", op.as_str(), a.describe(), b.describe()),
                };
                let quoted_num = |x: &Expr| matches!(x, Expr::Str(t) if t.trim().parse::<f64>().is_ok());
                let help = match (&a, &b) {
                    (Ty::Str, Ty::Num) if quoted_num(l) => Some(format!("write the number without quotes:   {} {} {}", crate::diagnose::code(l).trim_matches('"'), op.as_str(), crate::diagnose::code(r))),
                    (Ty::Num, Ty::Str) if quoted_num(r) => Some(format!("write the number without quotes:   {} {} {}", crate::diagnose::code(l), op.as_str(), crate::diagnose::code(r).trim_matches('"'))),
                    (Ty::Str, Ty::Num) => Some(format!("if the text holds a number, turn it into one first:   num({}) {} {}", crate::diagnose::code(l), op.as_str(), crate::diagnose::code(r))),
                    (Ty::Num, Ty::Str) => Some(format!("if the text holds a number, turn it into one first:   {} {} num({})", crate::diagnose::code(l), op.as_str(), crate::diagnose::code(r))),
                    (Ty::Nothing, _) | (_, Ty::Nothing) => Some("give it a value before doing math with it (or check first:  if x != none)".into()),
                    (Ty::Bool, Ty::Num) | (Ty::Num, Ty::Bool) => Some("true/false isn't a number here; use  if  to choose a number instead".into()),
                    _ => None,
                };
                let notes = [self.origin(l), self.origin(r)].into_iter().flatten().collect();
                let labels = vec![(&**l, format!("this is {}", a.describe())), (&**r, format!("this is {}", b.describe()))];
                self.report(verb, e, labels, notes, help);
                Ty::Unknown
            }
            Expr::Unary(op, x) => {
                let t = self.check(x);
                if *op == "-" && t.known() && !matches!(t, Ty::Num | Ty::List) {
                    let notes = self.origin(x).into_iter().collect();
                    self.report(format!("can't make {} negative - {} is {}", crate::diagnose::code(x), crate::diagnose::code(x), t.describe()), e, vec![], notes, None);
                    return Ty::Unknown;
                }
                self.known(e)
            }
            Expr::Call(callee, args) => {
                for a in args {
                    self.check(&a.value);
                }
                match &**callee {
                    Expr::Ident(n) => {
                        let t = self.var(n).unwrap_or(Ty::Unknown);
                        match &t {
                            Ty::Num | Ty::Str | Ty::Bool | Ty::List | Ty::Dict | Ty::Color | Ty::Nothing | Ty::Obj(_) | Ty::Prefab => {
                                let help = if matches!(t, Ty::Prefab) { Some(format!("to make a copy:   spawn {}", n)) } else { None };
                                let notes = self.origin(callee).into_iter().collect();
                                self.report(format!("{} is {}, not a function, so it can't be called with ( )", n, t.describe()), callee, vec![], notes, help);
                                return Ty::Unknown;
                            }
                            Ty::Type(name) => {
                                let name = name.clone();
                                self.check_build(&name, callee, args);
                            }
                            _ => {}
                        }
                    }
                    Expr::Field(obj, m) => {
                        let t = self.check(obj);
                        self.check_member(obj, m, &t, true, Some(args), callee);
                    }
                    other => {
                        self.check(other);
                    }
                }
                self.known(e)
            }
            Expr::Field(obj, f) => {
                let t = self.check(obj);
                if !self.check_member(obj, f, &t, false, None, e) {
                    return Ty::Unknown;
                }
                self.known(e)
            }
            Expr::Index(obj, i) => {
                let (t, it) = (self.check(obj), self.check(i));
                let what = crate::diagnose::code(obj);
                if matches!(t, Ty::Num | Ty::Bool | Ty::Nothing | Ty::Func | Ty::Color) {
                    let notes = self.origin(obj).into_iter().collect();
                    self.report(format!("can't use [ ] on {} - {} is {}", what, what, t.describe()), e, vec![], notes, Some("[ ] picks an item from a list, a letter from text, or a key from a dictionary".into()));
                    return Ty::Unknown;
                }
                if matches!(t, Ty::List | Ty::Str) && it.known() && it != Ty::Num {
                    let help = if t == Ty::List && it == Ty::Str { Some("to look things up by name, use a dictionary:  {key: value}".into()) } else { None };
                    self.report(
                        format!("{} needs a number in [ ], like {}[0], but this is {}", what, what, it.describe()),
                        i,
                        vec![],
                        vec![],
                        help,
                    );
                    return Ty::Unknown;
                }
                self.known(e)
            }
            Expr::List(items) => {
                items.iter().for_each(|x| {
                    self.check(x);
                });
                Ty::List
            }
            Expr::Dict(items) => {
                items.iter().for_each(|(_, x)| {
                    self.check(x);
                });
                Ty::Dict
            }
            Expr::Spawn { prefab, at, props, into } => {
                let t = self.check(prefab);
                if t.known() && t != Ty::Prefab {
                    let notes = self.origin(prefab).into_iter().collect();
                    self.report(format!("spawn needs a prefab, but {} is {}", crate::diagnose::code(prefab), t.describe()), prefab, vec![], notes, Some("make one with:  prefab Name  and an indented object under it".into()));
                }
                for x in at.iter().chain(into.iter()) {
                    self.check(x);
                }
                props.iter().for_each(|(_, x)| {
                    self.check(x);
                });
                if let Some(x) = into {
                    let it = self.known(x);
                    if it.known() && it != Ty::List {
                        self.report(format!("into needs a list, but {} is {}", crate::diagnose::code(x), it.describe()), x, vec![], vec![], Some(format!("make it a list first:   {} = []", crate::diagnose::code(x))));
                    }
                }
                Ty::Unknown
            }
            Expr::Load(x) | Expr::Pop(x) => {
                self.check(x);
                Ty::Unknown
            }
            _ => self.known(e),
        }
    }

    /// `Enemy(hp=5, speeed=2)`: only fields the type has, and no more values than it has fields.
    fn check_build(&mut self, name: &str, callee: &Expr, args: &[Arg]) {
        let Some((fields, _)) = self.members(name) else { return };
        let positional = args.iter().filter(|a| a.name.is_none()).count();
        if positional > fields.len() {
            self.report(
                format!("{} only has {} field(s), but {} values were given", name, fields.len(), positional),
                callee,
                vec![],
                vec![],
                Some(format!("its fields are: {}", fields.join(", "))),
            );
            return;
        }
        for a in args {
            let Some(n) = &a.name else { continue };
            if !fields.contains(n) {
                let hint = crate::suggest::closest(n, &fields).map(|c| format!(" Did you mean '{}'?", c)).unwrap_or_default();
                self.report(format!("{} has no field '{}'.{}", name, n, hint), &a.value, vec![], vec![], Some(format!("its fields are: {}", fields.join(", "))));
            }
        }
    }

    /// `thing.name` / `thing.name(...)`: does `name` exist on that kind of value? Returns false after reporting.
    fn check_member(&mut self, obj: &Expr, name: &str, t: &Ty, called: bool, args: Option<&[Arg]>, at: &Expr) -> bool {
        if ["to_string", "type", "is_a"].contains(&name) {
            return true;
        }
        let what = crate::diagnose::code(obj);
        match t {
            Ty::Obj(ty) => {
                let Some((fields, funcs)) = self.members(ty) else { return true };
                if let Some(f) = funcs.iter().find(|f| f.name == name) {
                    if let Some(args) = args {
                        let positional = args.iter().filter(|a| a.name.is_none()).count();
                        let named_ok = args.iter().all(|a| a.name.as_ref().map_or(true, |n| f.params.contains(n)));
                        let covered = f.params.iter().enumerate().all(|(i, p)| {
                            i < positional || args.iter().any(|a| a.name.as_deref() == Some(p)) || matches!(f.defaults.get(i), Some(Some(_)))
                        });
                        if positional > f.params.len() || !named_ok || !covered {
                            let sig = if f.params.is_empty() { format!("{}.{}()", what, name) } else { format!("{}.{}({})", what, name, f.params.join(", ")) };
                            self.report(
                                format!("{}.{} takes {} argument(s) but was given {}", ty, name, f.params.len(), args.len()),
                                at,
                                vec![],
                                vec![],
                                Some(format!("call it like:   {}", sig)),
                            );
                            return false;
                        }
                    } else if f.required() > 0 {
                        self.report(
                            format!("{}.{} needs {} argument(s)", ty, name, f.params.len()),
                            at,
                            vec![],
                            vec![],
                            Some(format!("call it like:   {}.{}({})", what, name, f.params.join(", "))),
                        );
                        return false;
                    }
                    return true;
                }
                if fields.iter().any(|f| f == name) || self.facts.written.contains(name) || self.has_builtin(t, name, called) {
                    return true;
                }
                let mut pool = fields.clone();
                pool.extend(funcs.iter().map(|f| f.name.clone()));
                let hint = crate::suggest::closest(name, &pool).map(|c| format!(" Did you mean '{}'?", c)).unwrap_or_default();
                let kind = if called { "function" } else { "field or function" };
                let mut has = vec![];
                if !fields.is_empty() {
                    has.push(format!("fields: {}", fields.join(", ")));
                }
                if !funcs.is_empty() {
                    has.push(format!("functions: {}", funcs.iter().map(|f| f.name.clone()).collect::<Vec<_>>().join(", ")));
                }
                let notes = self.origin(obj).into_iter().collect();
                self.report(format!("{} is {}, which has no {} '{}'.{}", what, t.describe(), kind, name, hint), at, vec![], notes, (!has.is_empty()).then(|| format!("{} has {}", ty, has.join("; "))));
                false
            }
            Ty::Prefab => {
                if called || !["all", "count"].contains(&name) {
                    self.report(format!("a prefab only has .all (its live copies) and .count, not '.{}'", name), at, vec![], vec![], Some(format!("to change one copy, spawn it first:   thing = spawn {}", what)));
                    return false;
                }
                true
            }
            Ty::Str | Ty::Num | Ty::List | Ty::Bool | Ty::Color | Ty::Nothing => {
                if self.has_builtin(t, name, called) {
                    return true;
                }
                let notes = self.origin(obj).into_iter().collect();
                let hint = match (t, name) {
                    (Ty::List, "upper" | "lower" | "trim" | "split" | "replace" | "capitalize") => Some(format!("{} is a list; to do it to every item:   {}.map(x -> x.{}())", what, what, name)),
                    (Ty::List, "push" | "append") => Some(format!("to add to the end:   push item to {}", what)),
                    (Ty::Str, "push" | "append" | "add") => Some(format!("to join text:   {} + \"more\"", what)),
                    (Ty::Num, "length" | "upper" | "split") => Some(format!("{} is a number; to treat it as text:   str({}).{}", what, what, name)),
                    (Ty::Nothing, _) => Some(format!("give {} a value first", what)),
                    _ => self.closest_builtin(t, name).map(|c| format!("did you mean  .{}  ?", c)),
                };
                let kind = if called { "method" } else { "property or method" };
                self.report(format!("{} is {}, which has no {} '.{}'", what, t.describe(), kind, name), at, vec![], notes, hint);
                false
            }
            _ => true,
        }
    }

    fn sample(t: &Ty) -> Option<Value> {
        Some(match t {
            Ty::Str => Value::Str("a".into()),
            Ty::Num => Value::Num(1.0),
            Ty::List => Value::list(vec![]),
            Ty::Bool => Value::Bool(true),
            Ty::Color => Value::Color(Rc::new([0.0, 0.0, 0.0, 1.0])),
            Ty::Nothing => Value::None,
            // what every object has (.animating, .collides_with ...)
            Ty::Obj(name) => Value::obj(crate::value::Obj::new(name)),
            _ => return None,
        })
    }

    /// Asks the real built-in methods (with no arguments) whether `name` exists for this kind of value.
    fn has_builtin(&mut self, t: &Ty, name: &str, called: bool) -> bool {
        let Some(v) = Self::sample(t) else { return true };
        let probe = self.probe.get_or_insert_with(|| Interp::new(std::path::Path::new("check.eza")));
        match crate::methods::call(probe, v, name, vec![], called) {
            Ok(_) => true,
            Err(e) => !(e.msg.contains("has no method") || e.msg.contains("has no property")),
        }
    }

    fn closest_builtin(&mut self, t: &Ty, name: &str) -> Option<String> {
        const ALL: &[&str] = &[
            "upper", "lower", "length", "trim", "split", "replace", "contains", "starts_with", "ends_with", "capitalize", "reverse", "lines", "words",
            "first", "last", "sort", "sum", "min", "max", "filter", "map", "find", "count", "join", "add", "remove", "unique", "index_of", "abs", "floor",
            "ceil", "round", "sqrt", "pow", "clamp", "format", "magnitude", "normalize", "dot", "cross", "move_toward", "angle", "rotate", "lighten",
            "darken", "mix", "invert",
        ];
        let pool: Vec<String> = ALL.iter().map(|s| s.to_string()).filter(|n| n != name).collect();
        let c = crate::suggest::closest(name, &pool)?;
        if self.has_builtin(t, &c, true) {
            Some(c)
        } else {
            None
        }
    }
}

/// The kind `a op b` gives, or None if Eza can't do it (the same rules as the running program).
fn binary_result(op: Op, a: &Ty, b: &Ty) -> Option<Ty> {
    use Ty::*;
    match (op, a, b) {
        (Op::Add, Num, Num) => Some(Num),
        (Op::Add, Str, _) | (Op::Add, _, Str) => Some(Str),
        (Op::Add | Op::Sub, List, List) => Some(List),
        (Op::Sub | Op::Mul | Op::Rem | Op::BitAnd | Op::BitOr | Op::BitXor | Op::Shl | Op::Shr, Num, Num) => Some(Num),
        (Op::Div, Num, Num) => Some(Num),
        (Op::Mul, Str, Num) => Some(Str),
        (Op::Mul, List, Num) | (Op::Mul, Num, List) | (Op::Div, List, Num) => Some(List),
        _ if a.known() && b.known() => None,
        _ => Some(Unknown),
    }
}

fn builtin_result(n: &str) -> Ty {
    match n {
        "len" | "num" | "int" | "random" | "random_int" | "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "atan2" | "radians" | "degrees" | "distance" | "ord" => Ty::Num,
        "str" | "type" | "chr" | "table" | "panel" => Ty::Str,
        "range" | "files" | "folders" | "find_files" => Ty::List,
        "exists" | "is_folder" => Ty::Bool,
        _ => Ty::Unknown,
    }
}

fn method_result(t: &Ty, m: &str) -> Ty {
    match (t, m) {
        (Ty::Str, "upper" | "lower" | "trim" | "trimleft" | "trimright" | "capitalize" | "replace" | "reverse" | "repeat" | "pad_left" | "pad_right") => Ty::Str,
        (Ty::Str, "split" | "lines" | "words" | "find_all") => Ty::List,
        (Ty::Str, "length") | (Ty::List, "length" | "sum" | "count") => Ty::Num,
        (Ty::Str, "contains" | "starts_with" | "ends_with" | "matches") => Ty::Bool,
        (Ty::List, "filter" | "map" | "sort" | "sortBy" | "sort_by" | "reverse" | "unique" | "add" | "remove" | "normalize" | "move_toward") => Ty::List,
        (Ty::List, "join") => Ty::Str,
        (Ty::List, "contains" | "any" | "all") => Ty::Bool,
        (Ty::Num, "abs" | "floor" | "ceil" | "round" | "sqrt" | "pow" | "clamp" | "min" | "max") => Ty::Num,
        (Ty::Num, "format" | "to_binary" | "to_hex") => Ty::Str,
        _ => Ty::Unknown,
    }
}

#[cfg(test)]
mod tests {
    fn problems(src: &str) -> Vec<String> {
        let prog = crate::parser::Parser::new(crate::lexer::lex(src).unwrap()).program().unwrap();
        super::check(&[("t.eza".to_string(), prog)]).into_iter().map(|d| d.msg.clone()).collect()
    }

    #[test]
    fn finds_sure_mistakes() {
        let p = problems("data Enemy\n    hp = 1\nscore = 0\nname = \"a\"\nx = score - name\ny = name.uper()\ng = Enemy()\nprint(g.hpp)\nscore()\n");
        assert_eq!(p.len(), 4, "{:?}", p);
        assert!(p[0].contains("can't subtract"));
        assert!(p[1].contains("'.uper'"));
        assert!(p[2].contains("'hpp'"));
        assert!(p[3].contains("not a function"));
    }

    #[test]
    fn quiet_when_unsure() {
        let p = problems("x = 5\nif random() > 0.5\n    change x to \"a\"\nprint(x - 1)\ndefine f, n\n    return n.anything\nbest = none\nchange best to 3\nprint(best + 1)\nt = \"n=\" + 5\n");
        assert!(p.is_empty(), "{:?}", p);
    }
}
