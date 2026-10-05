//! Everyday programming tools: list helpers that take functions, text patterns, number formatting,
//! dates and times, CSV files, web requests (`fetch`) and small databases.
use crate::error::{EzaError, R};
use crate::interp::Interp;
use crate::value::*;
use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime, TimeZone, Timelike};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;

fn err<T>(it: &Interp, msg: impl Into<String>) -> R<T> {
    Err(EzaError::runtime(it.line, msg))
}

fn arg<'a>(it: &Interp, args: &'a [Value], i: usize, what: &str) -> R<&'a Value> {
    match args.get(i) {
        Some(v) => Ok(v),
        None => err(it, format!(".{} needs {} argument(s)", what, i + 1)),
    }
}

fn is_func(v: &Value) -> bool {
    matches!(v, Value::Func(_) | Value::Native(_) | Value::Data(_))
}

fn call1(it: &mut Interp, f: &Value, v: &Value) -> R<Value> {
    it.call_value(f.clone(), vec![v.clone()], vec![], vec![])
}

/// A dictionary used as a pattern: does `rec` have all of these fields with these values?
fn matches_pattern(rec: &Value, pat: &Obj) -> bool {
    match rec {
        Value::Obj(o) => pat.fields.iter().all(|(k, v)| o.get(k).is_some_and(|x| equals(x, v))),
        _ => false,
    }
}

/// A test given to filter/find/count/remove: a function, a dictionary pattern, or a plain value.
fn test(it: &mut Interp, how: &Value, v: &Value) -> R<bool> {
    Ok(match how {
        f if is_func(f) => call1(it, f, v)?.truthy(),
        Value::Obj(p) if p.type_name == "dict" => matches_pattern(v, p),
        x => equals(v, x),
    })
}

// ---------- lists ----------

pub fn list_method(it: &mut Interp, l: &[Value], name: &str, args: &[Value]) -> R<Option<Value>> {
    Ok(Some(match name {
        "filter" | "keep" => {
            let how = arg(it, args, 0, name)?.clone();
            let mut out = vec![];
            for v in l {
                if test(it, &how, v)? {
                    out.push(v.clone());
                }
            }
            Value::list(out)
        }
        "map" => {
            let f = arg(it, args, 0, name)?.clone();
            if !is_func(&f) {
                return err(it, ".map needs a function, like  nums.map(n -> n * 2)");
            }
            let mut out = Vec::with_capacity(l.len());
            for v in l {
                out.push(call1(it, &f, v)?);
            }
            Value::list(out)
        }
        "find" => {
            let how = arg(it, args, 0, name)?.clone();
            for v in l {
                if test(it, &how, v)? {
                    return Ok(Some(v.clone()));
                }
            }
            Value::None
        }
        // nums.count = how many items; nums.count(3) / nums.count(n -> n > 3) = how many match
        "count" if args.is_empty() => Value::Num(l.len() as f64),
        "count" => {
            let how = arg(it, args, 0, name)?.clone();
            let mut n = 0;
            for v in l {
                if test(it, &how, v)? {
                    n += 1;
                }
            }
            Value::Num(n as f64)
        }
        "any" | "all" => {
            let how = arg(it, args, 0, name)?.clone();
            let want_all = name == "all";
            for v in l {
                if test(it, &how, v)? != want_all {
                    return Ok(Some(Value::Bool(!want_all)));
                }
            }
            Value::Bool(want_all)
        }
        "unique" => {
            let mut out: Vec<Value> = vec![];
            for v in l {
                if !out.iter().any(|o| equals(o, v)) {
                    out.push(v.clone());
                }
            }
            Value::list(out)
        }
        "group_by" => {
            let f = arg(it, args, 0, name)?.clone();
            let mut groups = Obj::new("dict");
            for v in l {
                let key = if is_func(&f) {
                    call1(it, &f, v)?
                } else {
                    // group_by("city"): group dictionaries by one of their fields
                    match v {
                        Value::Obj(o) => o.get(&f.display()).cloned().unwrap_or(Value::None),
                        _ => Value::None,
                    }
                };
                let k = key_str(&key);
                let mut items = match groups.get(&k) {
                    Some(Value::List(x)) => (**x).clone(),
                    _ => vec![],
                };
                items.push(v.clone());
                groups.set(&k, Value::list(items));
            }
            Value::obj(groups)
        }
        "index_of" => {
            let x = arg(it, args, 0, name)?;
            Value::Num(l.iter().position(|v| equals(v, x)).map_or(-1.0, |i| i as f64))
        }
        _ => return Ok(None),
    }))
}

// ---------- text ----------

thread_local! {
    static PATTERNS: RefCell<HashMap<String, regex::Regex>> = RefCell::new(HashMap::new());
}

