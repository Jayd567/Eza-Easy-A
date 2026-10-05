//! Drawing errors: the code around the problem with the exact spot underlined, the values involved,
//! a suggested fix, and an error code with a longer explanation (`eza explain E003`).
use crate::ast::{Arg, Expr};
use crate::error::{ErrKind, EzaError, Label, Target};
use crate::value::fmt_num;

// ---------- finding code in a line ----------

/// A piece of a source line, with the columns it covers (after tabs are expanded).
struct Piece {
    text: String,
    start: usize,
    end: usize,
}

/// Tabs become 4 spaces, so columns line up in the terminal.
fn expand_tabs(line: &str) -> String {
    line.replace('\t', "    ")
}

/// Where raw column `col` (tabs counted as 1) ends up once tabs are expanded.
fn display_col(raw: &str, col: usize) -> usize {
    raw.chars().take(col).map(|c| if c == '\t' { 4 } else { 1 }).sum()
}

/// Splits a line the way Eza reads it: names, numbers, text in quotes, symbols. Comments are dropped.
fn pieces(line: &str) -> Vec<Piece> {
    let c: Vec<char> = line.chars().collect();
    let (mut i, mut out) = (0, vec![]);
    while i < c.len() {
        let s = i;
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
            continue;
        }
        if ch == '#' {
            let mut j = i + 1;
            while j < c.len() && c[j].is_ascii_hexdigit() {
                j += 1;
            }
            let is_color = matches!(j - i - 1, 3 | 4 | 6 | 8) && (j >= c.len() || !(c[j].is_alphanumeric() || c[j] == '_'));
            if !is_color {
                break; // a comment
            }
            i = j;
        } else if ch == '"' || ch == '\'' {
            i += 1;
            while i < c.len() && c[i] != ch {
                if c[i] == '\\' {
                    i += 1;
                }
                i += 1;
            }
            i = (i + 1).min(c.len());
        } else if ch.is_ascii_digit() {
            while i < c.len() && (c[i].is_ascii_alphanumeric() || c[i] == '_' || (c[i] == '.' && i + 1 < c.len() && c[i + 1].is_ascii_digit())) {
                i += 1;
            }
        } else if ch.is_alphabetic() || ch == '_' {
            while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_') {
                i += 1;
            }
        } else {
            let two: String = c[i..(i + 2).min(c.len())].iter().collect();
            i += if ["==", "!=", "<=", ">=", "<<", ">>", "->"].contains(&two.as_str()) { 2 } else { 1 };
        }
        out.push(Piece { text: c[s..i].iter().collect(), start: s, end: i });
    }
    out
}

/// Marks a token that stands for text in quotes (its contents follow), or any text at all.
const TEXT: char = '\u{1}';
const ANY_TEXT: &str = "\u{2}";

fn arg_tokens(args: &[Arg], out: &mut Vec<String>) {
    for (i, a) in args.iter().enumerate() {
        if i > 0 {
            out.push(",".into());
        }
        if let Some(n) = &a.name {
            out.push(n.clone());
            out.push("=".into());
        }
        expr_tokens(&a.value, out);
    }
}

/// Text with `{...}` in it is read as "" + part + part ...; the leftmost piece is an empty text.
fn is_interpolated(e: &Expr) -> bool {
    match e {
        Expr::Binary(crate::ast::Op::Add, l, _) => matches!(&**l, Expr::Str(s) if s.is_empty()) || is_interpolated(l),
        _ => false,
    }
}

