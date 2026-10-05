//! `serve`: a small web server. Pages are blocks of Eza that `return` what the browser gets:
//! text (HTML when it starts with `<`), or a list / dictionary (sent as JSON). Inside a page,
//! `request` holds what came in and `response` can be changed (status, type, headers).
use crate::ast::Stmt;
use crate::interp::{Interp, Scope};
use crate::value::*;
use std::io::Read;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct Page {
    pub method: String,
    pub path: String,
    pub body: Rc<Vec<Stmt>>,
    pub scope: Rc<Scope>,
    pub file: u16,
}

#[derive(Clone)]
pub struct ServerDef {
    pub port: u16,
    /// files in this folder are served as they are (pictures, CSS, ...)
    pub folder: Option<PathBuf>,
    /// share=true: other devices on the same network can open it too
    pub share: bool,
    pub pages: Vec<Page>,
}

pub struct Server {
    http: tiny_http::Server,
    def: ServerDef,
}

/// The names `{like_this}` in a page's path.
pub fn path_params(path: &str) -> Vec<String> {
    path.split('/').filter_map(|s| s.strip_prefix('{').and_then(|s| s.strip_suffix('}'))).map(String::from).collect()
}

fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() && b[i + 1].is_ascii_hexdigit() && b[i + 2].is_ascii_hexdigit() => {
                let hex = |c: u8| (c as char).to_digit(16).unwrap_or(0) as u8;
                out.push(hex(b[i + 1]) * 16 + hex(b[i + 2]));
                i += 2;
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// a=1&b=two -> {a: "1", b: "two"}
fn form(s: &str) -> Value {
    let mut o = Obj::new("dict");
    for pair in s.split('&').filter(|p| !p.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        o.set(&decode(k), Value::Str(decode(v)));
    }
    Value::obj(o)
}

/// Does `/hello/{name}` fit `/hello/ada`? Gives the {names} and their values.
fn fits(pattern: &str, path: &str) -> Option<Vec<(String, Value)>> {
    let a: Vec<&str> = pattern.trim_end_matches('/').split('/').collect();
    let b: Vec<&str> = path.trim_end_matches('/').split('/').collect();
    if a.len() != b.len() {
        return None;
    }
    let mut params = vec![];
    for (p, s) in a.iter().zip(&b) {
        match p.strip_prefix('{').and_then(|x| x.strip_suffix('}')) {
            Some(name) if !s.is_empty() => params.push((name.to_string(), Value::Str(decode(s)))),
            Some(_) => return None,
            None if p == s => {}
            None => return None,
        }
    }
    Some(params)
}

fn mime(ext: &str) -> &'static str {
    match ext.to_lowercase().as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" => "application/json",
        "txt" | "md" | "eza" | "csv" => "text/plain; charset=utf-8",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "webp" => "image/webp",
        "wasm" => "application/wasm",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "woff2" => "font/woff2",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}

/// `change response.type to "json"`: short names for the common kinds.
fn type_name_to_mime(t: &str) -> String {
    match t {
        "html" => "text/html; charset=utf-8".into(),
        "text" => "text/plain; charset=utf-8".into(),
        "json" => "application/json".into(),
        "css" => "text/css; charset=utf-8".into(),
        "js" | "javascript" => "text/javascript; charset=utf-8".into(),
        "csv" => "text/csv; charset=utf-8".into(),
        other => other.to_string(),
    }
}

/// This computer's address on the local network (no data is sent; it only asks the OS for the route).
fn lan_address() -> Option<String> {
    let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("8.8.8.8:80").ok()?;
    Some(s.local_addr().ok()?.ip().to_string())
}

impl Server {
    pub fn start(def: ServerDef) -> Result<Server, String> {
        let host = if def.share { "0.0.0.0" } else { "127.0.0.1" };
        let http = tiny_http::Server::http((host, def.port)).map_err(|e| {
            let text = e.to_string();
            if text.contains("10048") || text.contains("10013") || text.to_lowercase().contains("in use") {
                format!("port {} is already used by another program on this computer - pick another one, like  serve port={}", def.port, def.port + 1)
            } else {
                format!("couldn't start the web server on port {}: {}", def.port, text)
            }
        })?;
        let local = format!("http://localhost:{}", def.port);
        println!("{}", crate::term::styled(0, &format!("Serving on {}", local), &[("bold".into(), Value::Bool(true)), ("color".into(), Value::Str("green".into()))]).unwrap_or(local.clone()));
        if def.share {
            if let Some(ip) = lan_address() {
                println!("Other devices on your network can open http://{}:{}", ip, def.port);
            }
        }
        println!("Press Ctrl+C to stop.");
        Ok(Server { http, def })
    }