fn pattern(it: &Interp, p: &str) -> R<regex::Regex> {
    if let Some(r) = PATTERNS.with(|c| c.borrow().get(p).cloned()) {
        return Ok(r);
    }
    match regex::Regex::new(p) {
        Ok(r) => {
            PATTERNS.with(|c| c.borrow_mut().insert(p.to_string(), r.clone()));
            Ok(r)
        }
        Err(e) => err(it, format!("\"{}\" isn't a valid pattern: {}", p, e.to_string().lines().last().unwrap_or(""))),
    }
}

fn pad_char(it: &Interp, args: &[Value], name: &str) -> R<char> {
    match args.get(1) {
        Some(v) => v.display().chars().next().ok_or_else(|| EzaError::runtime(it.line, format!(".{} needs one character to pad with", name))),
        None => Ok(' '),
    }
}

pub fn string_method(it: &Interp, s: &str, name: &str, args: &[Value]) -> R<Option<Value>> {
    let strs = |v: Vec<&str>| Value::list(v.into_iter().map(|x| Value::Str(x.to_string())).collect());
    Ok(Some(match name {
        "lines" => strs(s.lines().collect()),
        "words" => strs(s.split_whitespace().collect()),
        "pad_left" | "pad_right" => {
            let width = arg(it, args, 0, name)?.as_num(it.line)?.max(0.0) as usize;
            let c = pad_char(it, args, name)?;
            let fill: String = std::iter::repeat(c).take(width.saturating_sub(s.chars().count())).collect();
            Value::Str(if name == "pad_left" { fill + s } else { s.to_string() + &fill })
        }
        // the WHOLE text fits the pattern (for checking input like emails or phone numbers)
        "matches" => {
            let p = arg(it, args, 0, name)?.display();
            let r = pattern(it, &format!("^(?:{})$", p))?;
            Value::Bool(r.is_match(s))
        }
        "find_all" => {
            let r = pattern(it, &arg(it, args, 0, name)?.display())?;
            strs(r.find_iter(s).map(|m| m.as_str()).collect())
        }
        "replace_pattern" => {
            let r = pattern(it, &arg(it, args, 0, name)?.display())?;
            Value::Str(r.replace_all(s, arg(it, args, 1, name)?.display().as_str()).into_owned())
        }
        _ => return Ok(None),
    }))
}

pub fn number_method(it: &Interp, n: f64, name: &str, args: &[Value]) -> R<Option<Value>> {
    Ok(Some(match name {
        // 3.14159.format(2) -> "3.14"
        "format" => {
            let d = match args.first() {
                Some(v) => v.as_num(it.line)?.clamp(0.0, 20.0) as usize,
                None => 2,
            };
            Value::Str(format!("{:.*}", d, n))
        }
        // 1234567.commas -> "1,234,567"
        "commas" => {
            let text = fmt_num(n);
            let (sign, rest) = text.strip_prefix('-').map_or(("", text.as_str()), |r| ("-", r));
            let (int, frac) = rest.split_once('.').map_or((rest, None), |(a, b)| (a, Some(b)));
            let mut out = String::new();
            for (i, c) in int.chars().enumerate() {
                if i > 0 && (int.len() - i) % 3 == 0 {
                    out.push(',');
                }
                out.push(c);
            }
            Value::Str(format!("{}{}{}", sign, out, frac.map(|f| format!(".{}", f)).unwrap_or_default()))
        }
        _ => return Ok(None),
    }))
}

// ---------- dates and times ----------

const WEEKDAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

fn date_value(d: NaiveDateTime) -> Value {
    let mut o = Obj::new("date");
    let n = |x: u32| Value::Num(x as f64);
    o.set("year", Value::Num(d.year() as f64));
    o.set("month", n(d.month()));
    o.set("day", n(d.day()));
    o.set("hour", n(d.hour()));
    o.set("minute", n(d.minute()));
    o.set("second", n(d.second()));
    o.set("weekday", Value::Str(WEEKDAYS[d.weekday().num_days_from_monday() as usize].into()));
    // seconds since 1970 (used to compare dates)
    let ts = Local.from_local_datetime(&d).earliest().map_or(d.and_utc().timestamp(), |t| t.timestamp());
    o.set("timestamp", Value::Num(ts as f64));
    Value::obj(o)
}

fn naive_of(o: &Obj) -> Option<NaiveDateTime> {
    let g = |k: &str| o.get(k).and_then(|v| v.as_num(0).ok()).unwrap_or(0.0);
    NaiveDate::from_ymd_opt(g("year") as i32, g("month") as u32, g("day") as u32)?.and_hms_opt(g("hour") as u32, g("minute") as u32, g("second") as u32)
}