/// The tokens a piece of code would have been written with (brackets aside).
fn expr_tokens(e: &Expr, out: &mut Vec<String>) {
    let t = |s: &str| s.to_string();
    match e {
        _ if is_interpolated(e) => out.push(t(ANY_TEXT)),
        Expr::Num(n) => out.push(fmt_num(*n)),
        Expr::Str(s) => out.push(format!("{}{}", TEXT, s)),
        Expr::Bool(b) => out.push(b.to_string()),
        Expr::None => out.push(t("none")),
        Expr::Color(h) => out.push(format!("#{}", h)),
        Expr::Ident(n) => out.push(n.clone()),
        Expr::Field(o, f) => {
            expr_tokens(o, out);
            out.push(t("."));
            out.push(f.clone());
        }
        Expr::Index(o, i) => {
            expr_tokens(o, out);
            out.push(t("["));
            expr_tokens(i, out);
            out.push(t("]"));
        }
        Expr::Call(c, args) => {
            expr_tokens(c, out);
            out.push(t("("));
            arg_tokens(args, out);
            out.push(t(")"));
        }
        Expr::Unary(op, x) => {
            out.push(t(op));
            expr_tokens(x, out);
        }
        Expr::Binary(op, l, r) => {
            expr_tokens(l, out);
            out.push(t(op.as_str()));
            expr_tokens(r, out);
        }
        Expr::List(items) => {
            out.push(t("["));
            for (i, x) in items.iter().enumerate() {
                if i > 0 {
                    out.push(t(","));
                }
                expr_tokens(x, out);
            }
            out.push(t("]"));
        }
        Expr::Dict(items) => {
            out.push(t("{"));
            for (i, (k, v)) in items.iter().enumerate() {
                if i > 0 {
                    out.push(t(","));
                }
                out.push(k.clone());
                out.push(t(":"));
                expr_tokens(v, out);
            }
            out.push(t("}"));
        }
        Expr::Lambda(d) => {
            match d.params.as_slice() {
                [one] => out.push(one.clone()),
                many => {
                    out.push(t("("));
                    for (i, p) in many.iter().enumerate() {
                        if i > 0 {
                            out.push(t(","));
                        }
                        out.push(p.clone());
                    }
                    out.push(t(")"));
                }
            }
            out.push(t("->"));
            if let Some(crate::ast::StmtKind::Return(r)) = d.body.first().map(|s| &s.kind) {
                if let Some(x) = r.first() {
                    expr_tokens(x, out);
                }
            }
        }
        Expr::Spawn { prefab, .. } => {
            out.push(t("spawn"));
            expr_tokens(prefab, out);
        }
        Expr::Load(x) => {
            out.push(t("load"));
            expr_tokens(x, out);
        }
        Expr::Pop(x) => {
            out.push(t("pop"));
            expr_tokens(x, out);
        }
        Expr::Range(a, b) => {
            expr_tokens(a, out);
            out.push(t("to"));
            expr_tokens(b, out);
        }
    }
}

fn precedence(op: crate::ast::Op) -> u8 {
    use crate::ast::Op::*;
    match op {
        Or => 1,
        And => 2,
        Eq | Ne | Lt | Gt | Le | Ge | In => 3,
        BitOr | BitXor | BitAnd | Shl | Shr => 4,
        Add | Sub => 5,
        Mul | Div | Rem => 6,
    }
}

