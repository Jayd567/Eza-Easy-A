//! Explaining runtime errors: which values were involved, how they got that way (from the
//! history every change is recorded in), and what would fix it.
use super::*;
use crate::diagnose::code;
use crate::error::{Label, Target};

/// Values longer than this are cut short in messages.
const SHOW: usize = 90;
/// How many past changes to list for one value.
const CHANGES: usize = 3;

pub(super) fn short(v: &Value) -> String {
    let s = v.repr();
    if s.chars().count() > SHOW {
        s.chars().take(SHOW - 3).collect::<String>() + "..."
    } else {
        s
    }
}

/// "a number", "text", "a list", ... for messages.
pub(super) fn kind_of(v: &Value) -> String {
    match v {
        Value::None => "none (no value)".into(),
        Value::Bool(_) => "true/false".into(),
        Value::Num(_) => "a number".into(),
        Value::Str(_) => "text".into(),
        Value::List(_) => "a list".into(),
        Value::Obj(o) if o.type_name == "dict" => "a dictionary".into(),
        Value::Color(_) => "a color".into(),
        Value::Func(_) | Value::Native(_) => "a function".into(),
        other => format!("a {}", other.type_name()),
    }
}

/// `enemy.stats["hp"]` -> "enemy.stats.hp"
fn path_text(root: &str, path: &[PathEl]) -> String {
    let mut s = root.to_string();
    for p in path {
        match p {
            PathEl::Field(f) => {
                s.push('.');
                s.push_str(f);
            }
            PathEl::Index(Value::Num(n)) => s.push_str(&format!("[{}]", fmt_num(*n))),
            PathEl::Index(k) => s.push_str(&format!("[{}]", k.repr())),
        }
    }
    s
}

/// Is this code simple enough to look up without running anything (names, .fields, [literals])?
fn is_place(e: &Expr) -> bool {
    match e {
        Expr::Ident(_) => true,
        Expr::Field(o, _) => is_place(o),
        Expr::Index(o, i) => is_place(o) && matches!(&**i, Expr::Num(_) | Expr::Str(_) | Expr::Ident(_)),
        _ => false,
    }
}

/// One past change of a value.
struct Change {
    value: Value,
    at: Origin,
    created: bool,
    name: String,
}

impl Interp {
    /// The variable and path a piece of code reads, like `enemy.stats.defense`, when it's that simple.
    pub(super) fn place_of(&self, e: &Expr, scope: &Rc<Scope>) -> Option<(VarRef, Vec<PathEl>, String)> {
        match e {
            Expr::Ident(n) => Some((scope.lookup(n)?, vec![], n.clone())),
            Expr::Field(o, f) => {
                let (var, mut path, root) = self.place_of(o, scope)?;
                path.push(PathEl::Field(f.clone()));
                Some((var, path, root))
            }
            Expr::Index(o, i) => {
                let key = match &**i {
                    Expr::Num(n) => Value::Num(*n),
                    Expr::Str(s) => Value::Str(s.clone()),
                    Expr::Ident(n) => scope.value_of(n)?,
                    _ => return None,
                };
                let (var, mut path, root) = self.place_of(o, scope)?;
                path.push(PathEl::Index(key));
                Some((var, path, root))
            }
            _ => None,
        }
    }

    /// What a piece of code is right now, read without running anything again.
    pub(super) fn peek(&self, e: &Expr, scope: &Rc<Scope>) -> Option<Value> {
        if !is_place(e) {
            return None;
        }
        let (var, path, _) = self.place_of(e, scope)?;
        let var = if path.is_empty() { var } else { deref_var(var) };
        let v = var.borrow().get().clone();
        get_path(&deref_val(v), &path, 0).ok()
    }