pub fn date_text(o: &Obj) -> String {
    // plain days (midnight) show without a time
    naive_of(o).map_or("invalid date".into(), |d| {
        if d.time() == chrono::NaiveTime::MIN {
            d.format("%Y-%m-%d").to_string()
        } else {
            d.format("%Y-%m-%d %H:%M:%S").to_string()
        }
    })
}

fn parse_date(text: &str) -> Option<NaiveDateTime> {
    let t = text.trim();
    for f in ["%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M", "%Y/%m/%d %H:%M"] {
        if let Ok(d) = NaiveDateTime::parse_from_str(t, f) {
            return Some(d);
        }
    }
    for f in ["%Y-%m-%d", "%Y/%m/%d", "%d.%m.%Y"] {
        if let Ok(d) = NaiveDate::parse_from_str(t, f) {
            return d.and_hms_opt(0, 0, 0);
        }
    }
    // "2026-10-04T18:30:05.123Z" and other full timestamps from web APIs
    chrono::DateTime::parse_from_rfc3339(t).ok().map(|d| d.with_timezone(&Local).naive_local())
}

pub fn now() -> Value {
    let n = Local::now().naive_local();
    date_value(n.with_nanosecond(0).unwrap_or(n))
}

pub fn today() -> Value {
    date_value(Local::now().date_naive().and_hms_opt(0, 0, 0).unwrap())
}

/// date("2026-10-04"), date("2026-10-04 18:30"), or date(2026, 10, 4[, hour, minute, second])
pub fn date(it: &Interp, args: &[Value]) -> R<Value> {
    let d = match args {
        [Value::Str(s)] => parse_date(s).ok_or_else(|| {
            EzaError::runtime(it.line, format!("can't read \"{}\" as a date - write it like \"2026-10-04\" or \"2026-10-04 18:30\"", s))
        })?,
        [Value::Obj(o)] if o.type_name == "date" => return Ok(Value::Obj(o.clone())),
        nums if !nums.is_empty() && nums.len() <= 6 => {
            let v: Vec<f64> = nums.iter().map(|x| x.as_num(it.line)).collect::<R<_>>()?;
            let g = |i: usize, d: f64| v.get(i).copied().unwrap_or(d);
            NaiveDate::from_ymd_opt(g(0, 1970.0) as i32, g(1, 1.0) as u32, g(2, 1.0) as u32)
                .and_then(|d| d.and_hms_opt(g(3, 0.0) as u32, g(4, 0.0) as u32, g(5, 0.0) as u32))
                .ok_or_else(|| EzaError::runtime(it.line, "that date doesn't exist (check the month and day)"))?
        }
        _ => return err(it, "date needs text like \"2026-10-04\", or numbers: date(2026, 10, 4)"),
    };
    Ok(date_value(d))
}

