//! Nicer terminal output: colored `print`, `table(...)`, `panel(...)`, progress bars for
//! `each x in progress(list)`, `input(..., choices=[...])` with an arrow-key menu, and `clear()`.
//! When the output isn't a terminal (piped into a file or another program), the text stays plain.
use crate::error::{EzaError, R};
use crate::value::*;
use crossterm::style::{Attribute, Color, Stylize};
use std::io::{IsTerminal, Write};

/// Is the output a real terminal (so colors and redrawing make sense)?
pub fn is_tty() -> bool {
    std::io::stdout().is_terminal()
}

fn input_is_tty() -> bool {
    std::io::stdin().is_terminal() && is_tty()
}

/// Turns on color support once (old Windows consoles need to be asked).
fn init() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        if is_tty() {
            let _ = crossterm::execute!(std::io::stdout(), crossterm::style::ResetColor);
        }
    });
}

/// "green", "#FF8800", or a color value like #FF8800.
pub fn color_of(v: &Value) -> Option<Color> {
    match v {
        Value::Color(c) => Some(Color::Rgb { r: c[0] as u8, g: c[1] as u8, b: c[2] as u8 }),
        Value::Str(s) => {
            let s = s.trim().to_lowercase();
            if let Some(hex) = s.strip_prefix('#') {
                let full: String = if hex.len() == 3 { hex.chars().flat_map(|c| [c, c]).collect() } else { hex.to_string() };
                if full.len() >= 6 {
                    let p = |i: usize| u8::from_str_radix(&full[i..i + 2], 16).ok();
                    return Some(Color::Rgb { r: p(0)?, g: p(2)?, b: p(4)? });
                }
                return None;
            }
            Some(match s.as_str() {
                "black" => Color::Black,
                "red" => Color::Red,
                "green" => Color::Green,
                "yellow" => Color::Yellow,
                "blue" => Color::Blue,
                "magenta" | "purple" => Color::Magenta,
                "cyan" => Color::Cyan,
                "white" => Color::White,
                "gray" | "grey" => Color::DarkGrey,
                "orange" => Color::Rgb { r: 255, g: 165, b: 0 },
                "pink" => Color::Rgb { r: 255, g: 105, b: 180 },
                "dark_red" => Color::DarkRed,
                "dark_green" => Color::DarkGreen,
                "dark_blue" => Color::DarkBlue,
                _ => return None,
            })
        }
        _ => None,
    }
}

pub const STYLE_NAMES: [&str; 6] = ["color", "background", "bold", "italic", "underline", "dim"];
const COLOR_WORDS: &str = "red, green, blue, yellow, orange, purple, pink, cyan, white, gray, black, or a color like #FF8800";

/// The text with print's style settings (color=, background=, bold=, italic=, underline=, dim=).
pub fn styled(line: usize, text: &str, named: &[(String, Value)]) -> R<String> {
    for (k, v) in named {
        if !STYLE_NAMES.contains(&k.as_str()) {
            let hint = crate::suggest::closest(k, &STYLE_NAMES.iter().map(|s| s.to_string()).collect::<Vec<_>>())
                .map(|c| format!(" - did you mean '{}'?", c))
                .unwrap_or_default();
            return Err(EzaError::runtime(line, format!("print doesn't have a setting called '{}'{} (it has: {})", k, hint, STYLE_NAMES.join(", "))));
        }
        if (k == "color" || k == "background") && color_of(v).is_none() {
            return Err(EzaError::runtime(line, format!("{}={} isn't a color I know - use {}", k, v.repr(), COLOR_WORDS)));
        }
    }
    if !is_tty() {
        return Ok(text.to_string());
    }
    init();
    let mut s = text.to_string().stylize();
    for (k, v) in named {
        s = match k.as_str() {
            "color" => s.with(color_of(v).unwrap()),
            "background" => s.on(color_of(v).unwrap()),
            "bold" if v.truthy() => s.attribute(Attribute::Bold),
            "italic" if v.truthy() => s.attribute(Attribute::Italic),
            "underline" if v.truthy() => s.attribute(Attribute::Underlined),
            "dim" if v.truthy() => s.attribute(Attribute::Dim),
            _ => s,
        };
    }
    Ok(s.to_string())
}

fn width(s: &str) -> usize {
    s.chars().count()
}

fn cell_text(v: &Value) -> String {
    match v {
        Value::None => String::new(),
        other => other.display(),
    }
}

