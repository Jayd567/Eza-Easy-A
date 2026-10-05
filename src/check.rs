//! `eza check`: finds mistakes before the program runs - typos in names (with "did you mean"),
//! wrong argument counts, and `x = ...` used twice for the same name. It follows `include` files.
use crate::ast::*;
use crate::{lexer, methods, parser, suggest};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};

pub struct Diag {
    pub kind: &'static str,
    pub file: String,
    pub line: usize,
    pub msg: String,
}

impl fmt::Display for Diag {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "[{} Error] {}:{}: {}", self.kind, self.file, self.line, self.msg)
    }
}

/// Names the language provides without any declaration.
const IMPLICIT: &[&str] = &["keyboard", "mouse", "global", "pi", "screen", "scene", "self", "super", "sound", "args"];

#[derive(Default)]
struct Ctx {
    /// every name that is created anywhere in the program (order doesn't matter)
    names: HashSet<String>,
    defs: HashMap<String, Vec<Vec<String>>>,
    plain: HashSet<String>,
    binders: HashSet<String>,
    /// a gui window with a computed name: we can't know every name, so skip the typo check
    lenient: bool,
    /// `use "x.eza" as m`: m -> the names that module creates
    modules: HashMap<String, HashSet<String>>,
    diags: Vec<Diag>,
    file: String,
}

/// `include "x.eza"` lines (line, path), looking inside nested blocks too.
fn includes(stmts: &[Stmt], out: &mut Vec<(usize, String)>) {
    for s in stmts {
        match &s.kind {
            StmtKind::Include(p) => out.push((s.line, p.clone())),
            StmtKind::If(arms, els) => {
                arms.iter().for_each(|(_, b)| includes(b, out));
                if let Some(b) = els {
                    includes(b, out);
                }
            }
            StmtKind::Each(_, _, b) | StmtKind::While(_, b) | StmtKind::Test(_, b) => includes(b, out),
            StmtKind::Define(d) => includes(&d.body, out),
            _ => {}
        }
    }
}

/// `use` lines (line, path, alias), at the top level or inside blocks.
fn uses(stmts: &[Stmt], out: &mut Vec<(usize, String, String)>) {
    for s in stmts {
        match &s.kind {
            StmtKind::Use { path, alias } => out.push((s.line, path.clone(), alias.clone())),
            StmtKind::If(arms, els) => {
                arms.iter().for_each(|(_, b)| uses(b, out));
                if let Some(b) = els {
                    uses(b, out);
                }
            }
            StmtKind::Each(_, _, b) | StmtKind::While(_, b) | StmtKind::Test(_, b) => uses(b, out),
            StmtKind::Define(d) => uses(&d.body, out),
            _ => {}
        }
    }
}

pub fn check_file(path: &str) -> Vec<Diag> {
    check_file_seen(path, &mut HashSet::new())
}