/// "YYYY-MM-DD hh:mm" style formatting
fn format_date(d: NaiveDateTime, f: &str) -> String {
    let tokens: [(&str, String); 11] = [
        ("YYYY", format!("{:04}", d.year())),
        ("Month", MONTHS[d.month0() as usize].into()),
        ("Mon", MONTHS[d.month0() as usize][..3].into()),
        ("Weekday", WEEKDAYS[d.weekday().num_days_from_monday() as usize].into()),
        ("Wkd", WEEKDAYS[d.weekday().num_days_from_monday() as usize][..3].into()),
        ("YY", format!("{:02}", d.year() % 100)),
        ("MM", format!("{:02}", d.month())),
        ("DD", format!("{:02}", d.day())),
        ("hh", format!("{:02}", d.hour())),
        ("mm", format!("{:02}", d.minute())),
        ("ss", format!("{:02}", d.second())),
    ];
    let (mut out, mut rest) = (String::new(), f);
    'scan: while !rest.is_empty() {
        for (t, v) in &tokens {
            if let Some(r) = rest.strip_prefix(t) {
                out.push_str(v);
                rest = r;
                continue 'scan;
            }
        }
        let c = rest.chars().next().unwrap();
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

pub fn date_method(it: &Interp, o: &Obj, name: &str, args: &[Value]) -> R<Option<Value>> {
    let Some(d) = naive_of(o) else { return err(it, "this date is broken (a field like month or day was changed to something impossible)") };
    let amount = |it: &Interp| -> R<f64> { arg(it, args, 0, name)?.as_num(it.line) };
    let other = |it: &Interp| -> R<NaiveDateTime> {
        match arg(it, args, 0, name)? {
            Value::Obj(x) if x.type_name == "date" => naive_of(x).ok_or_else(|| EzaError::runtime(it.line, "the other date is broken")),
            Value::Str(s) => parse_date(s).ok_or_else(|| EzaError::runtime(it.line, format!("can't read \"{}\" as a date", s))),
            v => err(it, format!(".{} needs another date, got {}", name, v.type_name())),
        }
    };
    let add = |secs: f64| -> R<Value> {
        d.checked_add_signed(Duration::milliseconds((secs * 1000.0) as i64))
            .map(date_value)
            .ok_or_else(|| EzaError::runtime(it.line, "that date is too far away"))
    };
    Ok(Some(match name {
        "format" => Value::Str(format_date(d, &arg(it, args, 0, name)?.display())),
        "add_seconds" => add(amount(it)?)?,
        "add_minutes" => add(amount(it)? * 60.0)?,
        "add_hours" => add(amount(it)? * 3600.0)?,
        "add_days" => add(amount(it)? * 86400.0)?,
        "add_weeks" => add(amount(it)? * 604800.0)?,
        "add_months" | "add_years" => {
            let n = amount(it)? as i32 * if name == "add_years" { 12 } else { 1 };
            let m = chrono::Months::new(n.unsigned_abs());
            let r = if n >= 0 { d.checked_add_months(m) } else { d.checked_sub_months(m) };
            date_value(r.ok_or_else(|| EzaError::runtime(it.line, "that date is too far away"))?)
        }
        "days_until" => Value::Num((other(it)?.date() - d.date()).num_days() as f64),
        "seconds_until" => Value::Num((other(it)? - d).num_seconds() as f64),
        "date_only" => date_value(d.date().and_hms_opt(0, 0, 0).unwrap()),
        _ => return Ok(None),
    }))
}

// ---------- CSV ----------

/// Splits CSV text into rows of fields (quotes, commas and new lines inside quotes are handled).
fn csv_rows(text: &str) -> Vec<Vec<String>> {
    let (mut rows, mut row, mut field) = (vec![], vec![], String::new());
    let (mut quoted, mut chars) = (false, text.chars().peekable());
    while let Some(c) = chars.next() {
        match (quoted, c) {
            (true, '"') if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            (true, '"') => quoted = false,
            (true, c) => field.push(c),
            (false, '"') => quoted = true,
            (false, ',') => row.push(std::mem::take(&mut field)),
            (false, '\r') => {}
            (false, '\n') => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            (false, c) => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows.retain(|r| !(r.len() == 1 && r[0].trim().is_empty()));
    rows
}

fn csv_cell(s: &str) -> Value {
    let t = s.trim();
    match t.parse::<f64>() {
        Ok(n) if !t.is_empty() && !t.starts_with('+') && n.is_finite() => Value::Num(n),
        _ => match t {
            "true" | "TRUE" | "True" => Value::Bool(true),
            "false" | "FALSE" | "False" => Value::Bool(false),
            _ => Value::Str(s.to_string()),
        },
    }
}

/// The first row names the columns; every other row becomes a dictionary.
pub fn from_csv(text: &str) -> Value {
    let rows = csv_rows(text.trim_start_matches('\u{feff}'));
    let Some((head, body)) = rows.split_first() else { return Value::list(vec![]) };
    let out = body
        .iter()
        .map(|r| {
            let mut o = Obj::new("dict");
            for (i, k) in head.iter().enumerate() {
                o.set(k.trim(), r.get(i).map_or(Value::Str(String::new()), |c| csv_cell(c)));
            }
            Value::obj(o)
        })
        .collect();
    Value::list(out)
}

fn csv_quote(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) || s.starts_with(' ') || s.ends_with(' ') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// A list of dictionaries (one column per key) or a list of lists, as CSV text.
pub fn to_csv(it: &Interp, v: &Value) -> R<String> {
    let Value::List(rows) = v else { return err(it, "to save a .csv file, give a list of dictionaries (or a list of lists)") };
    let cell = |x: &Value| csv_quote(&x.display());
    let mut out = String::new();
    if rows.iter().all(|r| matches!(r, Value::Obj(_))) && !rows.is_empty() {
        let mut keys: Vec<String> = vec![];
        for r in rows.iter() {
            if let Value::Obj(o) = r {
                for (k, _) in &o.fields {
                    if !keys.contains(k) {
                        keys.push(k.clone());
                    }
                }
            }
        }
        out.push_str(&keys.iter().map(|k| csv_quote(k)).collect::<Vec<_>>().join(","));
        out.push('\n');
        for r in rows.iter() {
            if let Value::Obj(o) = r {
                let line: Vec<String> = keys.iter().map(|k| o.get(k).map(cell).unwrap_or_default()).collect();
                out.push_str(&line.join(","));
                out.push('\n');
            }
        }
        return Ok(out);
    }
    for r in rows.iter() {
        let line: Vec<String> = match r {
            Value::List(c) => c.iter().map(cell).collect(),
            other => vec![cell(other)],
        };
        out.push_str(&line.join(","));
        out.push('\n');
    }
    Ok(out)
}

// ---------- fetch ----------

/// fetch(url), fetch(url, send={...}), fetch(url, method="DELETE", headers={...})
pub fn fetch(it: &Interp, args: &[Value], named: &[(String, Value)]) -> R<Value> {
    let get = |i: usize, k: &str| named.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone()).or_else(|| args.get(i).cloned());
    for (k, _) in named {
        if !["url", "send", "method", "headers"].contains(&k.as_str()) {
            return err(it, format!("fetch doesn't have a '{}' setting (use send, method or headers)", k));
        }
    }
    let Some(url) = get(0, "url").map(|u| u.display()) else { return err(it, "fetch needs a web address, like fetch(\"https://...\")") };
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return err(it, format!("\"{}\" isn't a web address (it should start with https://)", url));
    }
    let send = get(1, "send");
    let method = get(9, "method").map(|m| m.display().to_uppercase()).unwrap_or_else(|| if send.is_some() { "POST".into() } else { "GET".into() });
    let tls = native_tls::TlsConnector::new().map_err(|e| EzaError::runtime(it.line, format!("can't set up secure connections: {}", e)))?;
    let agent = ureq::AgentBuilder::new()
        .tls_connector(std::sync::Arc::new(tls))
        .timeout(std::time::Duration::from_secs(20))
        .user_agent("eza")
        .build();
    let mut req = agent.request(&method, &url);
    if let Some(Value::Obj(h)) = get(9, "headers") {
        for (k, v) in &h.fields {
            req = req.set(k, &v.display());
        }
    }
    let result = match &send {
        None => req.call(),
        Some(Value::Str(s)) => req.set("Content-Type", "text/plain; charset=utf-8").send_string(s),
        Some(v) => {
            let body = crate::json::to_json(v).map_err(|m| EzaError::runtime(it.line, m))?;
            req.set("Content-Type", "application/json").send_string(&body)
        }
    };
    let resp = match result {
        Ok(r) => r,
        Err(ureq::Error::Status(code, r)) => {
            let text = r.into_string().unwrap_or_default();
            let snippet: String = text.chars().take(200).collect();
            let extra = if snippet.trim().is_empty() { String::new() } else { format!(": {}", snippet.trim()) };
            return err(it, format!("the server at {} answered with error {}{}", url, code, extra));
        }
        Err(e) => return err(it, format!("couldn't reach {}: {}", url, e)),
    };
    let is_json = resp.content_type().contains("json");
    let text = resp.into_string().map_err(|e| EzaError::runtime(it.line, format!("couldn't read the answer from {}: {}", url, e)))?;
    // JSON answers become dictionaries and lists; everything else stays text
    let trimmed = text.trim_start();
    if is_json || trimmed.starts_with('{') || trimmed.starts_with('[') {
        if let Ok(v) = crate::json::from_json(&text) {
            return Ok(v);
        }
    }
    Ok(Value::Str(text))
}

// ---------- databases ----------

/// database("todo.db"): records (dictionaries with an `id`) kept in a JSON file and saved after every change.
pub fn database(it: &mut Interp, args: &[Value]) -> R<Value> {
    let Some(name) = args.first().map(|v| v.display()) else { return err(it, "database needs a file name, like database(\"todo.db\")") };
    let full = it.resolve_path(&name);
    load_db(it, &full)?;
    let mut o = Obj::new("database");
    o.set("file", Value::Str(name));
    // remembered in full, so it's the same file even when used from an included script
    o.set("path", Value::Str(full.display().to_string()));
    Ok(Value::obj(o))
}

fn db_path(it: &Interp, o: &Obj) -> PathBuf {
    match o.get("path") {
        Some(Value::Str(p)) => PathBuf::from(p),
        _ => it.resolve_path(&o.get("file").map(|f| f.display()).unwrap_or_default()),
    }
}

fn load_db(it: &mut Interp, path: &PathBuf) -> R<()> {
    if it.dbs.contains_key(path) {
        return Ok(());
    }
    let records = if path.exists() {
        let text = std::fs::read_to_string(path).map_err(|e| EzaError::runtime(it.line, format!("can't open the database {}: {}", path.display(), e)))?;
        if text.trim().is_empty() {
            vec![]
        } else {
            match crate::json::from_json(&text) {
                Ok(Value::List(l)) => (*l).clone(),
                _ => return err(it, format!("{} isn't an Eza database (it should hold a list of records)", path.display())),
            }
        }
    } else {
        vec![]
    };
    it.dbs.insert(path.clone(), records);
    Ok(())
}

fn save_db(it: &Interp, path: &PathBuf) -> R<()> {
    let records = it.dbs.get(path).cloned().unwrap_or_default();
    let text = crate::json::to_json(&Value::list(records)).map_err(|m| EzaError::runtime(it.line, m))?;
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // write to a temporary file first, so a crash can't leave half a database
    let tmp = path.with_extension("eza-tmp");
    std::fs::write(&tmp, text)
        .and_then(|_| std::fs::rename(&tmp, path))
        .map_err(|e| EzaError::runtime(it.line, format!("can't save the database {}: {}", path.display(), e)))
}

fn id_of(v: &Value) -> Option<f64> {
    match v {
        Value::Num(n) => Some(*n),
        Value::Obj(o) => o.get("id").and_then(|i| i.as_num(0).ok()),
        _ => None,
    }
}

pub fn db_method(it: &mut Interp, o: &Obj, name: &str, args: &[Value]) -> R<Option<Value>> {
    let path = db_path(it, o);
    load_db(it, &path)?;
    let records = it.dbs.get(&path).cloned().unwrap_or_default();
    let changed = |it: &mut Interp, recs: Vec<Value>| -> R<()> {
        it.dbs.insert(path.clone(), recs);
        save_db(it, &path)
    };
    Ok(Some(match name {
        "all" => Value::list(records),
        "count" | "length" => match args.first() {
            None => Value::Num(records.len() as f64),
            Some(how) => {
                let how = how.clone();
                let mut n = 0;
                for r in &records {
                    if test(it, &how, r)? {
                        n += 1;
                    }
                }
                Value::Num(n as f64)
            }
        },
        "add" => {
            let Some(Value::Obj(rec)) = args.first() else { return err(it, ".add needs a record (a dictionary), like  notes.add({title: \"Buy milk\"})") };
            let next = records.iter().filter_map(id_of).fold(0.0, f64::max) + 1.0;
            let mut rec = (**rec).clone();
            rec.type_name = "dict".into();
            rec.fields.retain(|(k, _)| k != "id");
            rec.fields.insert(0, ("id".into(), Value::Num(next)));
            let rec = Value::obj(rec);
            let mut recs = records;
            recs.push(rec.clone());
            changed(it, recs)?;
            rec
        }
        "get" => {
            let id = id_of(arg(it, args, 0, name)?);
            records.into_iter().find(|r| id.is_some() && id_of(r) == id).unwrap_or(Value::None)
        }
        "find" | "first" => {
            let how = arg(it, args, 0, name)?.clone();
            let mut out = vec![];
            for r in records {
                if test(it, &how, &r)? {
                    if name == "first" {
                        return Ok(Some(r));
                    }
                    out.push(r);
                }
            }
            if name == "first" {
                Value::None
            } else {
                Value::list(out)
            }
        }
        "update" => {
            let target = arg(it, args, 0, name)?.clone();
            let Some(Value::Obj(changes)) = args.get(1) else { return err(it, ".update needs the changes as a dictionary, like  notes.update(note, {done: true})") };
            let id = id_of(&target);
            let mut recs = records;
            let mut updated = Value::None;
            for r in recs.iter_mut() {
                let hit = match id {
                    Some(_) => id_of(r) == id,
                    None => test(it, &target, r)?,
                };
                if let (true, Value::Obj(ro)) = (hit, &*r) {
                    let mut n = (**ro).clone();
                    for (k, v) in &changes.fields {
                        if k != "id" {
                            n.set(k, v.clone());
                        }
                    }
                    *r = Value::obj(n);
                    updated = r.clone();
                    if id.is_some() {
                        break;
                    }
                }
            }
            if matches!(updated, Value::None) {
                return err(it, "there's no record like that to update");
            }
            changed(it, recs)?;
            updated
        }
        "remove" => {
            let target = arg(it, args, 0, name)?.clone();
            let id = id_of(&target);
            let mut keep = vec![];
            let mut removed = 0;
            for r in records {
                let hit = match id {
                    Some(_) => id_of(&r) == id,
                    None => test(it, &target, &r)?,
                };
                if hit {
                    removed += 1;
                } else {
                    keep.push(r);
                }
            }
            changed(it, keep)?;
            Value::Num(removed as f64)
        }
        "clear" => {
            changed(it, vec![])?;
            Value::None
        }
        _ => return Ok(None),
    }))
}

// ---------- files and folders ----------

/// "*.csv" style patterns: * matches any run of characters, ? exactly one (case doesn't matter).
fn wildcard(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let n: Vec<char> = name.to_lowercase().chars().collect();
    let (mut pi, mut ni, mut star, mut mark) = (0, 0, None, 0);
    while ni < n.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == n[ni]) {
            pi += 1;
            ni += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ni;
            pi += 1;
        } else if let Some(s) = star {
            // let the last * swallow one more character and try again
            pi = s + 1;
            mark += 1;
            ni = mark;
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|c| *c == '*')
}

