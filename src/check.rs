//! `eza check`: finds mistakes before the program runs - typos in names (with "did you mean"),
//! wrong argument counts, and `x = ...` used twice for the same name - and warns about likely
//! mistakes (code that can never run, variables that are never used). It follows `include` and `use`.
use crate::ast::*;
use crate::error::{ErrKind, EzaError, Target};
use crate::{lexer, methods, parser, suggest};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// A problem `eza check` found: an error, or a warning (kind Warning) that doesn't stop the program.
pub type Diag = EzaError;

pub fn is_warning(d: &Diag) -> bool {
    d.kind == ErrKind::Warning
}

/// Every name an expression reads.
fn names_in_expr(e: &Expr, out: &mut HashSet<String>) {
    match e {
        Expr::Ident(n) => {
            out.insert(n.clone());
        }
        Expr::Field(o, _) | Expr::Unary(_, o) | Expr::Load(o) | Expr::Pop(o) => names_in_expr(o, out),
        Expr::Index(o, i) | Expr::Binary(_, o, i) | Expr::Range(o, i) => {
            names_in_expr(o, out);
            names_in_expr(i, out);
        }
        Expr::Call(c, args) => {
            names_in_expr(c, out);
            args.iter().for_each(|a| names_in_expr(&a.value, out));
        }
        Expr::List(items) => items.iter().for_each(|x| names_in_expr(x, out)),
        Expr::Dict(items) => items.iter().for_each(|(_, x)| names_in_expr(x, out)),
        Expr::Lambda(d) => names_in_stmts(&d.body, out),
        Expr::Spawn { prefab, at, props, into } => {
            names_in_expr(prefab, out);
            at.iter().chain(into.iter()).for_each(|a| names_in_expr(a, out));
            props.iter().for_each(|(_, x)| names_in_expr(x, out));
        }
        Expr::Num(_) | Expr::Str(_) | Expr::Bool(_) | Expr::None | Expr::Color(_) => {}
    }
}

fn names_in_decl(n: &DeclNode, out: &mut HashSet<String>) {
    n.label.iter().for_each(|l| names_in_expr(l, out));
    n.props.iter().for_each(|(_, x)| names_in_expr(x, out));
    if let Some(h) = &n.handler {
        names_in_stmts(h, out);
    }
    names_in_stmts(&n.events, out);
    n.children.iter().for_each(|c| names_in_decl(c, out));
}

