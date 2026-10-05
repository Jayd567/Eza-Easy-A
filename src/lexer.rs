use crate::error::{EzaError, Target, R};

/// A syntax error that points at columns `from..to` of its line.
fn err_at(line: usize, from: usize, to: usize, msg: impl Into<String>) -> EzaError {
    let mut e = EzaError::syntax(line, msg);
    e.label(Target::Cols(from, to.max(from + 1)), "", true);
    e
}

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Num(f64),
    Str(String),
    Color(String),
    Ident(String),
    Sym(&'static str),
    Newline,
    Indent,
    Dedent,
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub tok: Tok,
    pub line: usize,
    /// where it sits on its line: first column and one past the last (counting characters from 0)
    pub col: usize,
    pub end: usize,
}

const SYMS2: [&str; 7] = ["==", "!=", "<=", ">=", "<<", ">>", "->"];
const SYMS1: [&str; 21] = ["+", "-", "*", "/", "%", "<", ">", "=", "(", ")", "[", "]", ",", ".", "&", "|", "^", "~", "{", "}", ":"];

/// `#1A1A1A` is a color, `# hello` is a comment.
fn color_end(c: &[char], i: usize) -> Option<usize> {
    let mut j = i + 1;
    while j < c.len() && c[j].is_ascii_hexdigit() {
        j += 1;
    }
    let ok_len = matches!(j - i - 1, 3 | 4 | 6 | 8);
    let ok_end = j >= c.len() || !(c[j].is_alphanumeric() || c[j] == '_');
    if ok_len && ok_end {
        Some(j)
    } else {
        None
    }
}

/// Words after which a value comes next, so a `#` there is a color, not a comment.
const VALUE_WORDS: &[&str] = &["to", "by", "in", "at", "and", "or", "not", "then", "else", "with", "return", "if", "while", "until", "push", "expect", "print", "from", "into"];