/// A path written the way the script wrote the folder: files("data") gives "data/a.csv".
fn join_shown(folder: &str, name: &str) -> String {
    if folder.is_empty() || folder == "." {
        name.to_string()
    } else {
        format!("{}/{}", folder.trim_end_matches(['/', '\\']), name)
    }
}

fn path_arg(it: &Interp, args: &[Value], i: usize, what: &str) -> R<String> {
    match args.get(i) {
        Some(v) => Ok(v.display()),
        None => err(it, format!("{} needs a path, like {}(\"notes.txt\")", what, what)),
    }
}

/// files(), files("photos"), files("photos", "*.png"); folders(...) works the same for folders.
pub fn list_dir(it: &Interp, args: &[Value], want_folders: bool) -> R<Value> {
    let folder = args.first().map(|v| v.display()).unwrap_or_else(|| ".".into());
    let pattern = args.get(1).map(|v| v.display());
    let rd = std::fs::read_dir(it.resolve_path(&folder))
        .map_err(|e| EzaError::runtime(it.line, format!("can't look inside the folder \"{}\": {}", folder, e)))?;
    let mut names = vec![];
    for entry in rd.flatten() {
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let name = entry.file_name().to_string_lossy().to_string();
        if is_dir == want_folders && pattern.as_ref().map_or(true, |p| wildcard(p, &name)) {
            names.push(name);
        }
    }
    names.sort_by_key(|n| n.to_lowercase());
    Ok(Value::list(names.into_iter().map(|n| Value::Str(join_shown(&folder, &n))).collect()))
}