    /// The recorded changes of the value at `path` in `var`, newest first.
    fn changes(&self, var: &VarRef, path: &[PathEl], name: &str, out: &mut Vec<Change>, total: &mut usize, depth: usize) {
        let var = if path.is_empty() { var.clone() } else { deref_var(var.clone()) };
        let v = var.borrow();
        let at_path = |i: usize| get_path(&deref_val(v.step(i).0.clone()), path, 0).ok();
        let mut i = v.idx;
        let reached_start;
        loop {
            let cur = at_path(i);
            let prev = if i > 0 { at_path(i - 1) } else { None };
            let changed = match (&cur, &prev) {
                (Some(a), Some(b)) => !equals(a, b),
                (Some(_), None) => true,
                _ => false,
            };
            if changed {
                *total += 1;
                if out.len() < CHANGES {
                    out.push(Change { value: cur.clone().unwrap(), at: v.step(i).1, created: i == 0, name: name.to_string() });
                }
            }
            // the value didn't exist before this step (a property added later): nothing older to find
            if cur.is_none() || i == 0 {
                reached_start = i == 0 && cur.is_some();
                break;
            }
            i -= 1;
        }
        // made as a copy of something else (a loop's list item, a method's object): follow it
        if !reached_start || depth >= 4 {
            return;
        }
        let Some((src, index)) = v.source.clone() else { return };
        drop(v);
        let mut p = src.path.clone();
        let mut n = src.name.clone();
        if let Some(ix) = index {
            p.push(PathEl::Index(Value::Num(ix as f64)));
            n = format!("{}[{}]", n, ix);
        }
        // it wasn't really made where this copy was: look at the original instead
        if out.last().is_some_and(|c| c.created) {
            out.pop();
            *total -= 1;
        }
        p.extend(path.iter().cloned());
        self.changes(&src.var, &p, &path_text(&n, path), out, total, depth + 1);
    }

    fn where_text(&self, at: Origin) -> String {
        let file = self.file_name(at.file);
        let here = file == self.file_name(self.cur_file) || file.is_empty();
        let place = if here { format!("line {}", at.line) } else { format!("{}:{}", file, at.line) };
        if self.frame_mode && at.frame > 0 {
            format!("{} (frame {})", place, at.frame)
        } else {
            place
        }
    }

    /// Lines saying how the value of `e` got to be what it is, plus a label for where it last changed.
    pub(super) fn history(&self, e: &Expr, scope: &Rc<Scope>, err_line: usize) -> (Vec<String>, Option<Label>) {
        if !is_place(e) {
            return (vec![], None);
        }
        let Some((var, path, root)) = self.place_of(e, scope) else { return (vec![], None) };
        let (mut found, mut total) = (vec![], 0);
        self.changes(&var, &path, &path_text(&root, &path), &mut found, &mut total, 0);
        // only made a few lines up and never changed: plain to see already
        if let [c] = found.as_slice() {
            if c.created && self.file_name(c.at.file) == self.file_name(self.cur_file) && (c.at.line as usize).abs_diff(err_line) <= 3 {
                return (vec![], None);
            }
        }
        let mut lines = vec![];
        let mut label = None;
        for c in &found {
            let line = if c.created {
                format!("{} was {} when it was created, at {}", c.name, short(&c.value), self.where_text(c.at))
            } else {
                format!("{} became {} at {}", c.name, short(&c.value), self.where_text(c.at))
            };
            lines.push(line);
            // point at the most recent change in the code too, if it's somewhere else
            if label.is_none() && c.at.line as usize != err_line && c.at.line > 0 {
                let file = self.file_name(c.at.file).to_string();
                let last = match path.last() {
                    Some(PathEl::Field(f)) => f.clone(),
                    _ => root.clone(),
                };
                label = Some(Label {
                    line: c.at.line as usize,
                    file,
                    at: Target::Name(last),
                    msg: if c.created { format!("{} was set to {} here", c.name, short(&c.value)) } else { format!("{} became {} here", c.name, short(&c.value)) },
                    primary: false,
                });
            }
        }
        if total > found.len() {
            lines.push(format!("...and {} earlier change(s)", total - found.len()));
        }
        (lines, label)
    }