/// table(rows): a list of dictionaries (or objects) uses their keys as the header;
/// a list of lists takes header=[...] for one.
pub fn table(line: usize, rows: &Value, header: Option<&Value>) -> R<String> {
    let Value::List(items) = crate::interp::deref_val(rows.clone()) else {
        return Err(EzaError::runtime(line, format!("table needs a list of rows, got {}", rows.type_name())));
    };
    let mut head: Vec<String> = match header {
        Some(Value::List(h)) => h.iter().map(cell_text).collect(),
        Some(other) => return Err(EzaError::runtime(line, format!("header= needs a list of column names, got {}", other.type_name()))),
        None => vec![],
    };
    let keyed = items.iter().all(|r| matches!(crate::interp::deref_val(r.clone()), Value::Obj(_)));
    if keyed && head.is_empty() {
        for r in items.iter() {
            if let Value::Obj(o) = crate::interp::deref_val(r.clone()) {
                for (k, _) in &o.fields {
                    if !head.contains(k) {
                        head.push(k.clone());
                    }
                }
            }
        }
    }
    let mut body: Vec<Vec<(String, bool)>> = vec![];
    for r in items.iter() {
        let cells: Vec<Value> = match crate::interp::deref_val(r.clone()) {
            Value::Obj(o) => head.iter().map(|k| o.get(k).cloned().unwrap_or(Value::None)).collect(),
            Value::List(l) => (*l).clone(),
            other => vec![other],
        };
        body.push(cells.iter().map(|c| (cell_text(c), matches!(c, Value::Num(_)))).collect());
    }
    let cols = body.iter().map(|r| r.len()).max().unwrap_or(0).max(head.len());
    if cols == 0 {
        return Ok("(empty table)".to_string());
    }
    let mut w = vec![0usize; cols];
    for (i, h) in head.iter().enumerate() {
        w[i] = w[i].max(width(h));
    }
    for r in &body {
        for (i, (c, _)) in r.iter().enumerate() {
            w[i] = w[i].max(width(c));
        }
    }
    let rule = |l: &str, m: &str, r: &str| format!("{}{}{}", l, w.iter().map(|n| "─".repeat(n + 2)).collect::<Vec<_>>().join(m), r);
    let row = |cells: &[(String, bool)]| {
        let mut out = String::from("│");
        for i in 0..cols {
            let (text, number) = cells.get(i).cloned().unwrap_or_default();
            let pad = w[i] - width(&text);
            if number {
                out += &format!(" {}{} │", " ".repeat(pad), text);
            } else {
                out += &format!(" {}{} │", text, " ".repeat(pad));
            }
        }
        out
    };
    let mut out = vec![rule("┌", "┬", "┐")];
    if !head.is_empty() {
        out.push(row(&head.iter().map(|h| (h.clone(), false)).collect::<Vec<_>>()));
        out.push(rule("├", "┼", "┤"));
    }
    for r in &body {
        out.push(row(r));
    }
    out.push(rule("└", "┴", "┘"));
    Ok(out.join("\n"))
}

/// panel(text, title="Note"): the text in a box with rounded corners.
pub fn panel(text: &str, title: Option<&str>) -> String {
    let lines: Vec<&str> = if text.is_empty() { vec![""] } else { text.lines().collect() };
    let inner = lines.iter().map(|l| width(l)).max().unwrap_or(0).max(title.map_or(0, |t| width(t) + 2));
    let top = match title {
        Some(t) => format!("╭─ {} {}╮", t, "─".repeat(inner - width(t) - 1)),
        None => format!("╭{}╮", "─".repeat(inner + 2)),
    };
    let mut out = vec![top];
    for l in lines {
        out.push(format!("│ {}{} │", l, " ".repeat(inner - width(l))));
    }
    out.push(format!("╰{}╯", "─".repeat(inner + 2)));
    out.join("\n")
}

/// Draws (or redraws) a progress bar on the current line.
pub fn draw_progress(label: &str, done: usize, total: usize) {
    if !is_tty() {
        return;
    }
    init();
    let frac = if total == 0 { 1.0 } else { done as f64 / total as f64 };
    let n = 30;
    let filled = (frac * n as f64).round() as usize;
    let bar = format!("{}{}", "█".repeat(filled), "░".repeat(n - filled));
    let label = if label.is_empty() { String::new() } else { format!("{} ", label) };
    let mut out = std::io::stdout();
    let _ = write!(out, "\r{}{} {}/{} {:>3}%", label, bar.green(), done, total, (frac * 100.0).round() as i64);
    if done >= total {
        let _ = writeln!(out);
    }
    let _ = out.flush();
}