/// `modules_done` holds the module files already checked, so each is reported once
/// (and two modules that use each other don't loop forever).
fn check_file_seen(path: &str, modules_done: &mut HashSet<PathBuf>) -> Vec<Diag> {
    modules_done.insert(Path::new(path).canonicalize().unwrap_or_else(|_| PathBuf::from(path)));
    let syntax = |file: &str, line: usize, msg: String| vec![Diag { kind: "Syntax", file: file.to_string(), line, msg }];
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => return syntax(path, 0, format!("can't open the file: {}", e)),
    };
    let first = match lexer::lex(&src).and_then(|t| parser::Parser::new(t).program()) {
        Ok(p) => p,
        Err(e) => return syntax(path, e.line, e.msg.clone()),
    };
    let mut c = Ctx::default();
    let mut progs: Vec<(String, Vec<Stmt>)> = vec![(path.to_string(), first)];
    let mut seen: HashSet<PathBuf> = HashSet::new();
    seen.insert(Path::new(path).canonicalize().unwrap_or_else(|_| PathBuf::from(path)));
    let mut i = 0;
    while i < progs.len() {
        c.collect(&progs[i].1);
        let mut incs = vec![];
        includes(&progs[i].1, &mut incs);
        let dir = Path::new(&progs[i].0).parent().map(|p| p.to_path_buf()).unwrap_or_default();
        let from = progs[i].0.clone();
        for (line, rel) in incs {
            let full = dir.join(&rel);
            let key = full.canonicalize().unwrap_or_else(|_| full.clone());
            if !seen.insert(key) {
                continue;
            }
            match std::fs::read_to_string(&full) {
                Err(_) => c.diags.push(Diag { kind: "Check", file: from.clone(), line, msg: format!("can't find the included file \"{}\"", rel) }),
                Ok(text) => match lexer::lex(&text).and_then(|t| parser::Parser::new(t).program()) {
                    Ok(p) => progs.push((full.display().to_string(), p)),
                    Err(e) => c.diags.push(Diag { kind: "Syntax", file: full.display().to_string(), line: e.line, msg: e.msg.clone() }),
                },
            }
        }
        let mut used = vec![];
        uses(&progs[i].1, &mut used);
        for (line, rel, alias) in used {
            c.names.insert(alias.clone());
            let mut full = dir.join(&rel);
            if full.extension().is_none() {
                full.set_extension("eza");
            }
            let Ok(text) = std::fs::read_to_string(&full) else {
                c.diags.push(Diag { kind: "Check", file: from.clone(), line, msg: format!("can't find the module file \"{}\"", rel) });
                continue;
            };
            // what `alias.name` may refer to: every name the module creates
            let mut m = Ctx::default();
            if let Ok(p) = lexer::lex(&text).and_then(|t| parser::Parser::new(t).program()) {
                m.collect(&p);
                let mut incs = vec![];
                includes(&p, &mut incs);
                // names from the module's own includes aren't known here, so skip checking its names
                if incs.is_empty() && !m.lenient {
                    c.modules.insert(alias, m.names);
                }
            }
            let key = full.canonicalize().unwrap_or_else(|_| full.clone());
            if !modules_done.contains(&key) {
                let more = check_file_seen(&full.display().to_string(), modules_done);
                c.diags.extend(more);
            }
        }
        i += 1;
    }
    for (file, prog) in &progs {
        c.file = file.clone();
        c.check_block(prog, &[]);
    }
    c.diags
}

impl Ctx {
    // ---------- pass 1: which names exist ----------

    fn collect(&mut self, stmts: &[Stmt]) {
        for s in stmts {
            self.collect_stmt(s);
        }
    }

    fn collect_func(&mut self, d: &FuncDef) {
        for p in &d.params {
            self.names.insert(p.clone());
            self.binders.insert(p.clone());
        }
        self.collect(&d.body);
    }