/// find_files("photos", "*.png"): like files, but also looks inside every folder within.
pub fn find_files(it: &Interp, args: &[Value]) -> R<Value> {
    let folder = args.first().map(|v| v.display()).unwrap_or_else(|| ".".into());
    let pattern = args.get(1).map(|v| v.display()).unwrap_or_else(|| "*".into());
    fn walk(dir: &std::path::Path, shown: &str, pattern: &str, out: &mut Vec<String>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        let mut entries: Vec<_> = rd.flatten().collect();
        entries.sort_by_key(|e| e.file_name().to_string_lossy().to_lowercase());
        for e in entries {
            let name = e.file_name().to_string_lossy().to_string();
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                walk(&e.path(), &join_shown(shown, &name), pattern, out);
            } else if wildcard(pattern, &name) {
                out.push(join_shown(shown, &name));
            }
        }
    }
    let full = it.resolve_path(&folder);
    if !full.is_dir() {
        return err(it, format!("there's no folder \"{}\"", folder));
    }
    let mut out = vec![];
    walk(&full, &folder, &pattern, &mut out);
    Ok(Value::list(out.into_iter().map(Value::Str).collect()))
}

/// file_info("notes.txt"): name, extension, folder, size (bytes), modified (a date) and is_folder.
pub fn file_info(it: &Interp, args: &[Value]) -> R<Value> {
    let p = path_arg(it, args, 0, "file_info")?;
    let meta = std::fs::metadata(it.resolve_path(&p)).map_err(|e| EzaError::runtime(it.line, format!("can't find \"{}\": {}", p, e)))?;
    let path = std::path::Path::new(&p);
    let text = |s: Option<&std::ffi::OsStr>| Value::Str(s.map(|s| s.to_string_lossy().to_string()).unwrap_or_default());
    let mut o = Obj::new("dict");
    o.set("name", text(path.file_name()));
    o.set("extension", text(path.extension()));
    o.set("folder", Value::Str(path.parent().map(|d| d.display().to_string()).unwrap_or_default()));
    o.set("size", Value::Num(if meta.is_dir() { 0.0 } else { meta.len() as f64 }));
    o.set("is_folder", Value::Bool(meta.is_dir()));
    if let Ok(t) = meta.modified() {
        let d = chrono::DateTime::<Local>::from(t).naive_local();
        o.set("modified", date_value(d.with_nanosecond(0).unwrap_or(d)));
    }
    Ok(Value::obj(o))
}