/// The code as Eza would write it, for messages: `enemy.attack_power / enemy.defense`.
pub fn code(e: &Expr) -> String {
    let side = |x: &Expr, parent: crate::ast::Op| match x {
        Expr::Binary(op, ..) if !is_interpolated(x) && precedence(*op) < precedence(parent) => format!("({})", code(x)),
        _ => code(x),
    };
    match e {
        _ if is_interpolated(e) => "\"...\"".into(),
        Expr::Num(n) => fmt_num(*n),
        Expr::Str(s) => format!("\"{}\"", s),
        Expr::Bool(b) => b.to_string(),
        Expr::None => "none".into(),
        Expr::Color(h) => format!("#{}", h),
        Expr::Ident(n) => n.clone(),
        Expr::Field(o, f) => format!("{}.{}", code(o), f),
        Expr::Index(o, i) => format!("{}[{}]", code(o), code(i)),
        Expr::Call(c, args) => {
            let a: Vec<String> = args.iter().map(|a| match &a.name {
                Some(n) => format!("{}={}", n, code(&a.value)),
                None => code(&a.value),
            }).collect();
            format!("{}({})", code(c), a.join(", "))
        }
        Expr::Unary(op, x) if *op == "not" => format!("not {}", code(x)),
        Expr::Unary(op, x) => format!("{}{}", op, code(x)),
        Expr::Binary(op, l, r) => format!("{} {} {}", side(l, *op), op.as_str(), side(r, *op)),
        Expr::List(items) => format!("[{}]", items.iter().map(code).collect::<Vec<_>>().join(", ")),
        Expr::Dict(items) => format!("{{{}}}", items.iter().map(|(k, v)| format!("{}: {}", k, code(v))).collect::<Vec<_>>().join(", ")),
        Expr::Lambda(d) => {
            let body = match d.body.first().map(|s| &s.kind) {
                Some(crate::ast::StmtKind::Return(r)) if r.len() == 1 => code(&r[0]),
                _ => "...".into(),
            };
            match d.params.as_slice() {
                [one] => format!("{} -> {}", one, body),
                many => format!("({}) -> {}", many.join(", "), body),
            }
        }
        Expr::Spawn { prefab, .. } => format!("spawn {}", code(prefab)),
        Expr::Load(x) => format!("load {}", code(x)),
        Expr::Pop(x) => format!("pop {}", code(x)),
        Expr::Range(a, b) => format!("{} to {}", code(a), code(b)),
    }
}

fn same_token(src: &str, pat: &str) -> bool {
    if pat == ANY_TEXT {
        return src.starts_with('"') || src.starts_with('\'');
    }
    if let Some(text) = pat.strip_prefix(TEXT) {
        let inner = src.get(1..src.len().saturating_sub(1)).unwrap_or("");
        return (src.starts_with('"') || src.starts_with('\'')) && inner.replace("\\\"", "\"").replace("\\n", "\n") == text;
    }
    if src == pat {
        return true;
    }
    if src.starts_with('#') && pat.starts_with('#') {
        return src.eq_ignore_ascii_case(pat);
    }
    // numbers can be written many ways: 0.50, 1_000, 0xFF
    let num = |s: &str| -> Option<f64> {
        let s = s.replace('_', "");
        let l = s.to_lowercase();
        if let Some(h) = l.strip_prefix("0x") {
            return i64::from_str_radix(h, 16).ok().map(|v| v as f64);
        }
        if let Some(b) = l.strip_prefix("0b") {
            return i64::from_str_radix(b, 2).ok().map(|v| v as f64);
        }
        if let Some(o) = l.strip_prefix("0o") {
            return i64::from_str_radix(o, 8).ok().map(|v| v as f64);
        }
        s.parse().ok()
    };
    matches!((num(src), num(pat)), (Some(a), Some(b)) if a == b)
}

/// Where `want` (tokens) appears among `src`, ignoring round brackets on both sides.
fn find_tokens(src: &[Piece], want: &[String]) -> Option<(usize, usize)> {
    let src: Vec<&Piece> = src.iter().filter(|p| p.text != "(" && p.text != ")").collect();
    let want: Vec<&String> = want.iter().filter(|w| *w != "(" && *w != ")").collect();
    if want.is_empty() || want.len() > src.len() {
        return None;
    }
    (0..=src.len() - want.len())
        .find(|&i| want.iter().enumerate().all(|(k, w)| same_token(&src[i + k].text, w)))
        .map(|i| (src[i].start, src[i + want.len() - 1].end))
}

/// The columns of a piece of code in a (tab-expanded) line.
fn locate(line: &str, e: &Expr) -> Option<(usize, usize)> {
    let ps = pieces(line);
    let mut want = vec![];
    expr_tokens(e, &mut want);
    if let Some(found) = find_tokens(&ps, &want) {
        return Some(found);
    }
    // `1, 2, 3` is a list written without brackets
    if let Expr::List(_) = e {
        if want.len() >= 2 {
            return find_tokens(&ps, &want[1..want.len() - 1]);
        }
    }
    None
}