/// Every name a block of code reads (including nested blocks and short functions).
fn names_in_stmts(stmts: &[Stmt], out: &mut HashSet<String>) {
    for s in stmts {
        let mut e = |x: &Expr| names_in_expr(x, out);
        match &s.kind {
            StmtKind::Assign(_, x) | StmtKind::Expr(x) | StmtKind::Destroy(x) | StmtKind::Expect(x) | StmtKind::Param(_, x) | StmtKind::Go(x) => e(x),
            StmtKind::Unpack { value, .. } => e(value),
            StmtKind::Serve { props, pages } => {
                props.iter().for_each(|(_, x)| names_in_expr(x, out));
                for p in pages {
                    p.props.iter().for_each(|(_, x)| names_in_expr(x, out));
                    names_in_stmts(&p.body, out);
                }
            }
            StmtKind::Change(a, _, b) | StmtKind::Push(a, b) => {
                e(a);
                e(b);
            }
            StmtKind::If(arms, els) => {
                for (c, b) in arms {
                    names_in_expr(c, out);
                    names_in_stmts(b, out);
                }
                if let Some(b) = els {
                    names_in_stmts(b, out);
                }
            }
            StmtKind::Each(_, x, b) | StmtKind::While(x, b) => {
                names_in_expr(x, out);
                names_in_stmts(b, out);
            }
            StmtKind::Define(d) => names_in_stmts(&d.body, out),
            StmtKind::Return(xs) => xs.iter().for_each(|x| names_in_expr(x, out)),
            StmtKind::Rewind { target, steps } => {
                if let Some(t) = target {
                    out.insert(t.clone());
                }
                steps.iter().for_each(|x| names_in_expr(x, out));
            }
            StmtKind::Data(d) => {
                d.fields.iter().for_each(|(_, x)| names_in_expr(x, out));
                d.methods.iter().for_each(|m| names_in_stmts(&m.body, out));
                if let Some(p) = &d.parent {
                    out.insert(p.clone());
                }
            }
            StmtKind::Attempt(a, _, h) => {
                names_in_stmts(a, out);
                names_in_stmts(h, out);
            }
            StmtKind::Scene(n) | StmtKind::Stage(n) | StmtKind::Gui(n) | StmtKind::Prefab(_, n) => names_in_decl(n, out),
            StmtKind::Style(_, props, states) => {
                props.iter().for_each(|(_, x)| names_in_expr(x, out));
                states.iter().for_each(|(_, items)| items.iter().for_each(|(_, x)| names_in_expr(x, out)));
            }
            StmtKind::On(x, b) => {
                names_in_expr(x, out);
                names_in_stmts(b, out);
            }
            StmtKind::OnTouch { a, b, body, .. } => {
                names_in_expr(a, out);
                names_in_expr(b, out);
                names_in_stmts(body, out);
            }
            StmtKind::OnEvent { body, .. } => names_in_stmts(body, out),
            StmtKind::Trigger(_, v) => v.iter().for_each(|x| names_in_expr(x, out)),
            StmtKind::Persist { body, steps, until } => {
                names_in_stmts(body, out);
                steps.iter().chain(until.iter()).for_each(|x| names_in_expr(x, out));
            }
            StmtKind::Mimic(_, x, b) => {
                names_in_expr(x, out);
                names_in_stmts(b, out);
            }
            StmtKind::Tick(x) | StmtKind::Stop(x) => x.iter().for_each(|x| names_in_expr(x, out)),
            StmtKind::Tween { target, value, steps, .. } => {
                for x in [target, value, steps] {
                    names_in_expr(x, out);
                }
            }
            StmtKind::Wait { steps, .. } => names_in_expr(steps, out),
            StmtKind::Save { value, path, .. } => {
                names_in_expr(value, out);
                names_in_expr(path, out);
            }
            StmtKind::Test(_, b) => names_in_stmts(b, out),
            StmtKind::Play(x, props) => {
                names_in_expr(x, out);
                props.iter().for_each(|(_, v)| names_in_expr(v, out));
            }
            StmtKind::Emit { count, from, at } => {
                names_in_expr(count, out);
                names_in_expr(from, out);
                at.iter().for_each(|x| names_in_expr(x, out));
            }
            StmtKind::Include(_) | StmtKind::Use { .. } | StmtKind::Break | StmtKind::Continue => {}
        }
    }
}

/// `x = ...` lines anywhere in a block (name, line), not looking inside nested functions.
fn assigned_in(stmts: &[Stmt], out: &mut Vec<(String, usize)>) {
    for s in stmts {
        match &s.kind {
            StmtKind::Assign(n, _) => out.push((n.clone(), s.line)),
            StmtKind::Unpack { names, create: true, .. } => out.extend(names.iter().map(|n| (n.clone(), s.line))),
            StmtKind::If(arms, els) => {
                arms.iter().for_each(|(_, b)| assigned_in(b, out));
                if let Some(b) = els {
                    assigned_in(b, out);
                }
            }
            StmtKind::Each(_, _, b) | StmtKind::While(_, b) | StmtKind::Attempt(b, _, _) => assigned_in(b, out),
            _ => {}
        }
    }
}

/// Names the language provides without any declaration.
const IMPLICIT: &[&str] = &["keyboard", "mouse", "global", "pi", "screen", "scene", "self", "super", "sound", "args"];