    /// Answers the requests that are waiting (waits up to `wait` for one).
    pub fn poll(&self, it: &mut Interp, wait: Duration) {
        let mut wait = wait;
        while let Ok(Some(req)) = self.http.recv_timeout(wait) {
            self.answer(it, req);
            wait = Duration::ZERO;
            if it.quitting {
                return;
            }
        }
    }

    fn answer(&self, it: &mut Interp, mut req: tiny_http::Request) {
        let started = Instant::now();
        let method = req.method().as_str().to_uppercase();
        let url = req.url().to_string();
        let (path, query) = url.split_once('?').unwrap_or((&url, ""));
        let path = if path.is_empty() { "/".to_string() } else { path.to_string() };
        let mut headers = Obj::new("dict");
        let mut ctype_in = String::new();
        for h in req.headers() {
            let k = h.field.as_str().as_str().to_lowercase();
            if k == "content-type" {
                ctype_in = h.value.as_str().to_lowercase();
            }
            headers.set(&k, Value::Str(h.value.as_str().to_string()));
        }
        let mut body = String::new();
        let _ = req.as_reader().take(16 << 20).read_to_string(&mut body);
        let mut r = Obj::new("request");
        r.set("method", Value::Str(method.clone()));
        r.set("path", Value::Str(decode(&path)));
        r.set("query", form(query));
        r.set("headers", Value::obj(headers));
        r.set("form", if ctype_in.contains("x-www-form-urlencoded") { form(&body) } else { Value::obj(Obj::new("dict")) });
        r.set("data", if ctype_in.contains("json") { crate::json::from_json(&body).unwrap_or(Value::None) } else { Value::None });
        r.set("body", Value::Str(body));
        let request = Value::obj(r);

        let mut wrong_method = false;
        let mut found = None;
        for (i, p) in self.def.pages.iter().enumerate() {
            if let Some(params) = fits(&p.path, &path) {
                if p.method == method || (method == "HEAD" && p.method == "GET") {
                    found = Some((i, params));
                    break;
                }
                wrong_method = true;
            }
        }
        let (status, ctype, bytes, extra) = match found {
            Some((i, params)) => match it.run_page(i, request, params) {
                Ok((v, resp)) => build(v, resp),
                Err(e) => {
                    if e.is_switch() {
                        (200, "text/plain; charset=utf-8".to_string(), b"".to_vec(), vec![])
                    } else {
                        eprintln!("{}\n", e);
                        let msg = format!("Something went wrong in the page for {}:\n\n{}", path, e.headline());
                        (500, "text/plain; charset=utf-8".to_string(), msg.into_bytes(), vec![])
                    }
                }
            },
            None if wrong_method => (405, "text/plain; charset=utf-8".into(), format!("{} doesn't take {} requests", path, method).into_bytes(), vec![]),
            None => match self.file_for(&path) {
                Some(f) => {
                    let ext = f.extension().and_then(|e| e.to_str()).unwrap_or("").to_string();
                    match std::fs::read(&f) {
                        Ok(b) => (200, mime(&ext).to_string(), b, vec![]),
                        Err(_) => not_found(&path),
                    }
                }
                None => not_found(&path),
            },
        };
        let mut resp = tiny_http::Response::from_data(bytes).with_status_code(status);
        if let Ok(h) = tiny_http::Header::from_bytes("Content-Type", ctype.as_bytes()) {
            resp = resp.with_header(h);
        }
        for (k, v) in extra {
            if let Ok(h) = tiny_http::Header::from_bytes(k.as_bytes(), v.as_bytes()) {
                resp = resp.with_header(h);
            }
        }
        let _ = req.respond(resp);
        let ms = started.elapsed().as_millis();
        let line = format!("{} {}  {}  {} ms", method, url, status, ms);
        let color = if status >= 500 { "red" } else if status >= 400 { "yellow" } else { "gray" };
        println!("{}", crate::term::styled(0, &line, &[("color".into(), Value::Str(color.into()))]).unwrap_or(line));
    }