fn locate_name(line: &str, name: &str) -> Option<(usize, usize)> {
    pieces(line).into_iter().find(|p| p.text == name).map(|p| (p.start, p.end))
}

// ---------- drawing ----------

/// A file's text when it differs from what's on disk (an editor's unsaved changes, for `eza check --stdin`).
static GIVEN: std::sync::OnceLock<(String, String)> = std::sync::OnceLock::new();

pub fn use_text_for(file: &str, text: String) {
    let _ = GIVEN.set((file.to_string(), text));
}

fn read_lines(file: &str) -> Option<Vec<String>> {
    if file.is_empty() || file.starts_with('<') {
        return None;
    }
    let text = match GIVEN.get() {
        Some((f, t)) if f == file => t.clone(),
        _ => std::fs::read_to_string(file).ok()?,
    };
    let text = text.strip_prefix('\u{feff}').map(|s| s.to_string()).unwrap_or(text);
    Some(text.lines().map(|l| l.to_string()).collect())
}

/// The columns a label covers on its (tab-expanded) line.
fn columns(raw: &str, shown: &str, at: &Target) -> Option<(usize, usize)> {
    match at {
        Target::Expr(e) => locate(shown, e),
        Target::Name(n) => locate_name(shown, n),
        Target::Cols(a, b) => Some((display_col(raw, *a), display_col(raw, *b).max(display_col(raw, *a) + 1))),
    }
}

/// `   12 |     code` and the underline rows below it.
fn draw_line(out: &mut String, lines: &[String], n: usize, width: usize, labels: &[&Label], loose: &mut Vec<String>) {
    let Some(raw) = lines.get(n.wrapping_sub(1)) else { return };
    let shown = expand_tabs(raw);
    out.push_str(&format!("\n {:>w$} | {}", n, shown, w = width));
    for l in labels {
        match columns(raw, &shown, &l.at) {
            Some((a, b)) => {
                let mark = if l.primary { "^" } else { "-" };
                out.push_str(&format!("\n {:>w$} | {}{}", "", " ".repeat(a), mark.repeat((b - a).max(1)), w = width));
                if !l.msg.is_empty() {
                    out.push(' ');
                    out.push_str(&l.msg);
                }
            }
            None if l.primary && !l.msg.is_empty() => loose.push(l.msg.clone()),
            None => {}
        }
    }
}

