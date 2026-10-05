use crate::ast::*;
use crate::error::{EzaError, R};
use crate::lexer::{self, Tok, Token};
use std::rc::Rc;

pub struct Parser {
    t: Vec<Token>,
    p: usize,
}

fn describe(t: &Tok) -> String {
    match t {
        Tok::Num(n) => format!("number {}", n),
        Tok::Str(s) => format!("text \"{}\"", s),
        Tok::Color(c) => format!("color #{}", c),
        Tok::Ident(s) => format!("'{}'", s),
        Tok::Sym(s) => format!("'{}'", s),
        Tok::Newline => "end of line".into(),
        Tok::Indent => "an indented line".into(),
        Tok::Dedent => "end of block".into(),
        Tok::Eof => "end of file".into(),
    }
}

fn root_name(e: &Expr) -> Option<&str> {
    match e {
        Expr::Ident(n) => Some(n),
        Expr::Field(inner, _) | Expr::Index(inner, _) => root_name(inner),
        _ => None,
    }
}

/// The Purity Guard: inside `mimic`, only the shadow (and locals made inside the block) may change.
fn check_purity(body: &[Stmt], shadow: &str, locals: &mut Vec<String>) -> R<()> {
    for s in body {
        match &s.kind {
            StmtKind::Assign(n, _) => locals.push(n.clone()),
            StmtKind::Change(t, _, _) | StmtKind::Push(_, t) => {
                let root = root_name(t).unwrap_or("");
                if root != shadow && !locals.iter().any(|l| l == root) {
                    return Err(EzaError::syntax(
                        s.line,
                        format!("Cannot modify global variable '{}' inside an isolated simulation block.", root),
                    ));
                }
            }
            StmtKind::If(arms, els) => {
                for (_, b) in arms {
                    check_purity(b, shadow, locals)?;
                }
                if let Some(b) = els {
                    check_purity(b, shadow, locals)?;
                }
            }
            StmtKind::Each(v, _, b) => {
                locals.push(v.clone());
                check_purity(b, shadow, locals)?;
            }
            StmtKind::While(_, b) | StmtKind::Persist { body: b, .. } => check_purity(b, shadow, locals)?,
            StmtKind::Save { .. } => {
                return Err(EzaError::syntax(s.line, "save can't be used inside a mimic block (it would change the real world)"))
            }
            StmtKind::Wait { .. } => {
                return Err(EzaError::syntax(s.line, "wait can't be used inside a mimic block (a simulation runs instantly)"))
            }
            StmtKind::Destroy(_) => {
                return Err(EzaError::syntax(s.line, "destroy can't be used inside a mimic block (it would change the real world)"))
            }
            StmtKind::Play(..) | StmtKind::Stop(_) | StmtKind::Emit { .. } | StmtKind::Go(_) => {
                return Err(EzaError::syntax(s.line, "play, stop, emit and go can't be used inside a mimic block (a simulation is silent and invisible)"))
            }
            StmtKind::Attempt(a, e, h) => {
                check_purity(a, shadow, locals)?;
                locals.push(e.clone());
                check_purity(h, shadow, locals)?;
            }
            _ => {}
        }
    }
    Ok(())
}