    fn collect_stmt(&mut self, s: &Stmt) {
        match &s.kind {
            StmtKind::Assign(n, e) => {
                self.names.insert(n.clone());
                if let Expr::Lambda(d) = e {
                    self.defs.entry(n.clone()).or_default().push(d.params.clone());
                    self.collect_func(d);
                } else {
                    self.plain.insert(n.clone());
                }
            }
            StmtKind::Define(d) => {
                self.names.insert(d.name.clone());
                self.defs.entry(d.name.clone()).or_default().push(d.params.clone());
                self.collect_func(d);
            }
            StmtKind::Each(v, _, b) => {
                self.names.insert(v.clone());
                self.binders.insert(v.clone());
                self.collect(b);
            }
            StmtKind::If(arms, els) => {
                for (_, b) in arms {
                    self.collect(b);
                }
                if let Some(b) = els {
                    self.collect(b);
                }
            }
            StmtKind::While(_, b) | StmtKind::Test(_, b) | StmtKind::Persist { body: b, .. } => self.collect(b),
            StmtKind::On(_, b) => self.collect(b),
            StmtKind::Param(n, _) => {
                self.names.insert(n.clone());
            }
            StmtKind::Data(d) => {
                self.names.insert(d.name.clone());
                for m in &d.methods {
                    self.collect_func(m);
                }
            }
            StmtKind::Attempt(a, n, h) => {
                self.names.insert(n.clone());
                self.binders.insert(n.clone());
                self.collect(a);
                self.collect(h);
            }
            StmtKind::Mimic(sh, _, b) => {
                self.names.insert(sh.clone());
                self.binders.insert(sh.clone());
                self.collect(b);
            }
            StmtKind::Prefab(n, node) => {
                self.names.insert(n.clone());
                self.collect_decl(node);
            }
            StmtKind::Scene(node) => {
                self.names.insert("scene".into());
                self.collect_decl(node);
            }
            StmtKind::Stage(node) => {
                self.names.insert("stage".into());
                self.collect_decl(node);
            }
            StmtKind::Style(n, _, _) => {
                self.names.insert(n.clone());
            }
            StmtKind::Use { alias, .. } => {
                self.names.insert(alias.clone());
            }
            StmtKind::Gui(node) => {
                match &node.label {
                    Some(Expr::Str(l)) => {
                        self.names.insert(l.replace(|c: char| !c.is_alphanumeric(), "_"));
                    }
                    _ => self.lenient = true,
                }
                self.collect_decl(node);
            }
            _ => {}
        }
    }

    fn collect_decl(&mut self, n: &DeclNode) {
        self.names.insert(n.kind.clone());
        for (k, e) in &n.props {
            if k == "name" {
                if let Expr::Str(s) = e {
                    self.names.insert(s.clone());
                }
            }
        }
        if let Some(h) = &n.handler {
            self.collect(h);
        }
        self.collect(&n.events);
        for c in &n.children {
            self.collect_decl(c);
        }
    }

    // ---------- pass 2: look for mistakes ----------

    fn diag(&mut self, line: usize, msg: String) {
        self.diags.push(Diag { kind: "Check", file: self.file.clone(), line, msg });
    }

    fn use_name(&mut self, n: &str, line: usize) {
        if self.lenient || self.names.contains(n) || IMPLICIT.contains(&n) || methods::BUILTINS.contains(&n) {
            return;
        }
        let mut pool: Vec<String> = self.names.iter().cloned().collect();
        pool.extend(IMPLICIT.iter().map(|s| s.to_string()));
        pool.extend(methods::BUILTINS.iter().map(|s| s.to_string()));
        let hint = suggest::closest(n, &pool).map(|c| format!(" Did you mean '{}'?", c)).unwrap_or_default();
        self.diag(line, format!("'{}' isn't created anywhere in this program.{}", n, hint));
    }

    /// `initial` are names that already exist in this block's scope (function parameters, loop variables).
    fn check_block(&mut self, stmts: &[Stmt], initial: &[String]) {
        let mut seen: HashMap<String, usize> = initial.iter().map(|n| (n.clone(), 0)).collect();
        for s in stmts {
            if let StmtKind::Assign(n, _) = &s.kind {
                match seen.get(n) {
                    Some(0) => self.diag(s.line, format!("'{0}' already exists here - use 'change {0} to ...' to update it", n)),
                    Some(first) => {
                        let first = *first;
                        self.diag(s.line, format!("'{0}' already exists (line {1}) - use 'change {0} to ...' to update it", n, first))
                    }
                    None => {
                        seen.insert(n.clone(), s.line);
                    }
                }
            }
            self.check_stmt(s);
        }
    }

    fn check_func(&mut self, d: &FuncDef) {
        self.check_block(&d.body, &d.params);
    }