    /// Notes for each simple piece of code in `parts`: its value, the object it belongs to, and its history.
    fn involve(&self, err: &mut EzaError, parts: &[&Expr], scope: &Rc<Scope>) {
        let mut shown_roots: Vec<String> = vec![];
        let mut pointed = false;
        for e in parts {
            if !is_place(e) {
                continue;
            }
            let (hist, label) = self.history(e, scope, err.line);
            // the whole object it belongs to, once: enemy = Enemy(name: "Orc", hp: 30, ...)
            if let Some((var, path, root)) = self.place_of(e, scope) {
                if !path.is_empty() && !shown_roots.contains(&root) {
                    let v = deref_val(var.borrow().get().clone());
                    if matches!(v, Value::Obj(_) | Value::List(_)) {
                        err.notes.push(format!("{} = {}", root, short(&v)));
                        shown_roots.push(root);
                    }
                }
            }
            err.notes.extend(hist.into_iter().map(|h| format!("  {}", h)));
            // only the first (most important) value gets pointed at in the code
            if let (Some(l), false) = (label, pointed) {
                err.labels.push(l);
                pointed = true;
            }
        }
    }

    // ---------- the kinds of error ----------

    /// `a / b`, `a - b`, `a < b` ... went wrong.
    pub(super) fn explain_binop(&self, mut err: EzaError, op: Op, l: &Expr, r: &Expr, a: &Value, b: &Value, whole: &Expr, scope: &Rc<Scope>) -> EzaError {
        if err.explained {
            return err;
        }
        err.explained = true;
        let (lc, rc) = (code(l), code(r));
        if err.msg.contains("divide by zero") {
            err.label(Target::Expr(whole.clone()), "", true);
            err.label(Target::Expr(r.clone()), "this is 0", false);
            err.notes.push(format!("{} is 0, so {} has no answer.", rc, code(whole)));
            self.involve(&mut err, &[r], scope);
            err.help.push(format!("check it first:   if {} != 0", rc));
            if op == Op::Div {
                err.help.push(format!("never divide by less than 1:   {} / max({}, 1)", lc, rc));
            }
            return err;
        }
        err.label(Target::Expr(whole.clone()), "", true);
        err.label(Target::Expr(l.clone()), format!("{}: {}", kind_of(a), short(a)), false);
        err.label(Target::Expr(r.clone()), format!("{}: {}", kind_of(b), short(b)), false);
        self.involve(&mut err, &[l, r], scope);
        let numeric_text = |v: &Value| matches!(v, Value::Str(s) if s.trim().parse::<f64>().is_ok());
        for (v, c) in [(a, &lc), (b, &rc)] {
            if matches!(v, Value::None) {
                err.notes.insert(0, format!("{} is none - it doesn't have a value yet.", c));
                err.help.push(format!("give {} a value before this line, or check first:   if {} != none", c, c));
            } else if numeric_text(v) && !matches!(op, Op::Eq | Op::Ne) {
                err.help.push(format!("{} is text that holds a number - turn it into a number first:   num({})", c, c));
            }
        }
        match (op, a, b) {
            (Op::Add, Value::Num(_), Value::List(_)) | (Op::Add, Value::List(_), Value::Num(_)) => {
                err.help.push(format!("to add an item to a list, use:   change {} by item   (or {} + [item])", if matches!(a, Value::List(_)) { &lc } else { &rc }, if matches!(a, Value::List(_)) { &lc } else { &rc }));
            }
            (Op::BitAnd | Op::BitOr | Op::BitXor | Op::Shl | Op::Shr, _, _) => {
                err.help.push(format!("round them first:   {}.round {} {}.round", lc, op.as_str(), rc));
            }
            (_, Value::Num(_), Value::Str(_)) | (_, Value::Str(_), Value::Num(_)) if err.help.is_empty() && op.is_comparison() => {
                let text = if matches!(a, Value::Str(_)) { &lc } else { &rc };
                err.help.push(format!("compare numbers with numbers:   num({})", text));
            }
            _ => {}
        }
        err
    }