/// "score: {score}" becomes "score: " + score. Use {{ and }} for literal braces.
fn interpolate(s: String, line: usize) -> R<Expr> {
    if !s.contains('{') && !s.contains('}') {
        return Ok(Expr::Str(s));
    }
    let c: Vec<char> = s.chars().collect();
    let (mut parts, mut lit, mut i) = (vec![], String::new(), 0);
    while i < c.len() {
        let next = c.get(i + 1).copied();
        if (c[i] == '{' && next == Some('{')) || (c[i] == '}' && next == Some('}')) {
            lit.push(c[i]);
            i += 2;
            continue;
        }
        if c[i] == '{' {
            let mut depth = 0;
            let mut j = i;
            while j < c.len() {
                match c[j] {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            if j >= c.len() {
                return Err(EzaError::syntax(line, "text has a '{' without a matching '}' (use {{ for a literal brace)"));
            }
            let inner: String = c[i + 1..j].iter().collect();
            let fix = |mut e: EzaError| {
                e.line = line;
                e
            };
            let mut p = Parser::new(lexer::lex(&inner).map_err(fix)?);
            let e = p.expr().map_err(fix)?;
            if !p.at_end() {
                return Err(EzaError::syntax(line, format!("couldn't understand {{{}}} inside text", inner)));
            }
            if !lit.is_empty() {
                parts.push(Expr::Str(std::mem::take(&mut lit)));
            }
            parts.push(e);
            i = j + 1;
            continue;
        }
        lit.push(c[i]);
        i += 1;
    }
    if !lit.is_empty() {
        parts.push(Expr::Str(lit));
    }
    let mut acc = Expr::Str(String::new());
    for p in parts {
        acc = Expr::Binary(Op::Add, Box::new(acc), Box::new(p));
    }
    Ok(acc)
}

impl Parser {
    pub fn new(t: Vec<Token>) -> Self {
        Parser { t, p: 0 }
    }
    fn peek(&self) -> &Tok {
        &self.t[self.p].tok
    }
    fn peek_n(&self, k: usize) -> &Tok {
        &self.t[(self.p + k).min(self.t.len() - 1)].tok
    }
    fn line(&self) -> usize {
        self.t[self.p].line
    }
    fn next(&mut self) -> Tok {
        let t = self.t[self.p].tok.clone();
        if self.p < self.t.len() - 1 {
            self.p += 1;
        }
        t
    }
    fn is_kw(&self, k: &str) -> bool {
        matches!(self.peek(), Tok::Ident(s) if s == k)
    }
    fn is_sym(&self, s: &str) -> bool {
        matches!(self.peek(), Tok::Sym(x) if *x == s)
    }
    fn peek1_sym(&self, s: &str) -> bool {
        matches!(self.peek_n(1), Tok::Sym(x) if *x == s)
    }
    fn eat_kw(&mut self, k: &str) -> bool {
        let yes = self.is_kw(k);
        if yes {
            self.next();
        }
        yes
    }
    fn eat_sym(&mut self, s: &str) -> bool {
        let yes = self.is_sym(s);
        if yes {
            self.next();
        }
        yes
    }
    fn err<T>(&self, msg: impl Into<String>) -> R<T> {
        Err(EzaError::syntax(self.line(), msg))
    }
    fn expect_kw(&mut self, k: &str) -> R<()> {
        if self.eat_kw(k) {
            Ok(())
        } else {
            self.err(format!("expected '{}' but found {}", k, describe(self.peek())))
        }
    }
    fn expect_sym(&mut self, s: &str) -> R<()> {
        if self.eat_sym(s) {
            Ok(())
        } else {
            self.err(format!("expected '{}' but found {}", s, describe(self.peek())))
        }
    }
    fn ident(&mut self) -> R<String> {
        if let Tok::Ident(s) = self.peek() {
            let s = s.clone();
            self.next();
            Ok(s)
        } else {
            self.err(format!("expected a name but found {}", describe(self.peek())))
        }
    }
    fn at_end(&self) -> bool {
        matches!(self.peek(), Tok::Newline | Tok::Eof | Tok::Dedent)
    }
    fn end_line(&mut self) -> R<()> {
        if matches!(self.peek(), Tok::Newline) {
            self.next();
            Ok(())
        } else if self.at_end() {
            Ok(())
        } else {
            self.err(format!("unexpected {} (expected end of line)", describe(self.peek())))
        }
    }

    pub fn program(&mut self) -> R<Vec<Stmt>> {
        let mut v = vec![];
        while !matches!(self.peek(), Tok::Eof) {
            if matches!(self.peek(), Tok::Newline) {
                self.next();
                continue;
            }
            if matches!(self.peek(), Tok::Indent) {
                return self.err("unexpected indentation");
            }
            v.push(self.statement()?);
        }
        Ok(v)
    }

    /// NEWLINE INDENT statements DEDENT
    fn indented_block(&mut self) -> R<Vec<Stmt>> {
        self.expect_newline()?;
        if !matches!(self.peek(), Tok::Indent) {
            return self.err("expected an indented block on the next line");
        }
        self.next();
        let mut v = vec![];
        while !matches!(self.peek(), Tok::Dedent | Tok::Eof) {
            if matches!(self.peek(), Tok::Newline) {
                self.next();
                continue;
            }
            v.push(self.statement()?);
        }
        if matches!(self.peek(), Tok::Dedent) {
            self.next();
        }
        Ok(v)
    }

    fn expect_newline(&mut self) -> R<()> {
        if matches!(self.peek(), Tok::Newline) {
            self.next();
            Ok(())
        } else {
            self.err(format!("unexpected {} (expected end of line)", describe(self.peek())))
        }
    }

    /// Either an indented block, or a single statement on the same line (after optional `then`).
    fn block(&mut self) -> R<Vec<Stmt>> {
        self.eat_kw("then");
        if matches!(self.peek(), Tok::Newline) {
            self.indented_block()
        } else {
            Ok(vec![self.statement()?])
        }
    }

    fn params(&mut self) -> R<Vec<String>> {
        let mut v = vec![];
        while let Tok::Ident(s) = self.peek() {
            if s == "then" {
                break;
            }
            v.push(self.ident()?);
            if !self.eat_sym(",") {
                break;
            }
        }
        Ok(v)
    }

    fn statement(&mut self) -> R<Stmt> {
        let line = self.line();
        let kw = if let Tok::Ident(s) = self.peek() { s.clone() } else { String::new() };
        let is_decl = !self.peek1_sym(".") && !self.peek1_sym("=") && !self.peek1_sym("(");
        let kind = match kw.as_str() {
            "if" => {
                self.next();
                let c = self.expr()?;
                let mut arms = vec![(c, self.block()?)];
                let mut els = None;
                while self.eat_kw("else") {
                    if self.eat_kw("if") {
                        let c = self.expr()?;
                        arms.push((c, self.block()?));
                    } else {
                        els = Some(self.block()?);
                        break;
                    }
                }
                StmtKind::If(arms, els)
            }
            "each" => {
                self.next();
                let v = self.ident()?;
                self.expect_kw("in")?;
                let e = self.expr()?;
                StmtKind::Each(v, e, self.block()?)
            }
            "while" => {
                self.next();
                let e = self.expr()?;
                StmtKind::While(e, self.block()?)
            }
            "define" => {
                self.next();
                let name = self.ident()?;
                self.eat_sym(",");
                let params = self.params()?;
                let body = Rc::new(self.block()?);
                StmtKind::Define(Rc::new(FuncDef::new(name, params, body)))
            }
            // `class` is another word for `data`, for people coming from Python
            "data" | "class" if is_decl && matches!(self.peek_n(1), Tok::Ident(_)) => {
                self.next();
                let name = self.ident()?;
                let parent = if self.eat_kw("from") { Some(self.ident()?) } else { None };
                self.expect_newline()?;
                if !matches!(self.peek(), Tok::Indent) {
                    return self.err(format!("{} needs indented fields, like:  name = \"\"", kw));
                }
                self.next();
                let (mut fields, mut methods) = (vec![], vec![]);
                while !matches!(self.peek(), Tok::Dedent | Tok::Eof) {
                    if matches!(self.peek(), Tok::Newline) {
                        self.next();
                        continue;
                    }
                    if self.is_kw("define") {
                        match self.statement()?.kind {
                            StmtKind::Define(d) => methods.push(d),
                            _ => return self.err("expected a function here, like:  define take_damage, amount"),
                        }
                        continue;
                    }
                    let f = self.ident()?;
                    self.expect_sym("=")?;
                    fields.push((f, self.expr()?));
                    self.end_line()?;
                }
                self.eat_dedent();
                StmtKind::Data(Rc::new(DataDecl { name, parent, fields, methods }))
            }
            "attempt" => {
                self.next();
                let body = self.block()?;
                self.expect_kw("handle")?;
                let name = match self.peek() {
                    Tok::Ident(s) if s != "then" => self.ident()?,
                    _ => "error".to_string(),
                };
                StmtKind::Attempt(body, name, self.block()?)
            }
            "on" => {
                self.next();
                let e = self.expr()?;
                StmtKind::On(e, Rc::new(self.block()?))
            }
            "persist" => self.persist()?,
            "mimic" => {
                self.next();
                let shadow = self.ident()?;
                self.expect_kw("to")?;
                let src = self.expr()?;
                let body = self.block()?;
                check_purity(&body, &shadow, &mut vec![])?;
                StmtKind::Mimic(shadow, src, Rc::new(body))
            }
            "test" if is_decl && matches!(self.peek_n(1), Tok::Str(_)) => {
                self.next();
                let name = match self.next() {
                    Tok::Str(s) => s,
                    _ => String::new(),
                };
                StmtKind::Test(name, self.indented_block()?)
            }
            "prefab" if is_decl => {
                self.next();
                let name = self.ident()?;
                self.expect_newline()?;
                if !matches!(self.peek(), Tok::Indent) {
                    return self.err("prefab needs an indented object, like:  sphere width=1 health=30");
                }
                self.next();
                let kind = self.ident()?;
                let node = self.decl_node(kind)?;
                while matches!(self.peek(), Tok::Newline) {
                    self.next();
                }
                self.eat_dedent();
                if !node.children.is_empty() {
                    return self.err("a prefab is a single object for now (no indented children)");
                }
                StmtKind::Prefab(name, node)
            }
            "style" if is_decl && matches!(self.peek_n(1), Tok::Ident(_)) && matches!(self.peek_n(2), Tok::Newline) => {
                self.next();
                let name = self.ident()?;
                self.expect_newline()?;
                if !matches!(self.peek(), Tok::Indent) {
                    return self.err("a style needs indented lines, like:  background = #050505");
                }
                self.next();
                let (mut props, mut states) = (vec![], vec![]);
                while !matches!(self.peek(), Tok::Dedent | Tok::Eof) {
                    if matches!(self.peek(), Tok::Newline) {
                        self.next();
                        continue;
                    }
                    if self.eat_kw("on") {
                        let state = self.ident()?;
                        self.expect_newline()?;
                        if !matches!(self.peek(), Tok::Indent) {
                            return self.err(format!("'on {}' needs indented lines under it", state));
                        }
                        self.next();
                        let mut items = vec![];
                        while !matches!(self.peek(), Tok::Dedent | Tok::Eof) {
                            if matches!(self.peek(), Tok::Newline) {
                                self.next();
                                continue;
                            }
                            items.push(self.style_line()?);
                        }
                        self.eat_dedent();
                        states.push((state, items));
                        continue;
                    }
                    props.push(self.style_line()?);
                }
                self.eat_dedent();
                StmtKind::Style(name, props, states)
            }
            "stage" if is_decl => {
                self.next();
                StmtKind::Stage(self.decl_node("stage".into())?)
            }
            "scene" if is_decl => {
                self.next();
                StmtKind::Scene(self.decl_node("scene".into())?)
            }
            "gui" if is_decl => {
                self.next();
                let kind = self.ident()?;
                StmtKind::Gui(self.decl_node(kind)?)
            }
            _ if matches!(self.peek_n(2), Tok::Ident(s) if s == "define") && self.peek1_sym("=") => {
                // vortex_effect = define entity  (inline function stored in a variable)
                let name = self.ident()?;
                self.next();
                self.next();
                let params = self.params()?;
                let body = Rc::new(self.block()?);
                let def = Rc::new(FuncDef::new(name.clone(), params, body));
                StmtKind::Assign(name, Expr::Lambda(def))
            }
            _ => {
                let k = self.simple()?;
                self.end_line()?;
                k
            }
        };
        Ok(Stmt { line, kind })
    }

    fn eat_dedent(&mut self) {
        if matches!(self.peek(), Tok::Dedent) {
            self.next();
        }
    }

    /// Single-line statements (no trailing newline consumed).
    fn simple(&mut self) -> R<StmtKind> {
        let kw = if let Tok::Ident(s) = self.peek() { s.clone() } else { String::new() };
        Ok(match kw.as_str() {
            "change" if !self.peek1_sym("=") => {
                self.next();
                let target = self.postfix()?;
                if root_name(&target).is_none() {
                    return self.err("change needs a variable, like: change score by 10");
                }
                let mode = if self.eat_kw("to") {
                    ChangeMode::To
                } else if self.eat_kw("by") {
                    ChangeMode::By
                } else {
                    return self.err(format!("expected 'to' or 'by' but found {}", describe(self.peek())));
                };
                StmtKind::Change(target, mode, self.expr()?)
            }
            "return" => {
                self.next();
                let mut v = vec![];
                if !self.at_end() {
                    v.push(self.expr()?);
                    while self.eat_sym(",") {
                        v.push(self.expr()?);
                    }
                }
                StmtKind::Return(v)
            }
            "rewind" => {
                self.next();
                let target = if self.is_kw("to") || self.is_kw("by") {
                    None
                } else {
                    let t = self.ident()?;
                    if t == "scene" || t == "all" { None } else { Some(t) }
                };
                let steps = if self.eat_kw("to") {
                    self.expect_kw("beginning")?;
                    None
                } else {
                    self.expect_kw("by")?;
                    let e = self.expr()?;
                    if !self.eat_kw("steps") {
                        self.eat_kw("step");
                    }
                    Some(e)
                };
                StmtKind::Rewind { target, steps }
            }
            "include" => {
                self.next();
                match self.next() {
                    Tok::Str(s) => StmtKind::Include(s),
                    t => return self.err(format!("include needs a file path in quotes, found {}", describe(&t))),
                }
            }
            "use" if !self.peek1_sym("=") && !self.peek1_sym("(") && !self.peek1_sym(".") => {
                self.next();
                // use "tools/enemies.eza", or just  use enemies  for enemies.eza next to this file
                let path = match self.next() {
                    Tok::Str(s) => s,
                    Tok::Ident(s) => format!("{}.eza", s),
                    t => return self.err(format!("use needs a file, like:  use \"enemies.eza\"  (found {})", describe(&t))),
                };
                let alias = if self.eat_kw("as") {
                    self.ident()?
                } else {
                    let stem = std::path::Path::new(&path).file_stem().and_then(|s| s.to_str()).unwrap_or("module");
                    let mut name: String = stem.chars().map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' }).collect();
                    if name.starts_with(|c: char| c.is_ascii_digit()) {
                        name.insert(0, '_');
                    }
                    name
                };
                StmtKind::Use { path, alias }
            }
            "param" => {
                self.next();
                let name = self.ident()?;
                self.expect_sym("=")?;
                StmtKind::Param(name, self.expr()?)
            }
            "save" | "append" if !self.peek1_sym("=") && !self.peek1_sym("(") => {
                let append = kw == "append";
                self.next();
                let value = self.expr()?;
                self.expect_kw("to")?;
                let path = self.expr()?;
                StmtKind::Save { value, path, append }
            }
            "expect" if !self.peek1_sym("=") && !self.peek1_sym("(") => {
                self.next();
                StmtKind::Expect(self.expr()?)
            }
            "play" if !self.peek1_sym("=") && !self.peek1_sym("(") && !self.peek1_sym(".") => {
                self.next();
                let path = self.expr()?;
                let mut props = vec![];
                while matches!(self.peek(), Tok::Ident(_)) && self.peek1_sym("=") {
                    let k = self.ident()?;
                    self.next();
                    props.push((k, self.expr()?));
                }
                StmtKind::Play(path, props)
            }
            "stop" if !self.peek1_sym("=") && !self.peek1_sym("(") && !self.peek1_sym(".") => {
                self.next();
                if self.at_end() || (self.is_kw("all") && matches!(self.peek_n(1), Tok::Newline | Tok::Eof | Tok::Dedent)) {
                    self.eat_kw("all");
                    StmtKind::Stop(None)
                } else {
                    StmtKind::Stop(Some(self.expr()?))
                }
            }
            "emit" if !self.peek1_sym("=") && !self.peek1_sym("(") && !self.peek1_sym(".") => {
                self.next();
                let count = self.expr()?;
                self.expect_kw("from")?;
                let from = self.postfix()?;
                let at = if self.eat_kw("at") {
                    let first = self.expr()?;
                    if self.is_sym(",") {
                        let mut v = vec![first];
                        while self.eat_sym(",") {
                            v.push(self.expr()?);
                        }
                        Some(Expr::List(v))
                    } else {
                        Some(first)
                    }
                } else {
                    None
                };
                StmtKind::Emit { count, from, at }
            }
            "go" if matches!(self.peek_n(1), Tok::Ident(s) if s == "to") => {
                self.next();
                self.next();
                StmtKind::Go(self.expr()?)
            }
            "push" if !self.peek1_sym("=") && !self.peek1_sym("(") => {
                self.next();
                let value = self.expr()?;
                self.expect_kw("to")?;
                let target = self.postfix()?;
                if root_name(&target).is_none() {
                    return self.err("push needs a variable, like: push 5 to stack");
                }
                StmtKind::Push(value, target)
            }
            "destroy" => {
                self.next();
                StmtKind::Destroy(self.postfix()?)
            }
            "wait" if !self.peek1_sym("=") => {
                self.next();
                let steps = self.expr()?;
                let seconds = if self.eat_kw("seconds") || self.eat_kw("second") {
                    true
                } else {
                    if !self.eat_kw("steps") {
                        self.eat_kw("step");
                    }
                    false
                };
                StmtKind::Wait { steps, seconds }
            }
            "break" => {
                self.next();
                StmtKind::Break
            }
            "continue" => {
                self.next();
                StmtKind::Continue
            }
            "tween" if !self.peek1_sym("=") => {
                // tween player.position.x to 10 over 120 steps [ease ease_in_out]
                self.next();
                let target = self.postfix()?;
                if root_name(&target).is_none() {
                    return self.err("tween needs a variable, like: tween player.position.x to 10 over 60 steps");
                }
                self.expect_kw("to")?;
                let value = self.expr()?;
                self.expect_kw("over")?;
                let steps = self.expr()?;
                if !self.eat_kw("steps") {
                    self.eat_kw("step");
                }
                let ease = if self.eat_kw("ease") { self.ident()? } else { "ease_in_out".to_string() };
                if !["linear", "ease_in", "ease_out", "ease_in_out"].contains(&ease.as_str()) {
                    return self.err(format!("unknown ease '{}' (use linear, ease_in, ease_out or ease_in_out)", ease));
                }
                StmtKind::Tween { target, value, steps, ease }
            }
            "tick" if !self.peek1_sym("=") => {
                self.next();
                if self.at_end() { StmtKind::Tick(None) } else { StmtKind::Tick(Some(self.expr()?)) }
            }
            _ if matches!(self.peek(), Tok::Ident(_)) && self.peek1_sym("=") => {
                let name = self.ident()?;
                self.next();
                StmtKind::Assign(name, self.expr()?)
            }
            _ => StmtKind::Expr(self.expr()?),
        })
    }

    fn persist(&mut self) -> R<StmtKind> {
        self.next();
        let mut body = vec![];
        if matches!(self.peek(), Tok::Newline) {
            body = self.indented_block()?;
        } else {
            // inline: persist change a to 1 change b by 2 for 45 steps
            while !self.is_kw("for") && !self.is_kw("until") && !self.at_end() {
                let line = self.line();
                body.push(Stmt { line, kind: self.simple()? });
            }
        }
        let (mut steps, mut until) = (None, None);
        if self.eat_kw("for") {
            steps = Some(self.expr()?);
            if !self.eat_kw("steps") {
                self.eat_kw("step");
            }
        } else if self.eat_kw("until") {
            let e = self.expr()?;
            if self.is_kw("steps") || self.is_kw("step") {
                self.next();
                // `until cond or 120 steps`
                match e {
                    Expr::Binary(Op::Or, l, r) => {
                        until = Some(*l);
                        steps = Some(*r);
                    }
                    other => steps = Some(other),
                }
            } else {
                until = Some(e);
            }
        }
        // no ending: lasts while the surrounding `on` condition stays true
        self.end_line()?;
        Ok(StmtKind::Persist { body, steps, until })
    }

    fn decl_node(&mut self, kind: String) -> R<DeclNode> {
        let mut node = DeclNode { kind, label: None, props: vec![], children: vec![], handler: None, events: vec![] };
        if let Tok::Str(s) = self.peek() {
            let (s, line) = (s.clone(), self.line());
            self.next();
            node.label = Some(interpolate(s, line)?);
        }
        while let Tok::Ident(name) = self.peek() {
            if name == "then" {
                break;
            }
            let key = self.style_key()?;
            if !self.eat_sym("=") {
                return self.err(format!("expected {}=value", key));
            }
            let v = self.prop_value()?;
            node.props.push((key, v));
        }
        if self.eat_kw("then") {
            if matches!(self.peek(), Tok::Newline) {
                node.handler = Some(Rc::new(self.indented_block()?));
                return Ok(node);
            }
            let line = self.line();
            let k = self.simple()?;
            node.handler = Some(Rc::new(vec![Stmt { line, kind: k }]));
        }
        if self.at_end() && !matches!(self.peek(), Tok::Newline) {
            return Ok(node);
        }
        self.expect_newline()?;
        if matches!(self.peek(), Tok::Indent) {
            self.next();
            while !matches!(self.peek(), Tok::Dedent | Tok::Eof) {
                if matches!(self.peek(), Tok::Newline) {
                    self.next();
                    continue;
                }
                if self.is_kw("on") {
                    let s = self.statement()?;
                    node.events.push(s);
                    continue;
                }
                let kind = self.ident()?;
                node.children.push(self.decl_node(kind)?);
            }
            self.eat_dedent();
        }
        Ok(node)
    }

    /// A property name; CSS-style `background-color` is accepted and becomes `background`.
    fn style_key(&mut self) -> R<String> {
        let mut k = self.ident()?;
        while self.is_sym("-") && matches!(self.peek_n(1), Tok::Ident(_)) {
            self.next();
            k.push('_');
            k.push_str(&self.ident()?);
        }
        let k = k.to_lowercase();
        Ok(match k.as_str() {
            "background_color" | "bg" | "bg_color" => "background",
            "border_round" | "border_radius" | "radius" | "corner_radius" | "round" => "rounded",
            "text_size" => "font_size",
            "font_family" => "font",
            _ => return Ok(k),
        }
        .to_string())
    }

    /// `key = value` inside a style block
    fn style_line(&mut self) -> R<(String, Expr)> {
        let k = self.style_key()?;
        self.expect_sym("=")?;
        let v = self.prop_value()?;
        self.end_line()?;
        Ok((k, v))
    }

    /// A prop word; `sans-serif` stays one word.
    fn prop_word(&mut self) -> R<Expr> {
        let was_ident = matches!(self.peek(), Tok::Ident(_));
        let mut e = self.prop_atom()?;
        if was_ident {
            if let Expr::Str(s) = &mut e {
                while self.is_sym("-") && matches!(self.peek_n(1), Tok::Ident(_)) {
                    self.next();
                    s.push('-');
                    s.push_str(&self.ident()?);
                }
            }
        }
        Ok(e)
    }

    /// Values in `key=value` props: numbers, text, colors, true/false, bare words, or `1,2,3` vectors.
    fn prop_value(&mut self) -> R<Expr> {
        let first = self.prop_word()?;
        if !self.is_sym(",") {
            return Ok(first);
        }
        let mut v = vec![first];
        while self.eat_sym(",") {
            v.push(self.prop_word()?);
        }
        Ok(Expr::List(v))
    }

    fn prop_atom(&mut self) -> R<Expr> {
        let line = self.line();
        Ok(match self.next() {
            Tok::Num(n) => Expr::Num(n),
            Tok::Str(s) => Expr::Str(s),
            Tok::Color(c) => Expr::Color(c),
            Tok::Ident(s) => match s.as_str() {
                "true" => Expr::Bool(true),
                "false" => Expr::Bool(false),
                _ => Expr::Str(s),
            },
            Tok::Sym("[") => {
                let mut items = vec![];
                while !self.is_sym("]") {
                    items.push(self.expr()?);
                    if !self.eat_sym(",") {
                        break;
                    }
                }
                self.expect_sym("]")?;
                Expr::List(items)
            }
            Tok::Sym("-") => match self.next() {
                Tok::Num(n) => Expr::Num(-n),
                t => return Err(EzaError::syntax(line, format!("expected a number after '-', found {}", describe(&t)))),
            },
            t => return Err(EzaError::syntax(line, format!("expected a value, found {}", describe(&t)))),
        })
    }

    // ---------- expressions ----------

    /// `x -> x * 2` or `(a, b) -> a + b`: a short function that gives back one value.
    fn arrow_params(&self) -> Option<(Vec<String>, usize)> {
        if let (Tok::Ident(n), Tok::Sym("->")) = (self.peek(), self.peek_n(1)) {
            return Some((vec![n.clone()], 2));
        }
        if !self.is_sym("(") {
            return None;
        }
        let (mut names, mut k) = (vec![], 1);
        loop {
            match self.peek_n(k) {
                Tok::Ident(n) => names.push(n.clone()),
                Tok::Sym(")") if names.is_empty() => break,
                _ => return None,
            }
            k += 1;
            match self.peek_n(k) {
                Tok::Sym(",") => k += 1,
                Tok::Sym(")") => break,
                _ => return None,
            }
        }
        matches!(self.peek_n(k + 1), Tok::Sym("->")).then_some((names, k + 2))
    }

    pub fn expr(&mut self) -> R<Expr> {
        if let Some((params, skip)) = self.arrow_params() {
            let line = self.line();
            for _ in 0..skip {
                self.next();
            }
            let body = self.expr()?;
            let ret = Stmt { line, kind: StmtKind::Return(vec![body]) };
            return Ok(Expr::Lambda(Rc::new(FuncDef::new("function".into(), params, Rc::new(vec![ret])))));
        }
        let mut l = self.and()?;
        while self.eat_kw("or") {
            let r = self.and()?;
            l = Expr::Binary(Op::Or, Box::new(l), Box::new(r));
        }
        Ok(l)
    }
    fn and(&mut self) -> R<Expr> {
        let mut l = self.not()?;
        while self.eat_kw("and") {
            let r = self.not()?;
            l = Expr::Binary(Op::And, Box::new(l), Box::new(r));
        }
        Ok(l)
    }
    fn not(&mut self) -> R<Expr> {
        if self.eat_kw("not") {
            Ok(Expr::Unary("not", Box::new(self.not()?)))
        } else {
            self.cmp()
        }
    }
    fn cmp(&mut self) -> R<Expr> {
        let mut l = self.bor()?;
        loop {
            let op = match self.peek() {
                Tok::Sym(s) if ["==", "!=", "<", ">", "<=", ">="].contains(s) => *s,
                _ => break,
            };
            self.next();
            let r = self.bor()?;
            l = Expr::Binary(Op::from_str(op).expect("known operator"), Box::new(l), Box::new(r));
        }
        Ok(l)
    }
    fn bin_level(&mut self, ops: &[&str], next: fn(&mut Self) -> R<Expr>) -> R<Expr> {
        let mut l = next(self)?;
        loop {
            let op = match self.peek() {
                Tok::Sym(s) if ops.contains(s) => *s,
                _ => break,
            };
            self.next();
            let r = next(self)?;
            l = Expr::Binary(Op::from_str(op).expect("known operator"), Box::new(l), Box::new(r));
        }
        Ok(l)
    }
    // bitwise: | lowest, then ^, then &, then << >>
    fn bor(&mut self) -> R<Expr> {
        self.bin_level(&["|"], Self::bxor)
    }
    fn bxor(&mut self) -> R<Expr> {
        self.bin_level(&["^"], Self::band)
    }
    fn band(&mut self) -> R<Expr> {
        self.bin_level(&["&"], Self::shift)
    }
    fn shift(&mut self) -> R<Expr> {
        self.bin_level(&["<<", ">>"], Self::add)
    }
    fn add(&mut self) -> R<Expr> {
        let mut l = self.mul()?;
        loop {
            let op = match self.peek() {
                Tok::Sym(s) if *s == "+" || *s == "-" => *s,
                _ => break,
            };
            self.next();
            let r = self.mul()?;
            l = Expr::Binary(Op::from_str(op).expect("known operator"), Box::new(l), Box::new(r));
        }
        Ok(l)
    }
    fn mul(&mut self) -> R<Expr> {
        let mut l = self.unary()?;
        loop {
            let op = match self.peek() {
                Tok::Sym(s) if ["*", "/", "%"].contains(s) => *s,
                _ => break,
            };
            self.next();
            let r = self.unary()?;
            l = Expr::Binary(Op::from_str(op).expect("known operator"), Box::new(l), Box::new(r));
        }
        Ok(l)
    }
    fn unary(&mut self) -> R<Expr> {
        if self.eat_sym("~") {
            return Ok(Expr::Unary("~", Box::new(self.unary()?)));
        }
        if self.eat_sym("-") {
            Ok(Expr::Unary("-", Box::new(self.unary()?)))
        } else {
            self.postfix()
        }
    }
    fn postfix(&mut self) -> R<Expr> {
        let mut e = self.primary()?;
        loop {
            if self.eat_sym(".") {
                let name = self.ident()?;
                e = Expr::Field(Box::new(e), name);
            } else if self.eat_sym("(") {
                let mut args = vec![];
                while !self.is_sym(")") {
                    let name = if matches!(self.peek(), Tok::Ident(_)) && self.peek1_sym("=") {
                        let n = self.ident()?;
                        self.next();
                        Some(n)
                    } else {
                        None
                    };
                    args.push(Arg { name, value: self.expr()? });
                    if !self.eat_sym(",") {
                        break;
                    }
                }
                self.expect_sym(")")?;
                e = Expr::Call(Box::new(e), args);
            } else if self.eat_sym("[") {
                let idx = self.expr()?;
                self.expect_sym("]")?;
                e = Expr::Index(Box::new(e), Box::new(idx));
            } else {
                break;
            }
        }
        Ok(e)
    }
    /// After `load` / `pop`: is the next token the start of a value (rather than an operator word)?
    fn value_follows(&self) -> bool {
        const WORDS: [&str; 13] = ["and", "or", "not", "then", "in", "to", "by", "if", "else", "at", "over", "for", "until"];
        match self.peek() {
            Tok::Str(_) | Tok::Sym("(") => true,
            Tok::Ident(s) => !WORDS.contains(&s.as_str()),
            _ => false,
        }
    }

    /// spawn Goblin at 5,0,3 health=50   (the `spawn` word is already consumed)
    fn spawn_expr(&mut self) -> R<Expr> {
        let name = self.ident()?;
        let at = if self.eat_kw("at") {
            let first = self.expr()?;
            if self.is_sym(",") {
                let mut v = vec![first];
                while self.eat_sym(",") {
                    v.push(self.expr()?);
                }
                Some(Box::new(Expr::List(v)))
            } else {
                Some(Box::new(first))
            }
        } else {
            None
        };
        let mut props = vec![];
        while matches!(self.peek(), Tok::Ident(_)) && self.peek1_sym("=") {
            let k = self.ident()?;
            self.next();
            props.push((k, self.expr()?));
        }
        Ok(Expr::Spawn { prefab: Box::new(Expr::Ident(name)), at, props })
    }

    fn primary(&mut self) -> R<Expr> {
        let line = self.line();
        Ok(match self.next() {
            Tok::Num(n) => Expr::Num(n),
            Tok::Str(s) => interpolate(s, line)?,
            Tok::Color(c) => Expr::Color(c),
            Tok::Ident(s) => match s.as_str() {
                "true" => Expr::Bool(true),
                "false" => Expr::Bool(false),
                "none" => Expr::None,
                "spawn" if matches!(self.peek(), Tok::Ident(_)) => self.spawn_expr()?,
                "load" if self.value_follows() => Expr::Load(Box::new(self.bor()?)),
                "pop" if self.value_follows() && !matches!(self.peek(), Tok::Str(_)) => Expr::Pop(Box::new(self.postfix()?)),
                _ => Expr::Ident(s),
            },
            Tok::Sym("(") => {
                let e = self.expr()?;
                self.expect_sym(")")?;
                e
            }
            Tok::Sym("{") => {
                let mut items = vec![];
                while !self.is_sym("}") {
                    let key = match self.next() {
                        Tok::Ident(s) | Tok::Str(s) => s,
                        Tok::Num(n) => crate::value::fmt_num(n),
                        t => return Err(EzaError::syntax(line, format!("a dictionary key must be a name or text, found {}", describe(&t)))),
                    };
                    self.expect_sym(":")?;
                    items.push((key, self.expr()?));
                    if !self.eat_sym(",") {
                        break;
                    }
                }
                self.expect_sym("}")?;
                Expr::Dict(items)
            }
            Tok::Sym("[") => {
                let mut items = vec![];
                while !self.is_sym("]") {
                    items.push(self.expr()?);
                    if !self.eat_sym(",") {
                        break;
                    }
                }
                self.expect_sym("]")?;
                Expr::List(items)
            }
            t => return Err(EzaError::syntax(line, format!("expected a value but found {}", describe(&t)))),
        })
    }
}