    /// A file from `folder=` for this path (index.html for a folder); never outside the folder.
    fn file_for(&self, path: &str) -> Option<PathBuf> {
        let base = self.def.folder.as_ref()?;
        let rel = decode(path);
        if rel.split(['/', '\\']).any(|s| s == "..") {
            return None;
        }
        let mut f = base.join(rel.trim_start_matches('/'));
        if f.is_dir() {
            f = f.join("index.html");
        }
        f.is_file().then_some(f)
    }
}

fn not_found(path: &str) -> (u16, String, Vec<u8>, Vec<(String, String)>) {
    let html = format!(
        "<!doctype html><title>Not found</title><body style=\"font-family:sans-serif;padding:2em\"><h1>Not found</h1><p>There's no page at <code>{}</code>.</p></body>",
        path.replace('<', "&lt;")
    );
    (404, "text/html; charset=utf-8".into(), html.into_bytes(), vec![])
}

/// What a page returned, plus its `response` changes -> status, content type, body, headers.
fn build(v: Value, resp: Value) -> (u16, String, Vec<u8>, Vec<(String, String)>) {
    let (mut status, mut ctype, mut headers) = (200u16, String::new(), vec![]);
    if let Value::Obj(r) = &resp {
        if let Some(Value::Num(n)) = r.get("status") {
            status = (*n as i64).clamp(100, 599) as u16;
        }
        if let Some(Value::Str(t)) = r.get("type") {
            ctype = type_name_to_mime(t);
        }
        if let Some(Value::Obj(h)) = r.get("headers") {
            for (k, v) in &h.fields {
                headers.push((k.clone(), v.display()));
            }
        }
    }
    let v = crate::interp::deref_val(v);
    let (default_type, bytes) = match &v {
        Value::Str(s) => (
            if s.trim_start().starts_with('<') { "text/html; charset=utf-8" } else { "text/plain; charset=utf-8" },
            s.clone().into_bytes(),
        ),
        Value::None => ("text/plain; charset=utf-8", vec![]),
        Value::List(_) | Value::Obj(_) => match crate::json::to_json(&v) {
            Ok(j) => ("application/json", j.into_bytes()),
            Err(e) => {
                status = 500;
                ("text/plain; charset=utf-8", format!("this page's answer can't be sent as JSON: {}", e).into_bytes())
            }
        },
        other => ("text/plain; charset=utf-8", other.display().into_bytes()),
    };
    if ctype.is_empty() {
        ctype = default_type.to_string();
    }
    (status, ctype, bytes, headers)
}

/// Runs the server until the program is stopped (Ctrl+C) or a page uses `quit`.
/// `on every frame`, timers and `wait` keep going in between requests.
pub fn run(it: &mut Interp) -> i32 {
    let Some(def) = it.server.clone() else { return 0 };
    // like a game window: an error in an `on` block switches that block off instead of stopping everything
    it.frame_mode = true;
    let server = match Server::start(def) {
        Ok(s) => s,
        Err(msg) => {
            eprintln!("[Runtime Error] {}", msg);
            return 1;
        }
    };
    let frame = Duration::from_micros(16_667);
    let mut next = Instant::now() + frame;
    loop {
        let now = Instant::now();
        server.poll(it, next.saturating_duration_since(now));
        if it.quitting {
            return 0;
        }
        if Instant::now() >= next {
            next += frame;
            if Instant::now() > next + frame * 10 {
                next = Instant::now() + frame; // fell far behind (a slow page): don't try to catch up
            }
            let r = it.tick();
            for e in it.take_errors() {
                eprintln!("{}\n", e);
            }
            match r {
                Err(e) if e.is_switch() && it.quitting => return 0,
                Err(e) if e.is_switch() => {}
                Err(e) => eprintln!("{}\n", e),
                Ok(()) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_with_names() {
        let p = fits("/hello/{name}", "/hello/Ada%20L").unwrap();
        assert_eq!(p[0].0, "name");
        assert!(matches!(&p[0].1, Value::Str(s) if s == "Ada L"));
        assert!(fits("/hello/{name}", "/hello").is_none());
        assert!(fits("/", "/").is_some());
        assert!(fits("/scores", "/scores/").is_some());
        assert_eq!(decode("a+b%21%E2%9C%93"), "a b!\u{2713}");
    }
}