    /// `list[i]` or `dict["key"]` went wrong.
    pub(super) fn explain_index(&self, mut err: EzaError, obj: &Expr, idx: &Expr, o: &Value, i: &Value, scope: &Rc<Scope>) -> EzaError {
        if err.explained {
            return err;
        }
        err.explained = true;
        let (oc, ic) = (code(obj), code(idx));
        let len = match o {
            Value::List(l) => Some(l.len()),
            Value::Str(s) => Some(s.chars().count()),
            _ => None,
        };
        match (len, i) {
            (Some(n), Value::Num(k)) if err.msg.contains("out of range") => {
                let what = if matches!(o, Value::Str(_)) { "letters" } else { "items" };
                err.label(Target::Expr(idx.clone()), format!("this is {}", fmt_num(*k)), true);
                if n == 0 {
                    err.notes.push(format!("{} is empty, so there's nothing at any position.", oc));
                    err.help.push(format!("check first:   if len({}) > 0", oc));
                } else {
                    err.notes.push(format!("{} has {} {}: positions 0 to {} (or -1 to -{} counting from the end).", oc, n, what, n - 1, n));
                    if *k == n as f64 {
                        err.notes.push("Positions start at 0, so the last one is one less than the length.".into());
                        err.help.push(format!("the last one is   {}[{}]   or   {}[-1]", oc, n - 1, oc));
                    } else {
                        err.help.push(format!("check first:   if {} < len({})", ic, oc));
                    }
                }
                self.involve(&mut err, &[idx], scope);
            }
            _ if err.msg.contains("no key") => {
                err.label(Target::Expr(idx.clone()), "", true);
                if let Value::Obj(d) = o {
                    let keys: Vec<String> = d.fields.iter().take(12).map(|(k, _)| k.clone()).collect();
                    err.notes.push(if keys.is_empty() { format!("{} is empty.", oc) } else { format!("its keys are: {}", keys.join(", ")) });
                }
                err.help.push(format!("to get a fallback instead of an error:   {}.get({}, 0)", oc, ic));
                err.help.push(format!("check first:   if {}.has({})", oc, ic));
                self.involve(&mut err, &[idx], scope);
            }
            _ => {
                err.label(Target::Expr(obj.clone()), format!("this is {}", kind_of(o)), true);
                self.involve(&mut err, &[obj], scope);
                if matches!(o, Value::None) {
                    err.help.push(format!("give {} a value before this line, or check first:   if {} != none", oc, oc));
                }
            }
        }
        err
    }