#[derive(Default)]
struct Ctx {
    /// every name that is created anywhere in the program (order doesn't matter)
    names: HashSet<String>,
    defs: HashMap<String, Vec<std::rc::Rc<FuncDef>>>,
    plain: HashSet<String>,
    binders: HashSet<String>,
    /// a gui window with a computed name: we can't know every name, so skip the typo check
    lenient: bool,
    /// `use "x.eza" as m`: m -> the names that module creates
    modules: HashMap<String, HashSet<String>>,
    diags: Vec<Diag>,
    file: String,
    /// the define line of each function: name -> (line, file)
    def_lines: HashMap<String, (usize, String)>,
    /// `on event "x"` and `trigger "x"` names: name -> where it first appears (line, file)
    listened: HashMap<String, (usize, String)>,
    triggered: HashMap<String, (usize, String)>,
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
    check_file_seen(path, &mut HashSet::new(), None)
}

/// The same, with the file's text given (what an editor has, before it's saved).
pub fn check_text(path: &str, text: String) -> Vec<Diag> {
    check_file_seen(path, &mut HashSet::new(), Some(text))
}

/// `modules_done` holds the module files already checked, so each is reported once
/// (and two modules that use each other don't loop forever).
fn check_file_seen(path: &str, modules_done: &mut HashSet<PathBuf>, text: Option<String>) -> Vec<Diag> {
    modules_done.insert(Path::new(path).canonicalize().unwrap_or_else(|_| PathBuf::from(path)));
    let src = match text.map(Ok).unwrap_or_else(|| std::fs::read_to_string(path)) {
        Ok(s) => s,
        Err(e) => return vec![EzaError::syntax(0, format!("can't open the file: {}", e)).in_file(path)],
    };
    let first = match lexer::lex(&src).and_then(|t| parser::Parser::new(t).program()) {
        Ok(p) => p,
        Err(e) => return vec![e.in_file(path)],
    };
    let mut c = Ctx::default();
    let mut progs: Vec<(String, Vec<Stmt>)> = vec![(path.to_string(), first)];
    let mut seen: HashSet<PathBuf> = HashSet::new();
    seen.insert(Path::new(path).canonicalize().unwrap_or_else(|_| PathBuf::from(path)));
    let mut i = 0;
    while i < progs.len() {
        c.file = progs[i].0.clone();
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
                Err(_) => {
                    let mut d = EzaError::check(line, format!("can't find the included file \"{}\"", rel), false).in_file(&from);
                    d.label(Target::Expr(Expr::Str(rel.clone())), "", true);
                    c.diags.push(d);
                }
                Ok(text) => match lexer::lex(&text).and_then(|t| parser::Parser::new(t).program()) {
                    Ok(p) => progs.push((full.display().to_string(), p)),
                    Err(e) => c.diags.push(e.in_file(&full.display().to_string())),
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
                let mut d = EzaError::check(line, format!("can't find the module file \"{}\"", rel), false).in_file(&from);
                d.label(Target::Expr(Expr::Str(rel.clone())), "", true);
                c.diags.push(d);
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
                let more = check_file_seen(&full.display().to_string(), modules_done, None);
                c.diags.extend(more);
            }
        }
        i += 1;
    }
    for (file, prog) in &progs {
        c.file = file.clone();
        c.check_block(prog, &[]);
    }
    c.check_events();
    c.diags.extend(crate::types::check(&progs));
    // in the order they appear: file by file, top to bottom
    let order: Vec<String> = progs.iter().map(|(f, _)| f.clone()).collect();
    c.diags.sort_by_key(|d| (order.iter().position(|f| *f == d.file).unwrap_or(usize::MAX), d.line));
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
                    self.defs.entry(n.clone()).or_default().push(d.clone());
                    self.collect_func(d);
                } else {
                    self.plain.insert(n.clone());
                }
            }
            StmtKind::Define(d) => {
                self.names.insert(d.name.clone());
                self.defs.entry(d.name.clone()).or_default().push(d.clone());
                self.def_lines.insert(d.name.clone(), (d.line, self.file.clone()));
                self.collect_func(d);
            }
            StmtKind::Each(v, _, b) => {
                for n in each_names(v) {
                    self.names.insert(n.to_string());
                    self.binders.insert(n.to_string());
                }
                self.collect(b);
            }
            StmtKind::Unpack { names, .. } => {
                for n in names {
                    self.names.insert(n.clone());
                    self.plain.insert(n.clone());
                }
            }
            StmtKind::Serve { pages, .. } => {
                for p in pages {
                    for n in ["request".to_string(), "response".to_string()].into_iter().chain(crate::server::path_params(&p.path)) {
                        self.names.insert(n.clone());
                        self.binders.insert(n);
                    }
                    self.collect(&p.body);
                }
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
            StmtKind::OnTouch { names, body, .. } => {
                if let Some((x, y)) = names {
                    for n in [x, y].into_iter().filter(|n| !n.is_empty()) {
                        self.names.insert(n.clone());
                        self.binders.insert(n.clone());
                    }
                }
                self.collect(body);
            }
            StmtKind::OnEvent { name, var, body } => {
                self.listened.entry(name.clone()).or_insert((s.line, self.file.clone()));
                if let Some(v) = var {
                    self.names.insert(v.clone());
                    self.binders.insert(v.clone());
                }
                self.collect(body);
            }
            StmtKind::Trigger(name, _) => {
                self.triggered.entry(name.clone()).or_insert((s.line, self.file.clone()));
            }
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
        // sprite "hero" makes a name too
        if let Some(Expr::Str(l)) = &n.label {
            self.names.insert(l.replace(|c: char| !c.is_alphanumeric(), "_"));
        }
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

    /// A problem, pointing at `at` in its line, with an optional suggested fix.
    fn report(&mut self, line: usize, msg: String, at: Target, help: Option<String>, warning: bool) {
        let mut d = EzaError::check(line, msg, warning).in_file(&self.file);
        d.label(at, "", true);
        d.help.extend(help);
        self.diags.push(d);
    }

    fn use_name(&mut self, n: &str, line: usize) {
        if self.lenient || self.names.contains(n) || IMPLICIT.contains(&n) || methods::BUILTINS.contains(&n) {
            return;
        }
        let mut pool: Vec<String> = self.names.iter().cloned().collect();
        pool.extend(IMPLICIT.iter().map(|s| s.to_string()));
        pool.extend(methods::BUILTINS.iter().map(|s| s.to_string()));
        let close = suggest::closest(n, &pool);
        let hint = close.as_ref().map(|c| format!(" Did you mean '{}'?", c)).unwrap_or_default();
        let help = crate::diagnose::other_language_name(n).map(|h| h.to_string());
        self.report(line, format!("'{}' isn't created anywhere in this program.{}", n, hint), Target::Name(n.to_string()), help, false);
    }

    /// `initial` are names that already exist in this block's scope (function parameters, loop variables).
    fn check_block(&mut self, stmts: &[Stmt], initial: &[String]) {
        let mut seen: HashMap<String, usize> = initial.iter().map(|n| (n.clone(), 0)).collect();
        let mut ended_by: Option<&str> = None;
        for s in stmts {
            // nothing after a return / break / continue in the same block can ever run
            if let Some(word) = ended_by.take() {
                let mut d = EzaError::check(s.line, format!("this line can never run - it comes right after a '{}' in the same block", word), true).in_file(&self.file);
                d.help.push(format!("remove it, or move it above the '{}'", word));
                self.diags.push(d);
            }
            if let StmtKind::Unpack { names, create: true, .. } = &s.kind {
                for n in names {
                    if let Some(first) = seen.get(n).copied() {
                        let at = if first == 0 { String::new() } else { format!(" (line {})", first) };
                        self.report(s.line, format!("'{0}' already exists{1} - use 'change {0} to ...' to update it", n, at), Target::Name(n.clone()), None, false);
                    } else {
                        seen.insert(n.clone(), s.line);
                    }
                }
            }
            if let StmtKind::Assign(n, e) = &s.kind {
                let help = Some(format!("to give it a new value:   change {} to {}", n, crate::diagnose::code(e)));
                match seen.get(n) {
                    Some(0) => self.report(s.line, format!("'{0}' already exists here - use 'change {0} to ...' to update it", n), Target::Name(n.clone()), help, false),
                    Some(first) => {
                        let first = *first;
                        self.report(s.line, format!("'{0}' already exists (line {1}) - use 'change {0} to ...' to update it", n, first), Target::Name(n.clone()), help, false)
                    }
                    None => {
                        seen.insert(n.clone(), s.line);
                    }
                }
            }
            self.check_stmt(s);
            ended_by = match s.kind {
                StmtKind::Return(_) => Some("return"),
                StmtKind::Break => Some("break"),
                StmtKind::Continue => Some("continue"),
                _ => None,
            };
        }
    }

    fn check_func(&mut self, d: &FuncDef) {
        self.check_block(&d.body, &d.params);
        // a variable made inside the function that nothing ever reads is usually a mistake
        let mut made = vec![];
        assigned_in(&d.body, &mut made);
        if made.is_empty() {
            return;
        }
        let mut read = HashSet::new();
        names_in_stmts(&d.body, &mut read);
        let mut warned = HashSet::new();
        for (n, line) in made {
            if !read.contains(&n) && !n.starts_with('_') && warned.insert(n.clone()) {
                self.report(
                    line,
                    format!("'{}' is created but never used in {}", n, d.name),
                    Target::Name(n.clone()),
                    Some(format!("use it, or remove the line (start the name with _ if it's on purpose: _{})", n)),
                    true,
                );
            }
        }
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
                let names: Vec<String> = each_names(v).into_iter().map(String::from).collect();
                self.check_block(b, &names);
            }
            StmtKind::Serve { props, pages } => {
                props.iter().for_each(|(_, e)| self.check_expr(e, line));
                for p in pages {
                    p.props.iter().for_each(|(_, e)| self.check_expr(e, p.line));
                    let mut known = vec!["request".to_string(), "response".to_string()];
                    known.extend(crate::server::path_params(&p.path));
                    self.check_block(&p.body, &known);
                }
            }
            StmtKind::Unpack { names, value, create } => {
                self.check_expr(value, line);
                if !create {
                    for n in names {
                        self.use_name(n, line);
                    }
                }
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
            StmtKind::OnTouch { a, b, names, body, .. } => {
                self.check_expr(a, line);
                self.check_expr(b, line);
                let known: Vec<String> = names.iter().flat_map(|(x, y)| [x.clone(), y.clone()]).filter(|n| !n.is_empty()).collect();
                self.check_block(body, &known);
            }
            StmtKind::OnEvent { var, body, .. } => self.check_block(body, &var.iter().cloned().collect::<Vec<_>>()),
            StmtKind::Trigger(_, v) => v.iter().for_each(|e| self.check_expr(e, line)),
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
                        self.report(line, format!("can't find the script \"{}\" to go to", p), Target::Expr(e.clone()), Some("go to only finds scripts in the same folder as this one".into()), false);
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
            Expr::Binary(_, l, r) | Expr::Range(l, r) => {
                self.check_expr(l, line);
                self.check_expr(r, line);
            }
            Expr::List(items) => items.iter().for_each(|x| self.check_expr(x, line)),
            Expr::Dict(items) => items.iter().for_each(|(_, x)| self.check_expr(x, line)),
            Expr::Spawn { prefab, at, props, into } => {
                self.check_expr(prefab, line);
                for a in at.iter().chain(into.iter()) {
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
            self.report(line, format!("'{}' is private to the module {} (names starting with _ stay inside their file)", f, m), Target::Name(f.to_string()), Some(format!("add a function to {}.eza that gives it back", m)), false);
        } else if !names.contains(f) {
            let pool: Vec<String> = names.iter().filter(|n| !n.starts_with('_')).cloned().collect();
            let hint = suggest::closest(f, &pool).map(|c| format!(" Did you mean '{}'?", c)).unwrap_or_default();
            let mut public: Vec<String> = pool.clone();
            public.sort();
            public.truncate(12);
            let help = (!public.is_empty()).then(|| format!("{} has: {}", m, public.join(", ")));
            self.report(line, format!("the module {} has no '{}'.{}", m, f, hint), Target::Name(f.to_string()), help, false);
        }
    }

    /// `trigger "x"` with nothing listening (or `on event "x"` that nothing triggers) is usually a typo.
    fn check_events(&mut self) {
        let mut found = vec![];
        for (from, to, what) in [(&self.triggered, &self.listened, "nothing listens for"), (&self.listened, &self.triggered, "nothing ever triggers")] {
            let pool: Vec<String> = to.keys().cloned().collect();
            for (name, (line, file)) in from {
                if to.contains_key(name) {
                    continue;
                }
                let close = suggest::closest(name, &pool);
                let hint = close.as_ref().map(|c| format!(" Did you mean \"{}\"?", c)).unwrap_or_default();
                let help = if what == "nothing listens for" {
                    format!("add a block that reacts to it:   on event \"{}\"", name)
                } else {
                    format!("start it somewhere with:   trigger \"{}\"", name)
                };
                found.push((*line, file.clone(), format!("{} the event \"{}\".{}", what, name, hint), name.clone(), help));
            }
        }
        found.sort();
        for (line, file, msg, name, help) in found {
            let mut d = EzaError::check(line, msg, true).in_file(&file);
            d.label(Target::Expr(Expr::Str(name)), "", true);
            d.help.push(help);
            self.diags.push(d);
        }
    }

    fn check_call(&mut self, n: &str, args: &[Arg], line: usize) {
        // only when the name clearly means one function
        let Some(defs) = self.defs.get(n) else { return };
        if defs.len() != 1 || self.plain.contains(n) || self.binders.contains(n) {
            return;
        }
        let def = defs[0].clone();
        let params = def.params.clone();
        let positional = args.iter().filter(|a| a.name.is_none()).count();
        let usage = Some(format!("call it like:   {}({})", n, params.join(", ")));
        let mut problem = None;
        for a in args {
            if let Some(nm) = &a.name {
                if !params.contains(nm) {
                    problem = Some(format!("{} has no parameter called '{}' (it takes: {})", n, nm, params.join(", ")));
                    break;
                }
            }
        }
        if problem.is_none() && positional > params.len() {
            problem = Some(format!("{} takes {} argument(s) but you gave {}", n, params.len(), positional));
        }
        if problem.is_none() {
            for (i, p) in params.iter().enumerate() {
                let named = args.iter().any(|a| a.name.as_deref() == Some(p));
                let has_default = matches!(def.defaults.get(i), Some(Some(_)));
                if i >= positional && !named && !has_default {
                    problem = Some(format!("{} is missing the argument '{}'", n, p));
                    break;
                }
            }
        }
        let Some(msg) = problem else { return };
        self.report(line, msg, Target::Name(n.to_string()), usage, false);
        // and point at the define line
        if let Some((dl, df)) = self.def_lines.get(n).cloned() {
            if dl > 0 {
                if let Some(d) = self.diags.last_mut() {
                    let sig = if params.is_empty() { format!("define {}", n) } else { format!("define {}, {}", n, params.join(", ")) };
                    let file = if df == d.file { String::new() } else { df };
                    d.labels.push(crate::error::Label { line: dl, file, at: Target::Name(n.to_string()), msg: format!("defined here as:  {}", sig), primary: false });
                }
            }
        }
    }
}