/// The whole error as shown in the terminal.
pub fn render(e: &EzaError) -> String {
    let mut out = e.headline();
    if e.is_switch() {
        return out;
    }
    let mut loose = vec![];
    if let Some(lines) = read_lines(&e.file) {
        let own = |l: &&Label| l.file.is_empty() || l.file == e.file;
        let first = e.line.saturating_sub(2).max(1);
        let last = (e.line + 1).min(lines.len());
        // labels on the lines shown around the error go right under them; the rest get their own snippet
        let in_window = |l: &&Label| own(l) && l.line >= first && l.line <= last;
        let others: Vec<&Label> = e.labels.iter().filter(|l| !in_window(l)).collect();
        let mut width = last.to_string().len();
        for l in &others {
            width = width.max(l.line.to_string().len());
        }
        for n in first..=last {
            let labels: Vec<&Label> = e.labels.iter().filter(|l| in_window(l) && l.line == n).collect();
            draw_line(&mut out, &lines, n, width, &labels, &mut loose);
        }
        // other places that matter: where a value was set, a function defined, ...
        let mut drawn: Vec<(String, usize)> = vec![];
        for l in &others {
            if drawn.contains(&(l.file.clone(), l.line)) {
                continue;
            }
            drawn.push((l.file.clone(), l.line));
            let same: Vec<&Label> = others.iter().filter(|o| o.file == l.file && o.line == l.line).copied().collect();
            let text = if own(l) { Some(lines.clone()) } else { read_lines(&l.file) };
            let Some(text) = text else { continue };
            // a line is only worth showing if something on it can be pointed at
            let found = text.get(l.line.wrapping_sub(1)).is_some_and(|raw| {
                let shown = expand_tabs(raw);
                same.iter().any(|s| columns(raw, &shown, &s.at).is_some())
            });
            if !found {
                loose.extend(same.iter().filter(|s| s.primary && !s.msg.is_empty()).map(|s| s.msg.clone()));
                continue;
            }
            if own(l) {
                if l.line > last + 1 || l.line + 1 < first {
                    out.push_str(&format!("\n {:>w$}", "...", w = width + 2));
                }
            } else {
                out.push_str(&format!("\n   --> {}:{}", l.file, l.line));
            }
            draw_line(&mut out, &text, l.line, width, &same, &mut loose);
        }
        if e.kind == ErrKind::Syntax && e.help.is_empty() {
            if let Some(src) = lines.get(e.line.wrapping_sub(1)) {
                for (i, h) in syntax_hints(src, &e.msg).iter().enumerate() {
                    out.push_str(if i == 0 { "\n\n   help: " } else { "\n   help: " });
                    out.push_str(h);
                }
            }
        }
    } else {
        loose.extend(e.labels.iter().filter(|l| !l.msg.is_empty()).map(|l| l.msg.clone()));
    }
    if !e.notes.is_empty() || !loose.is_empty() {
        out.push('\n');
    }
    for n in loose.iter().chain(e.notes.iter()) {
        for (i, part) in n.lines().enumerate() {
            out.push_str(if i == 0 { "\n   " } else { "\n     " });
            out.push_str(part);
        }
    }
    for (i, h) in e.help.iter().enumerate() {
        out.push_str(if i == 0 { "\n\n   help: " } else { "\n     or: " });
        out.push_str(h);
    }
    if !e.trace.is_empty() {
        out.push('\n');
    }
    for (name, line) in e.trace.iter().take(6) {
        out.push_str(&format!("\n   in {} (called from line {})", name, line));
    }
    if e.trace.len() > 6 {
        out.push_str(&format!("\n   ... and {}+ more calls", e.trace.len() - 6));
    }
    for c in &e.context {
        out.push_str(&format!("\n   {}", c));
    }
    out
}

/// One line per problem with exact columns, for editors: `path:line:col:endcol: message`.
pub fn plain(e: &EzaError) -> String {
    let kind = match e.kind {
        ErrKind::Syntax => "Syntax Error",
        ErrKind::Warning => "Check Warning",
        ErrKind::Check => "Check Error",
        _ => "Runtime Error",
    };
    let mut cols = String::new();
    if let (Some(lines), Some(l)) = (read_lines(&e.file), e.labels.iter().find(|l| l.primary && l.line == e.line)) {
        if let Some(raw) = lines.get(e.line.wrapping_sub(1)) {
            let shown = expand_tabs(raw);
            if let Some((a, b)) = columns(raw, &shown, &l.at) {
                // editors count tabs as one column
                let back = |dcol: usize| {
                    let mut seen = 0;
                    for (i, ch) in raw.chars().enumerate() {
                        if seen >= dcol {
                            return i;
                        }
                        seen += if ch == '\t' { 4 } else { 1 };
                    }
                    raw.chars().count()
                };
                cols = format!(":{}:{}", back(a) + 1, back(b) + 1);
            }
        }
    }
    let first_help = e.help.first().map(|h| format!(" - {}", h)).unwrap_or_default();
    format!("[{}] {}:{}{}: {}{}", kind, e.file, e.line, cols, e.msg, first_help)
}

// ---------- syntax hints: habits from other languages ----------