    /// `thing.name` or `thing.name(...)`: the property or function isn't there.
    pub(super) fn explain_field(&self, mut err: EzaError, obj: &Expr, name: &str, whole: &Expr, scope: &Rc<Scope>) -> EzaError {
        if err.explained {
            return err;
        }
        err.explained = true;
        let oc = code(obj);
        err.label(Target::Expr(whole.clone()), "", true);
        let value = self.peek(obj, scope);
        if let Some(v) = &value {
            match v {
                Value::None => {
                    err.notes.push(format!("{} is none - it doesn't have a value, so it has no '{}'.", oc, name));
                    err.help.push(format!("give {} a value first, or check:   if {} != none", oc, oc));
                }
                Value::Obj(o) if o.type_name == "dict" => {
                    let keys: Vec<String> = o.fields.iter().take(14).map(|(k, _)| k.clone()).collect();
                    err.notes.push(if keys.is_empty() { format!("{} is an empty dictionary.", oc) } else { format!("{} has the keys: {}", oc, keys.join(", ")) });
                    err.help.push(format!("to get a fallback instead of an error:   {}.get(\"{}\", 0)", oc, name));
                    err.help.push(format!("to add it:   change {}.{} to ...", oc, name));
                }
                Value::Obj(o) => {
                    let mut names: Vec<String> = o.fields.iter().map(|(k, _)| k.clone()).collect();
                    if let Some(c) = &o.class {
                        let mut d = Some(c.clone());
                        while let Some(t) = d {
                            names.extend(t.methods.iter().map(|m| format!("{}()", m.def.name)));
                            d = t.parent.clone();
                        }
                    }
                    if !names.is_empty() {
                        err.notes.push(format!("{} has: {}", oc, names.into_iter().take(14).collect::<Vec<_>>().join(", ")));
                    }
                }
                _ => {}
            }
        }
        // a function that belongs to a different kind of value
        let text_fns = ["upper", "lower", "trim", "split", "replace", "capitalize", "starts_with", "ends_with", "words", "lines", "pad_left", "pad_right", "matches", "find_all"];
        let list_fns = ["sort", "sum", "first", "last", "filter", "map", "find", "add", "remove", "join", "unique", "index_of", "group_by", "reverse", "min", "max"];
        match &value {
            Some(Value::List(_)) if text_fns.contains(&name) => {
                err.help.push(format!(".{} works on text; to use it on every item of the list:   {}.map(x -> x.{})", name, oc, name));
            }
            Some(Value::Str(_)) if list_fns.contains(&name) && !["reverse", "min", "max"].contains(&name) => {
                err.help.push(format!(".{} works on lists; turn the text into a list first, e.g.   {}.split(\" \").{}", name, oc, name));
            }
            Some(Value::Num(_)) if text_fns.contains(&name) || name == "length" => {
                err.help.push(format!("numbers don't have .{}; turn it into text first:   str({}).{}", name, oc, name));
            }
            Some(Value::Obj(o)) if err.msg.contains("has no property") && o.type_name != "dict" => {
                err.help.push(format!("to give it one:   change {}.{} to ...", oc, name));
            }
            _ => {}
        }
        self.involve(&mut err, &[obj], scope);
        err
    }

    /// A name that doesn't exist: typos, things made later or inside a block, habits from other languages.
    pub(super) fn explain_missing(&self, mut err: EzaError, name: &str, scope: &Rc<Scope>) -> EzaError {
        if err.explained {
            return err;
        }
        err.explained = true;
        err.label(Target::Name(name.to_string()), "", true);
        if let Some(h) = crate::diagnose::other_language_name(name) {
            err.help.push(h.to_string());
        }
        // inside a type's function: probably a property of self
        if let Some(Value::Obj(me)) = scope.value_of("self").map(deref_val) {
            if me.get(name).is_some() {
                if let Some(cut) = err.msg.find(" Did you mean") {
                    err.msg.truncate(cut);
                }
                err.help.push(format!("inside a type's function, its properties need self:   self.{}", name));
                return err;
            }
        }
        // a property of some object that exists: hp -> player.hp
        let mut owners = vec![];
        for (var_name, var) in self.globals.entries() {
            if let Value::Obj(o) = deref_val(var.borrow().get().clone()) {
                if o.get(name).is_some() && owners.len() < 2 {
                    owners.push(format!("{}.{}", var_name, name));
                }
            }
        }
        owners.sort();
        if !owners.is_empty() {
            err.help.push(format!("did you mean {}?", owners.join(" or ")));
        }
        // in a module: the main file's variables aren't visible
        if !Rc::ptr_eq(&self.top, &self.globals) && self.globals.get_local(name).is_some() {
            err.help.push(format!("modules can't see the main file's variables - pass {} in as an argument to the function", name));
        }
        // made further down, or inside a block
        if let Ok(text) = std::fs::read_to_string(self.file_name(self.cur_file)) {
            let made_here = |l: &str| {
                let t = l.trim_start();
                let words: Vec<&str> = t.split(|c: char| !(c.is_alphanumeric() || c == '_')).filter(|w| !w.is_empty()).collect();
                let assign = t.strip_prefix(name).is_some_and(|rest| {
                    let rest = rest.trim_start();
                    rest.starts_with('=') && !rest.starts_with("==")
                });
                assign || matches!(words.as_slice(), ["define" | "data" | "class" | "style" | "prefab", n, ..] if *n == name)
            };
            for (i, l) in text.lines().enumerate() {
                let n = i + 1;
                if n == err.line || !made_here(l) {
                    continue;
                }
                let indent = l.len() - l.trim_start().len();
                if n > err.line {
                    err.notes.push(format!("'{}' is created later, at line {} - code runs from top to bottom, so it doesn't exist yet here.", name, n));
                    err.labels.push(Label { line: n, file: String::new(), at: Target::Name(name.to_string()), msg: "created here, after it's used".into(), primary: false });
                } else if indent > 0 {
                    err.notes.push(format!("'{}' is created at line {}, but inside a block - names made inside an if, loop or function only exist there.", name, n));
                    err.help.push(format!("create it before that block instead, e.g.   {} = 0", name));
                    err.labels.push(Label { line: n, file: String::new(), at: Target::Name(name.to_string()), msg: "only exists inside this block".into(), primary: false });
                }
                break;
            }
        }
        err
    }