pub fn lex(src: &str) -> R<Vec<Token>> {
    // Windows editors (Notepad, PowerShell) may save a UTF-8 byte-order mark at the start
    let src = src.strip_prefix('\u{feff}').unwrap_or(src);
    let c: Vec<char> = src.chars().collect();
    let n = c.len();
    let mut toks: Vec<Token> = Vec::new();
    let mut indents = vec![0usize];
    let (mut i, mut line, mut depth, mut at_start) = (0usize, 1usize, 0i32, true);
    // index of the first character of the current line, for columns
    let mut line_start = 0usize;
    macro_rules! push {
        ($t:expr) => {
            toks.push(Token { tok: $t, line, col: 0, end: 0 })
        };
    }
    while i < n {
        if at_start && depth == 0 {
            let mut col = 0;
            while i < n && (c[i] == ' ' || c[i] == '\t') {
                col += if c[i] == '\t' { 4 } else { 1 };
                i += 1;
            }
            if i >= n {
                break;
            }
            if c[i] == '\r' {
                i += 1;
                continue;
            }
            if c[i] == '\n' {
                line += 1;
                i += 1;
                line_start = i;
                continue;
            }
            // a line starting with # is always a comment (a color on its own line would do nothing)
            if c[i] == '#' {
                while i < n && c[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            if col > *indents.last().unwrap() {
                indents.push(col);
                push!(Tok::Indent);
            } else {
                while col < *indents.last().unwrap() {
                    indents.pop();
                    push!(Tok::Dedent);
                }
                if col != *indents.last().unwrap() {
                    return Err(err_at(line, 0, i - line_start, "indentation doesn't line up with any outer block"));
                }
            }
            at_start = false;
        }
        let ch = c[i];
        let (start, before) = (i, toks.len());
        match ch {
            '\n' => {
                if depth == 0 {
                    if !matches!(toks.last().map(|t| &t.tok), Some(Tok::Newline) | None) {
                        push!(Tok::Newline);
                    }
                    at_start = true;
                }
                line += 1;
                i += 1;
                line_start = i;
            }
            ' ' | '\t' | '\r' => i += 1,
            '#' => {
                // `#FF0000` is a color only where a value can go (after `=`, `to`, `(` ...);
                // after a finished value it starts a comment, so `x = 5 #bad idea` works
                let value_expected = match toks.last().map(|t| &t.tok) {
                    Some(Tok::Sym(s)) => !matches!(*s, ")" | "]" | "}"),
                    Some(Tok::Ident(w)) => VALUE_WORDS.contains(&w.as_str()),
                    Some(Tok::Newline | Tok::Indent | Tok::Dedent) | None => false,
                    _ => false,
                };
                if let Some(j) = color_end(&c, i).filter(|_| value_expected) {
                    push!(Tok::Color(c[i + 1..j].iter().collect()));
                    i = j;
                } else {
                    while i < n && c[i] != '\n' {
                        i += 1;
                    }
                }
            }
            // 0b1010 (binary), 0x1F (hex), 0o17 (octal); `_` may separate digits
            '0' if i + 1 < n && matches!(c[i + 1], 'b' | 'x' | 'o' | 'B' | 'X' | 'O') => {
                let radix = match c[i + 1].to_ascii_lowercase() {
                    'b' => 2,
                    'o' => 8,
                    _ => 16,
                };
                i += 2;
                let s = i;
                while i < n && (c[i].is_digit(radix) || c[i] == '_') {
                    i += 1;
                }
                let digits: String = c[s..i].iter().filter(|ch| **ch != '_').collect();
                match i64::from_str_radix(&digits, radix) {
                    Ok(v) if !digits.is_empty() => push!(Tok::Num(v as f64)),
                    _ => return Err(err_at(line, start - line_start, i - line_start, "this number literal isn't valid")),
                }
            }
            '0'..='9' => {
                let s = i;
                // 1_000_000: `_` may separate digits here too
                while i < n && (c[i].is_ascii_digit() || (c[i] == '_' && i + 1 < n && c[i + 1].is_ascii_digit())) {
                    i += 1;
                }
                if i + 1 < n && c[i] == '.' && c[i + 1].is_ascii_digit() {
                    i += 1;
                    while i < n && c[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                let text: String = c[s..i].iter().filter(|ch| **ch != '_').collect();
                push!(Tok::Num(text.parse().unwrap()));
            }
            // straight quotes plus the “smart quotes” word processors insert
            '"' | '\'' | '\u{201C}' | '\u{201D}' | '\u{2018}' => {
                let closers: &[char] = match ch {
                    '"' => &['"'],
                    '\'' => &['\''],
                    '\u{2018}' => &['\u{2019}', '\''],
                    _ => &['\u{201D}', '\u{201C}', '"'],
                };
                i += 1;
                let mut s = String::new();
                loop {
                    if i >= n || c[i] == '\n' {
                        return Err(err_at(line, start - line_start, i - line_start, "text is missing its closing quote"));
                    }
                    if closers.contains(&c[i]) {
                        i += 1;
                        break;
                    }
                    if c[i] == '\\' && i + 1 < n {
                        i += 1;
                        s.push(match c[i] {
                            'n' => '\n',
                            't' => '\t',
                            other => other,
                        });
                    } else {
                        s.push(c[i]);
                    }
                    i += 1;
                }
                push!(Tok::Str(s));
            }
            ch if ch.is_alphabetic() || ch == '_' => {
                let s = i;
                while i < n && (c[i].is_alphanumeric() || c[i] == '_') {
                    i += 1;
                }
                push!(Tok::Ident(c[s..i].iter().collect()));
            }
            _ => {
                let two: String = c[i..(i + 2).min(n)].iter().collect();
                if let Some(s) = SYMS2.iter().find(|s| **s == two) {
                    push!(Tok::Sym(s));
                    i += 2;
                } else if let Some(s) = SYMS1.iter().find(|s| s.starts_with(ch)) {
                    match ch {
                        '(' | '[' | '{' => depth += 1,
                        ')' | ']' | '}' => depth -= 1,
                        _ => {}
                    }
                    push!(Tok::Sym(s));
                    i += 1;
                } else {
                    return Err(err_at(line, start - line_start, start - line_start + 1, format!("unexpected character '{}'", ch)));
                }
            }
        }
        // where the token(s) made in this round sit on their line
        if toks.len() > before && start >= line_start {
            for t in &mut toks[before..] {
                t.col = start - line_start;
                t.end = i.saturating_sub(line_start).max(t.col + 1);
            }
        }
    }
    if !matches!(toks.last().map(|t| &t.tok), Some(Tok::Newline) | None) {
        push!(Tok::Newline);
    }
    while indents.len() > 1 {
        indents.pop();
        push!(Tok::Dedent);
    }
    push!(Tok::Eof);
    Ok(toks)
}

#[cfg(test)]
mod tests {
    use super::{lex, Tok};

    fn colors(src: &str) -> usize {
        lex(src).unwrap().iter().filter(|t| matches!(t.tok, Tok::Color(_))).count()
    }

    #[test]
    fn hash_is_a_comment_after_a_value_and_a_color_where_a_value_goes() {
        assert_eq!(colors("x = 5 #bad idea\n"), 0);
        assert_eq!(colors("#face\n"), 0);
        assert_eq!(colors("c = #FF0000\nchange c to #fff\nl = [#000, #111]\n"), 4);
    }
}