fn copy_tree(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let target = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_tree(&e.path(), &target)?;
        } else {
            std::fs::copy(e.path(), target)?;
        }
    }
    Ok(())
}

/// make_folder, copy_file, move_file, delete_file and delete_folder.
pub fn file_command(it: &Interp, name: &str, args: &[Value]) -> R<Value> {
    let p = path_arg(it, args, 0, name)?;
    let full = it.resolve_path(&p);
    let fail = |e: std::io::Error| EzaError::runtime(it.line, format!("{} couldn't finish with \"{}\": {}", name, p, e));
    match name {
        "make_folder" => std::fs::create_dir_all(&full).map_err(fail)?,
        "copy_file" | "move_file" => {
            let to_text = match args.get(1) {
                Some(v) => v.display(),
                None => return err(it, format!("{} needs where to put it, like {}(\"a.txt\", \"backup/a.txt\")", name, name)),
            };
            if !full.exists() {
                return err(it, format!("there's no file or folder \"{}\"", p));
            }
            let mut to = it.resolve_path(&to_text);
            // copying into a folder that exists keeps the name
            if to.is_dir() {
                if let Some(n) = full.file_name() {
                    to = to.join(n);
                }
            }
            if let Some(parent) = to.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent).map_err(fail)?;
                }
            }
            if name == "copy_file" {
                if full.is_dir() {
                    copy_tree(&full, &to).map_err(fail)?;
                } else {
                    std::fs::copy(&full, &to).map_err(fail)?;
                }
            } else if std::fs::rename(&full, &to).is_err() {
                // moving to another drive: copy, then remove the original
                if full.is_dir() {
                    copy_tree(&full, &to).map_err(fail)?;
                    std::fs::remove_dir_all(&full).map_err(fail)?;
                } else {
                    std::fs::copy(&full, &to).map_err(fail)?;
                    std::fs::remove_file(&full).map_err(fail)?;
                }
            }
        }
        "delete_file" => {
            if full.is_dir() {
                return err(it, format!("\"{}\" is a folder - use delete_folder for folders", p));
            }
            if !full.exists() {
                return err(it, format!("there's no file \"{}\" to delete", p));
            }
            std::fs::remove_file(&full).map_err(fail)?;
        }
        "delete_folder" => {
            if !full.is_dir() {
                return err(it, format!("there's no folder \"{}\" to delete", p));
            }
            // a typo here could wipe out far too much, so a few folders are off limits
            let real = full.canonicalize().unwrap_or_else(|_| full.clone());
            let script_dir = it.resolve_path(".").canonicalize().ok();
            let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from).and_then(|h| h.canonicalize().ok());
            let off_limits = real.parent().is_none() || script_dir.as_ref().is_some_and(|d| d.starts_with(&real)) || home.as_ref().is_some_and(|h| h.starts_with(&real));
            if off_limits {
                return err(it, format!("delete_folder won't delete \"{}\" - it holds your script, your home folder or a whole drive", p));
            }
            std::fs::remove_dir_all(&full).map_err(fail)?;
        }
        _ => return err(it, format!("unknown file command '{}'", name)),
    }
    Ok(Value::None)
}