/// Help for a syntax error, worked out from the line itself.
fn syntax_hints(line: &str, msg: &str) -> Vec<String> {
    let t = line.trim();
    let mut out = vec![];
    let words: Vec<&str> = t.split(|c: char| !(c.is_alphanumeric() || c == '_')).filter(|w| !w.is_empty()).collect();
    let first = words.first().copied().unwrap_or("");
    let ps = pieces(t);
    let toks: Vec<&str> = ps.iter().map(|p| p.text.as_str()).collect();
    if let Some(i) = toks.windows(2).position(|w| matches!(w, [op, "="] if ["+", "-", "*", "/"].contains(op))) {
        let target = toks[..i].join("");
        let rest: Vec<&str> = toks[i + 2..].to_vec();
        let by = if toks[i] == "-" { format!("-{}", rest.join(" ")) } else { rest.join(" ") };
        if toks[i] == "+" || toks[i] == "-" {
            out.push(format!("Eza writes this as:  change {} by {}", target, by));
        } else {
            out.push(format!("Eza writes this as:  change {} to {} {} {}", target, target, toks[i], rest.join(" ")));
        }
    }
    if toks.windows(2).any(|w| w == ["+", "+"] || w == ["-", "-"]) {
        out.push("Eza doesn't have ++ or --: write  change x by 1  (or by -1)".into());
    }
    if (first == "if" || first == "while" || first == "else") && toks.contains(&"=") && !toks.contains(&"==") {
        let fixed: Vec<&str> = toks.iter().map(|tk| if *tk == "=" { "==" } else { tk }).collect();
        out.push(format!("to compare, use == (one = only creates a variable):   {}", fixed.join(" ")));
    }
    if msg.contains("closing quote") {
        out.push("end the text with the same kind of quote it starts with: \"like this\"".into());
    }
    if msg.contains("indentation") {
        out.push("line it up exactly with the lines of the block it belongs to (usually 4 spaces per level)".into());
    }
    if t.ends_with(':') {
        out.push("Eza doesn't put ':' at the end of lines - a block is just the indented lines below".into());
    }
    if t.ends_with('{') || t == "}" {
        out.push("Eza doesn't use { } for blocks - indent the lines that belong inside instead".into());
    }
    if t.ends_with(';') {
        out.push("Eza doesn't need ';' at the end of lines".into());
    }
    match first {
        "def" | "function" | "func" | "fn" | "fun" | "void" => out.push(format!("functions are made with define:  define {}", words.get(1).unwrap_or(&"name"))),
        "elif" | "elsif" | "elseif" => out.push("Eza writes it as two words:  else if".into()),
        "var" | "let" | "const" | "int" | "float" | "string" | "auto" if words.len() > 1 => {
            out.push(format!("just write the name - no {} needed:  {} = ...", first, words[1]))
        }
        "for" => out.push("loops in Eza:  each item in list   or   each i in 10   or   while condition".into()),
        "import" | "require" | "using" | "from" => out.push("to load another file:  use \"file.eza\"  (or  include \"file.eza\")".into()),
        "data" if msg.contains("indented") => out.push("put the fields underneath, indented:  name = \"\"".into()),
        _ => {}
    }
    if words.contains(&"this") {
        out.push("inside a type's function, the object is called self (not this)".into());
    }
    out
}

/// Names that people bring from other languages, and what Eza calls them.
pub fn other_language_name(name: &str) -> Option<&'static str> {
    Some(match name {
        "null" | "nil" | "None" | "NULL" | "undefined" | "nullptr" => "Eza calls 'no value' none",
        "True" | "TRUE" => "Eza writes it in lowercase: true",
        "False" | "FALSE" => "Eza writes it in lowercase: false",
        "this" => "inside a type's function, the object is called self",
        "elif" | "elsif" | "elseif" => "Eza writes it as two words: else if",
        "println" | "printf" | "puts" | "echo" | "console" | "log" => "to show something, use print(...)",
        "function" | "def" | "func" | "fn" => "functions are made with: define name, arguments",
        "length" | "size" | "count" => "to get a length, use len(x) or x.length",
        "string" | "toString" | "to_str" => "to turn something into text, use str(x)",
        "parseInt" | "parseFloat" | "float" | "Number" => "to turn text into a number, use num(x) (or int(x))",
        "var" | "let" | "const" => "just write the name: x = 5",
        "class" => "Eza makes classes with data:  data Enemy",
        _ => return None,
    })
}

// ---------- error codes ----------