    fn check_stmt(&mut self, s: &Stmt) {
        let line = s.line;
        match &s.kind {
            StmtKind::Assign(_, e) => self.check_expr(e, line),
            StmtKind::Change(t, _, e) => {
                self.check_expr(t, line);
                self.check_expr(e, line);
            }
            StmtKind::Expr(e) | StmtKind::Destroy(e) | StmtKind::Expect(e) | StmtKind::Param(_, e) => self.check_expr(e, line),
            StmtKind::If(arms, els) => {
                for (c, b) in arms {
                    self.check_expr(c, line);
                    self.check_block(b, &[]);
                }
                if let Some(b) = els {
                    self.check_block(b, &[]);
                }
            }
            StmtKind::Each(v, e, b) => {
                self.check_expr(e, line);
                self.check_block(b, &[v.clone()]);
            }
            StmtKind::While(c, b) => {
                self.check_expr(c, line);
                self.check_block(b, &[]);
            }
            StmtKind::Define(d) => self.check_func(d),
            StmtKind::Return(es) => es.iter().for_each(|e| self.check_expr(e, line)),
            StmtKind::Rewind { target, steps } => {
                if let Some(t) = target {
                    self.use_name(t, line);
                }
                if let Some(e) = steps {
                    self.check_expr(e, line);
                }
            }
            StmtKind::Data(d) => {
                if let Some(p) = &d.parent {
                    self.use_name(p, line);
                }
                d.fields.iter().for_each(|(_, e)| self.check_expr(e, line));
                for m in &d.methods {
                    let mut known = m.params.clone();
                    known.push("self".into());
                    if d.parent.is_some() {
                        known.push("super".into());
                    }
                    self.check_block(&m.body, &known);
                }
            }
            StmtKind::Attempt(a, n, h) => {
                self.check_block(a, &[]);
                self.check_block(h, &[n.clone()]);
            }
            StmtKind::Scene(n) | StmtKind::Gui(n) | StmtKind::Prefab(_, n) | StmtKind::Stage(n) => self.check_decl(n),
            StmtKind::Push(v, t) => {
                self.check_expr(v, line);
                self.check_expr(t, line);
            }
            StmtKind::Style(..) => {}
            StmtKind::On(e, b) => {
                self.check_expr(e, line);
                self.check_block(b, &[]);
            }
            StmtKind::Persist { body, steps, until } => {
                self.check_block(body, &[]);
                steps.iter().chain(until.iter()).for_each(|e| self.check_expr(e, line));
            }
            StmtKind::Mimic(_, e, b) => {
                self.check_expr(e, line);
                self.check_block(b, &[]);
            }
            StmtKind::Tick(e) => e.iter().for_each(|e| self.check_expr(e, line)),
            StmtKind::Tween { target, value, steps, .. } => {
                self.check_expr(target, line);
                self.check_expr(value, line);
                self.check_expr(steps, line);
            }
            StmtKind::Wait { steps, .. } => self.check_expr(steps, line),
            StmtKind::Save { value, path, .. } => {
                self.check_expr(value, line);
                self.check_expr(path, line);
            }
            StmtKind::Test(_, b) => self.check_block(b, &[]),
            StmtKind::Play(p, props) => {
                self.check_expr(p, line);
                props.iter().for_each(|(_, e)| self.check_expr(e, line));
            }
            StmtKind::Stop(e) => e.iter().for_each(|e| self.check_expr(e, line)),
            StmtKind::Emit { count, from, at } => {
                self.check_expr(count, line);
                self.check_expr(from, line);
                at.iter().for_each(|e| self.check_expr(e, line));
            }
            StmtKind::Go(e) => {
                self.check_expr(e, line);
                if let Expr::Str(p) = e {
                    let mut full = Path::new(&self.file).parent().map(|d| d.join(p)).unwrap_or_else(|| PathBuf::from(p));
                    if full.extension().is_none() {
                        full.set_extension("eza");
                    }
                    if !full.is_file() {
                        self.diag(line, format!("can't find the script \"{}\" to go to", p));
                    }
                }
            }
            StmtKind::Include(_) | StmtKind::Use { .. } | StmtKind::Break | StmtKind::Continue => {}
        }
    }