// ---------- running other programs ----------

/// run("git status"), run(["git", "status"]); input="..." types into the program, folder="..." runs
/// it somewhere else, show=true lets its output appear while it runs.
pub fn run_program(it: &Interp, args: &[Value], named: &[(String, Value)]) -> R<Value> {
    use std::io::Write as _;
    use std::process::{Command, Stdio};
    let mut input = None;
    let mut folder = it.resolve_path(".");
    let mut show = false;
    for (k, v) in named {
        match k.as_str() {
            "input" => input = Some(v.display()),
            "folder" => folder = it.resolve_path(&v.display()),
            "show" => show = v.truthy(),
            other => return err(it, format!("run doesn't have a '{}' setting (use input, folder or show)", other)),
        }
    }
    let (mut cmd, what) = match args.first() {
        Some(Value::List(parts)) if !parts.is_empty() => {
            let parts: Vec<String> = parts.iter().map(|p| p.display()).collect();
            let mut c = Command::new(&parts[0]);
            c.args(&parts[1..]);
            (c, parts.join(" "))
        }
        Some(v @ (Value::Str(_) | Value::Num(_))) => {
            let line = v.display();
            #[cfg(windows)]
            let c = {
                use std::os::windows::process::CommandExt;
                let mut c = Command::new("cmd");
                // passed through untouched, so quotes in the command line stay exactly as written
                c.arg("/C").raw_arg(&line);
                c
            };
            #[cfg(not(windows))]
            let c = {
                let mut c = Command::new("sh");
                c.arg("-c").arg(&line);
                c
            };
            (c, line)
        }
        _ => return err(it, "run needs a command, like run(\"git status\") or run([\"git\", \"status\"])"),
    };
    cmd.current_dir(&folder);
    cmd.stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() });
    if show {
        cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());
    } else {
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    }
    let mut child = cmd.spawn().map_err(|e| EzaError::runtime(it.line, format!("can't start \"{}\": {}", what, e)))?;
    if let (Some(text), Some(mut stdin)) = (input, child.stdin.take()) {
        let _ = stdin.write_all(text.as_bytes());
    }
    let out = child.wait_with_output().map_err(|e| EzaError::runtime(it.line, format!("\"{}\" stopped unexpectedly: {}", what, e)))?;
    let text = |b: &[u8]| Value::Str(String::from_utf8_lossy(b).replace("\r\n", "\n"));
    let code = out.status.code().unwrap_or(-1);
    let mut o = Obj::new("run_result");
    o.set("output", text(&out.stdout));
    o.set("errors", text(&out.stderr));
    o.set("code", Value::Num(code as f64));
    o.set("ok", Value::Bool(out.status.success()));
    Ok(Value::obj(o))
}
