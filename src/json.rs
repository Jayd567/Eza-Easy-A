//! Minimal JSON for `save` / `load` (objects become dictionaries).
use crate::value::*;

pub fn to_json(v: &Value) -> Result<String, String> {
    let mut s = String::new();
    write(v, 0, &mut s)?;
    s.push('\n');
    Ok(s)
}

fn quote(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn write(v: &Value, depth: usize, out: &mut String) -> Result<(), String> {
    let pad = |n: usize| "  ".repeat(n);
    match v {
        Value::None => out.push_str("null"),
        Value::Bool(b) => out.push_str(&b.to_string()),
        Value::Num(n) => {
            if !n.is_finite() {
                out.push_str("null");
            } else if n.fract() == 0.0 && n.abs() < 1e15 {
                out.push_str(&format!("{}", *n as i64));
            } else {
                out.push_str(&format!("{}", n));
            }
        }
        Value::Str(s) => quote(s, out),
        Value::Color(c) => quote(&color_hex(**c), out),
        Value::List(l) => {
            if l.is_empty() {
                out.push_str("[]");
            } else if l.iter().all(|x| matches!(x, Value::Num(_) | Value::Str(_) | Value::Bool(_) | Value::None)) {
                out.push('[');
                for (i, x) in l.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    write(x, depth, out)?;
                }
                out.push(']');
            } else {
                out.push_str("[\n");
                for (i, x) in l.iter().enumerate() {
                    out.push_str(&pad(depth + 1));
                    write(x, depth + 1, out)?;
                    out.push_str(if i + 1 < l.len() { ",\n" } else { "\n" });
                }
                out.push_str(&pad(depth));
                out.push(']');
            }
        }
        Value::Entity(h) => {
            let inner = h.var.borrow().get().clone();
            write(&inner, depth, out)?;
        }
        // dates are saved as text like "2026-10-04 18:30:05" (read back with date(...))
        Value::Obj(o) if o.type_name == "date" => write(&Value::Str(crate::tools::date_text(o)), depth, out)?,
        Value::Obj(o) => {
            if o.fields.is_empty() {
                out.push_str("{}");
            } else {
                out.push_str("{\n");
                for (i, (k, x)) in o.fields.iter().enumerate() {
                    out.push_str(&pad(depth + 1));
                    quote(k, out);
                    out.push_str(": ");
                    write(x, depth + 1, out)?;
                    out.push_str(if i + 1 < o.fields.len() { ",\n" } else { "\n" });
                }
                out.push_str(&pad(depth));
                out.push('}');
            }
        }
        other => return Err(format!("can't save {} to a file", other.type_name())),
    }
    Ok(())
}

pub fn from_json(text: &str) -> Result<Value, String> {
    let mut p = P { c: text.trim_start_matches('\u{feff}').chars().collect(), i: 0 };
    let v = p.value()?;
    p.ws();
    if p.i < p.c.len() {
        return Err(p.err("extra text after the value"));
    }
    Ok(v)
}

struct P {
    c: Vec<char>,
    i: usize,
}

impl P {
    fn err(&self, msg: &str) -> String {
        let line = self.c[..self.i.min(self.c.len())].iter().filter(|c| **c == '\n').count() + 1;
        format!("{} (line {})", msg, line)
    }
    fn ws(&mut self) {
        while self.i < self.c.len() && self.c[self.i].is_whitespace() {
            self.i += 1;
        }
    }
    fn eat(&mut self, ch: char) -> bool {
        self.ws();
        if self.i < self.c.len() && self.c[self.i] == ch {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn word(&mut self, w: &str) -> bool {
        let n = w.chars().count();
        if self.c.len() >= self.i + n && self.c[self.i..self.i + n].iter().collect::<String>() == w {
            self.i += n;
            true
        } else {
            false
        }
    }
    fn value(&mut self) -> Result<Value, String> {
        self.ws();
        let Some(&ch) = self.c.get(self.i) else { return Err(self.err("unexpected end of file")) };
        match ch {
            '{' => {
                self.i += 1;
                let mut o = Obj::new("dict");
                if self.eat('}') {
                    return Ok(Value::obj(o));
                }
                loop {
                    self.ws();
                    let k = self.string()?;
                    if !self.eat(':') {
                        return Err(self.err("expected ':' after a key"));
                    }
                    let v = self.value()?;
                    o.set(&k, v);
                    if self.eat(',') {
                        continue;
                    }
                    if self.eat('}') {
                        return Ok(Value::obj(o));
                    }
                    return Err(self.err("expected ',' or '}'"));
                }
            }
            '[' => {
                self.i += 1;
                let mut items = vec![];
                if self.eat(']') {
                    return Ok(Value::list(items));
                }
                loop {
                    items.push(self.value()?);
                    if self.eat(',') {
                        continue;
                    }
                    if self.eat(']') {
                        return Ok(Value::list(items));
                    }
                    return Err(self.err("expected ',' or ']'"));
                }
            }
            '"' => Ok(Value::Str(self.string()?)),
            _ if self.word("true") => Ok(Value::Bool(true)),
            _ if self.word("false") => Ok(Value::Bool(false)),
            _ if self.word("null") => Ok(Value::None),
            _ => {
                let s = self.i;
                while self.i < self.c.len() && "+-0123456789.eE".contains(self.c[self.i]) {
                    self.i += 1;
                }
                let text: String = self.c[s..self.i].iter().collect();
                match text.parse::<f64>() {
                    Ok(n) if !text.is_empty() => Ok(Value::Num(n)),
                    _ => Err(self.err(&format!("unexpected '{}'", ch))),
                }
            }
        }
    }
    fn string(&mut self) -> Result<String, String> {
        if self.i >= self.c.len() || self.c[self.i] != '"' {
            return Err(self.err("expected text in quotes"));
        }
        self.i += 1;
        let mut s = String::new();
        while self.i < self.c.len() {
            let ch = self.c[self.i];
            self.i += 1;
            match ch {
                '"' => return Ok(s),
                '\\' => {
                    let Some(&e) = self.c.get(self.i) else { break };
                    self.i += 1;
                    match e {
                        'n' => s.push('\n'),
                        't' => s.push('\t'),
                        'r' => s.push('\r'),
                        'b' => s.push('\u{8}'),
                        'f' => s.push('\u{c}'),
                        'u' => {
                            let hex: String = self.c.iter().skip(self.i).take(4).collect();
                            self.i += 4;
                            let code = u32::from_str_radix(&hex, 16).map_err(|_| self.err("bad \\u escape"))?;
                            s.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                        }
                        other => s.push(other),
                    }
                }
                c => s.push(c),
            }
        }
        Err(self.err("text is missing its closing quote"))
    }
}