/// clear(): an empty terminal, with the cursor at the top.
pub fn clear() {
    if is_tty() {
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
            crossterm::terminal::Clear(crossterm::terminal::ClearType::Purge),
            crossterm::cursor::MoveTo(0, 0)
        );
    }
}

/// input("Pick one", choices=[...]): an arrow-key menu (or a numbered list when not in a terminal).
pub fn choose(line: usize, prompt: &str, choices: &[Value]) -> R<Value> {
    if choices.is_empty() {
        return Err(EzaError::runtime(line, "choices= needs at least one thing to pick"));
    }
    if !input_is_tty() {
        if !prompt.is_empty() {
            println!("{}", prompt);
        }
        for (i, c) in choices.iter().enumerate() {
            println!("  {}. {}", i + 1, c.display());
        }
        loop {
            print!("> ");
            let _ = std::io::stdout().flush();
            let mut s = String::new();
            if std::io::stdin().read_line(&mut s).unwrap_or(0) == 0 {
                return Ok(choices[0].clone());
            }
            let s = s.trim();
            if let Ok(k) = s.parse::<usize>() {
                if (1..=choices.len()).contains(&k) {
                    return Ok(choices[k - 1].clone());
                }
            }
            if let Some(c) = choices.iter().find(|c| c.display().eq_ignore_ascii_case(s)) {
                return Ok(c.clone());
            }
            println!("type a number from 1 to {}", choices.len());
        }
    }
    use crossterm::event::{read, Event, KeyCode, KeyEventKind, KeyModifiers};
    use crossterm::{cursor, terminal, ExecutableCommand};
    init();
    let mut out = std::io::stdout();
    if !prompt.is_empty() {
        println!("{}", prompt.bold());
    }
    let mut at = 0usize;
    let draw = |out: &mut std::io::Stdout, at: usize, first: bool| {
        if !first {
            let _ = out.execute(cursor::MoveUp(choices.len() as u16));
        }
        for (i, c) in choices.iter().enumerate() {
            let _ = out.execute(terminal::Clear(terminal::ClearType::CurrentLine));
            if i == at {
                let _ = write!(out, "\r{}\r\n", format!(" > {}", c.display()).cyan().bold());
            } else {
                let _ = write!(out, "\r   {}\r\n", c.display());
            }
        }
        let _ = out.flush();
    };
    let _ = out.execute(cursor::Hide);
    let _ = terminal::enable_raw_mode();
    draw(&mut out, at, true);
    let picked = loop {
        let Ok(Event::Key(k)) = read() else { continue };
        if k.kind != KeyEventKind::Press {
            continue;
        }
        match k.code {
            KeyCode::Up | KeyCode::Char('k') => at = (at + choices.len() - 1) % choices.len(),
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => at = (at + 1) % choices.len(),
            KeyCode::Enter | KeyCode::Char(' ') => break Some(at),
            KeyCode::Char(d) if d.is_ascii_digit() && d != '0' && (d as usize - '0' as usize) <= choices.len() => {
                break Some(d as usize - '1' as usize)
            }
            KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => break None,
            KeyCode::Esc => break None,
            _ => {}
        }
        draw(&mut out, at, false);
    };
    let _ = terminal::disable_raw_mode();
    let _ = out.execute(cursor::Show);
    match picked {
        Some(i) => {
            draw(&mut out, i, false);
            Ok(choices[i].clone())
        }
        None => Err(EzaError::runtime(line, "the choice was cancelled (Esc or Ctrl+C)")),
    }
}

/// input("Password", hidden=true): typing shows * instead of the letters.
pub fn hidden(prompt: &str) -> String {
    use crossterm::event::{read, Event, KeyCode, KeyEventKind, KeyModifiers};
    print!("{}", prompt);
    let _ = std::io::stdout().flush();
    if !input_is_tty() {
        let mut s = String::new();
        std::io::stdin().read_line(&mut s).ok();
        return s.trim_end_matches(['\r', '\n']).to_string();
    }
    let _ = crossterm::terminal::enable_raw_mode();
    let mut s = String::new();
    loop {
        let Ok(Event::Key(k)) = read() else { continue };
        if k.kind != KeyEventKind::Press {
            continue;
        }
        match k.code {
            KeyCode::Enter => break,
            KeyCode::Backspace => {
                if s.pop().is_some() {
                    print!("\u{8} \u{8}");
                }
            }
            KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => break,
            KeyCode::Char(ch) => {
                s.push(ch);
                print!("*");
            }
            _ => {}
        }
        let _ = std::io::stdout().flush();
    }
    let _ = crossterm::terminal::disable_raw_mode();
    println!();
    s
}