    /// Calling went wrong: the wrong number of arguments, or calling something that isn't a function.
    pub(super) fn explain_call(&self, mut err: EzaError, callee: &Expr, whole: &Expr, scope: &Rc<Scope>) -> EzaError {
        if err.explained || !err.trace.is_empty() || err.line != self.line {
            return err;
        }
        err.explained = true;
        err.label(Target::Expr(whole.clone()), "", true);
        let f = self.peek(callee, scope);
        match &f {
            Some(Value::Func(func)) if err.msg.contains("argument") => {
                let d = &func.def;
                let sig = if d.params.is_empty() { format!("define {}", d.name) } else { format!("define {}, {}", d.name, d.params.join(", ")) };
                err.notes.push(format!("{} takes {} argument(s): {}", d.name, d.params.len(), if d.params.is_empty() { "none".into() } else { d.params.join(", ") }));
                if d.line > 0 {
                    err.labels.push(Label { line: d.line, file: self.file_name(func.file).to_string(), at: Target::Name(d.name.clone()), msg: format!("defined here as:  {}", sig), primary: false });
                }
                err.help.push(format!("call it like:   {}({})", code(callee), d.params.join(", ")));
            }
            Some(v) if err.msg.contains("is not a function") => {
                err.notes.push(format!("{} is {} ({}), not a function.", code(callee), kind_of(v), short(v)));
                err.help.push(format!("remove the ( ) to use its value:   {}", code(callee)));
            }
            _ => {
                // a built-in got something it can't use: show what it got
                if let Expr::Call(_, args) = whole {
                    for a in args.iter().take(4) {
                        if let Some(v) = self.peek(&a.value, scope) {
                            err.notes.push(format!("{} is {}: {}", code(&a.value), kind_of(&v), short(&v)));
                        }
                    }
                }
                if err.msg.contains("isn't a number") {
                    err.notes.push("num() only works on text made of digits, like \"12\" or \"3.5\".".into());
                    err.help.push("if it might not be a number, catch the problem:   attempt  (num line)  handle  (fallback)".into());
                } else if err.msg.contains("expected a number") {
                    err.help.push("text that holds a number can be turned into one with   num(...)".into());
                }
            }
        }
        err
    }

    /// `expect a == b` failed: point at both sides and say how they got their values.
    pub(super) fn explain_expect(&self, mut err: EzaError, l: &Expr, r: &Expr, a: &Value, b: &Value, scope: &Rc<Scope>) -> EzaError {
        err.explained = true;
        err.label(Target::Expr(l.clone()), format!("this is {}", short(a)), true);
        err.label(Target::Expr(r.clone()), format!("this is {}", short(b)), false);
        self.involve(&mut err, &[l, r], scope);
        err
    }
}