    fn check_decl(&mut self, n: &DeclNode) {
        let line = n.handler.as_ref().and_then(|h| h.first()).map_or(0, |s| s.line);
        if let Some(l) = &n.label {
            self.check_expr(l, line);
        }
        for (_, e) in &n.props {
            self.check_expr(e, line);
        }
        if let Some(h) = &n.handler {
            self.check_block(h, &[]);
        }
        for s in &n.events {
            self.check_stmt(s);
        }
        for c in &n.children {
            self.check_decl(c);
        }
    }

    fn check_expr(&mut self, e: &Expr, line: usize) {
        match e {
            Expr::Ident(n) => self.use_name(n, line),
            Expr::Field(o, f) => {
                self.check_expr(o, line);
                if let Expr::Ident(m) = &**o {
                    self.check_module_name(m, f, line);
                }
            }
            Expr::Index(o, i) => {
                self.check_expr(o, line);
                self.check_expr(i, line);
            }
            Expr::Call(callee, args) => {
                self.check_expr(callee, line);
                for a in args {
                    self.check_expr(&a.value, line);
                }
                if let Expr::Ident(n) = &**callee {
                    self.check_call(n, args, line);
                }
            }
            Expr::Unary(_, x) | Expr::Load(x) | Expr::Pop(x) => self.check_expr(x, line),
            Expr::Binary(_, l, r) => {
                self.check_expr(l, line);
                self.check_expr(r, line);
            }
            Expr::List(items) => items.iter().for_each(|x| self.check_expr(x, line)),
            Expr::Dict(items) => items.iter().for_each(|(_, x)| self.check_expr(x, line)),
            Expr::Spawn { prefab, at, props } => {
                self.check_expr(prefab, line);
                if let Some(a) = at {
                    self.check_expr(a, line);
                }
                props.iter().for_each(|(_, x)| self.check_expr(x, line));
            }
            Expr::Lambda(d) => {
                // `x -> ...` parameters exist inside the short function
                for p in &d.params {
                    self.names.insert(p.clone());
                }
                self.check_func(d)
            }
            Expr::Num(_) | Expr::Str(_) | Expr::Bool(_) | Expr::None | Expr::Color(_) => {}
        }
    }

    /// `enemies.spwan` where enemies is a module: that name has to exist in the module.
    fn check_module_name(&mut self, m: &str, f: &str, line: usize) {
        let Some(names) = self.modules.get(m) else { return };
        if f.starts_with('_') {
            self.diag(line, format!("'{}' is private to the module {} (names starting with _ stay inside their file)", f, m));
        } else if !names.contains(f) {
            let pool: Vec<String> = names.iter().filter(|n| !n.starts_with('_')).cloned().collect();
            let hint = suggest::closest(f, &pool).map(|c| format!(" Did you mean '{}'?", c)).unwrap_or_default();
            self.diag(line, format!("the module {} has no '{}'.{}", m, f, hint));
        }
    }

    fn check_call(&mut self, n: &str, args: &[Arg], line: usize) {
        // only when the name clearly means one function
        let Some(defs) = self.defs.get(n) else { return };
        if defs.len() != 1 || self.plain.contains(n) || self.binders.contains(n) {
            return;
        }
        let params = defs[0].clone();
        let positional = args.iter().filter(|a| a.name.is_none()).count();
        for a in args {
            if let Some(nm) = &a.name {
                if !params.contains(nm) {
                    self.diag(line, format!("{} has no parameter called '{}' (it takes: {})", n, nm, params.join(", ")));
                    return;
                }
            }
        }
        if positional > params.len() {
            self.diag(line, format!("{} takes {} argument(s) but you gave {}", n, params.len(), positional));
            return;
        }
        for (i, p) in params.iter().enumerate() {
            let named = args.iter().any(|a| a.name.as_deref() == Some(p));
            if i >= positional && !named {
                self.diag(line, format!("{} is missing the argument '{}'", n, p));
                return;
            }
        }
    }
}