/// (code, short name, what it means and how to fix it)
pub const CODES: &[(&str, &str, &str)] = &[
    ("E001", "name doesn't exist", "A name was used before anything was called that. It's usually a typo (Eza suggests the closest name), or the line that creates it comes later in the file - code runs from top to bottom. Inside a module, the main file's variables aren't visible: pass what the module needs into its functions."),
    ("E002", "name already exists", "`x = ...` creates a new name, so using it twice for the same name in one block is a mistake. To give an existing variable a new value, write `change x to ...` (or `change x by ...` to add to it)."),
    ("E003", "division by zero", "Dividing (/) or taking the remainder (%) by 0 has no answer. Check the number first (`if d != 0`), or make sure it can never be 0, for example with `max(d, 1)`. The error shows where the 0 came from."),
    ("E004", "wrong kinds of values", "An operator got values it can't combine, like subtracting text from a number. Text that holds a number (\"12\") must be turned into a number with num(...) first; a number becomes text with str(...). `none` means a variable has no value yet."),
    ("E005", "can't compare", "< > <= >= only compare numbers with numbers, text with text, and dates with dates. Turn text into a number with num(...) to compare it with one."),
    ("E006", "index out of range", "List and text positions start at 0, so a list with 3 items has positions 0, 1 and 2. Negative positions count from the end: -1 is the last item. Check the length with len(x) before reading a position that might not be there."),
    ("E007", "missing property or key", "The object or dictionary doesn't have that name. Check the spelling (Eza suggests the closest one). To add a new one, use `change thing.name to ...`. For dictionaries, d.get(\"key\", default) gives a fallback instead of an error."),
    ("E008", "unknown function on a value", "That kind of value doesn't have this function (method). Lists, text and numbers each have their own; Eza says which kind has it. For a list of texts, use .map to apply a text function to each one: names.map(n -> n.upper)."),
    ("E009", "wrong number of arguments", "A function was called with more or fewer values than its `define` line asks for. Pass one value per name on the define line, by position or by name: heal(player, amount=5)."),
    ("E010", "not a function", "Something that isn't a function was called with ( ). Remove the brackets to use the value itself."),
    ("E011", "not a number", "A number was needed but something else was given. Text that holds a number can be turned into one with num(...)."),
    ("E012", "file not found", "A file couldn't be opened. Paths are relative to the script's own folder; use exists(path) to check before opening."),
    ("E013", "simulation changed the real world", "Inside `mimic`, only the shadow copy may change. Changing anything else would make the prediction affect the real game."),
    ("E014", "too much recursion", "A function kept calling itself more than 2000 times deep. Make sure there's a case where it stops calling itself."),
    ("E015", "wait in the wrong place", "`wait` only works inside a function or an `on` block (and inside if/each/while there). It can't be used at the top of the script or inside attempt, persist or mimic."),
    ("E016", "indentation", "The lines of a block must all be indented by the same amount, and a block ends when the indentation goes back to an outer level."),
    ("E017", "couldn't read the code", "The line isn't written in a way Eza understands. The caret points at where reading stopped; the help line suggests the usual fix."),
    ("E018", "module problem", "A `use` line failed: the file is missing, two modules use each other, or a module's variable was changed from outside (only the module's own code may change its variables)."),
    ("E019", "expect failed", "An `expect` (in a test, or as a check in your code) wasn't true. The message shows the values on both sides."),
    ("E020", "nothing to pop", "`pop` took from an empty list, stack or queue. Check `.empty` or len(...) first."),
    ("E021", "game object problem", "Something about prefabs, spawning, touching or sprite animations was used the wrong way: `on a touches b` needs objects, lists of objects or a prefab; a prefab only has `.all` and `.count` (spawn a copy to change one); `spawn ... into list` needs a list that already exists (`coins = []`); `animation` needs frame numbers like [0, 1, 2], or the name of one of the sprite's `animations`."),
];

/// The code for an error, worked out from its message.
pub fn code_of(e: &EzaError) -> &'static str {
    let m = e.msg.as_str();
    let has = |s: &str| m.contains(s);
    match e.kind {
        ErrKind::Syntax if has("indent") => "E016",
        ErrKind::Syntax if has("isolated simulation") => "E013",
        ErrKind::Syntax if has("wait ") => "E015",
        ErrKind::Syntax => "E017",
        ErrKind::Switch | ErrKind::Warning => "",
        _ if has("isolated simulation") => "E013",
        _ if has("doesn't exist yet") || has("isn't created anywhere") => "E001",
        _ if has("already exists") && !has("module") => "E002",
        _ if has("divide by zero") => "E003",
        _ if has("can't compare") => "E005",
        _ if has("can't use '") || has("can't change '") => "E004",
        _ if has("can't add") || has("can't subtract") || has("can't multiply") || has("can't divide") || has("negative") => "E004",
        _ if has("can't use [ ]") || has("needs a number in [ ]") || has("can't loop over") || has("can't go through") => "E004",
        _ if has("out of range") => "E006",
        _ if has("has no property") || has("there's no key") || has("has no field") => "E007",
        _ if has("has no method") || has("has no function") => "E008",
        _ if has("argument(s)") || has("is missing the argument") || has("only has") => "E009",
        _ if has("is not a function") || has("not a function, so") => "E010",
        _ if has("expected a number") || has("isn't a number") || has("needs whole numbers") => "E011",
        _ if has("can't open") || has("can't find") || has("can't load") => "E012",
        _ if has("recursion") => "E014",
        _ if has("wait only works") => "E015",
        _ if has("module") || has("can't use \"") => "E018",
        _ if has("expect failed") => "E019",
        _ if has("nothing to pop") => "E020",
        _ if has("touches needs") || has("animation") || has("prefab only has") || has("into needs") || has("spawn needs a prefab") => "E021",
        _ => "",
    }
}

/// `eza explain E003` (or `eza explain` for the list).
pub fn explain(code: Option<&str>) -> i32 {
    match code {
        None => {
            println!("Error codes (eza explain E003 shows one in detail):\n");
            for (c, name, _) in CODES {
                println!("  {}  {}", c, name);
            }
            0
        }
        Some(c) => {
            let want = c.to_uppercase();
            let want = if want.starts_with('E') { want } else { format!("E{:0>3}", want) };
            match CODES.iter().find(|(code, _, _)| *code == want) {
                Some((code, name, text)) => {
                    println!("{}: {}\n", code, name);
                    let mut line = String::new();
                    for word in text.split(' ') {
                        if line.len() + word.len() > 88 {
                            println!("{}", line.trim_end());
                            line.clear();
                        }
                        line.push_str(word);
                        line.push(' ');
                    }
                    println!("{}", line.trim_end());
                    0
                }
                None => {
                    eprintln!("there's no error code {} - `eza explain` lists them all", c);
                    2
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> Expr {
        let toks = crate::lexer::lex(src).unwrap();
        crate::parser::Parser::new(toks).expr_for_tests().unwrap()
    }

    #[test]
    fn finds_code_in_a_line() {
        let line = "            change player.health by enemy.attack_power / (enemy.defense)  # ouch";
        let e = parse("enemy.attack_power / enemy.defense");
        let (a, b) = locate(line, &e).unwrap();
        assert_eq!(&line[a..b], "enemy.attack_power / (enemy.defense");
        let d = parse("enemy.defense");
        let (a, b) = locate(line, &d).unwrap();
        assert_eq!(&line[a..b], "enemy.defense");
        assert_eq!(code(&e), "enemy.attack_power / enemy.defense");
        assert_eq!(code(&parse("heal(p, amount=5)")), "heal(p, amount=5)");
        assert_eq!(code(&parse("list[i + 1]")), "list[i + 1]");
    }

    #[test]
    fn numbers_match_however_they_are_written() {
        let e = parse("x / 0.5");
        assert!(locate("y = x / 0.50", &e).is_some());
    }
}
