mod ast;
#[cfg(feature = "engine")]
mod engine;
mod check;
mod diagnose;
mod error;
mod interp;
mod layout;
mod lexer;
mod methods;
mod json;
mod parser;
mod suggest;
mod tools;
mod two_d;
mod types;
mod physics;
mod value;

#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::io::{BufRead, Write};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const USAGE: &str = "Eza language

usage:
  eza                 start the interactive prompt
  eza <file.eza>      run a script (opens a 3D window if it has a `scene`)
  eza play <file.eza> same as `eza <file.eza>`
  eza run <file.eza>  run a script without ever opening a window
                      (words after the file name reach the script as `args`)
  eza check <file>    find mistakes (typos, wrong kinds of values, wrong argument counts, ...) without running
  eza test [path]     run the `test` blocks in a file or every .eza file in a folder
  eza build <file>    make dist/<name>/ with <name>.exe, to share without installing Eza
  eza explain E003    explain an error code in detail (`eza explain` lists them)
  eza --version";

/// For built programs: errors are also written next to the program, so players can send them in.
static REPORT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Adds an error to the report file of a built program (does nothing for `eza` itself).
pub fn write_report(text: &str) {
    let Some(path) = REPORT.get() else { return };
    let when = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    let entry = format!("---- {} ----\n{}\n\n", when, text);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = f.write_all(entry.as_bytes());
    }
}

/// A program made by `eza build`: `game.exe` runs the `game.eza` sitting next to it.
fn bundled_script() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let script = exe.with_extension("eza");
    script.is_file().then(|| script.display().to_string())
}

/// What a script gets as `args`: everything after the script's name.
fn script_args(args: &[String]) -> Vec<String> {
    match args.first().map(|s| s.as_str()) {
        Some("play" | "run") => args.iter().skip(2).cloned().collect(),
        Some("check" | "test" | "build" | "explain" | "help" | "--help" | "-h" | "--version" | "-v") | None => vec![],
        Some(_) => args.iter().skip(1).cloned().collect(),
    }
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    // a built program: everything typed after its name goes to the script
    let double_clicked = args.is_empty() && bundled_script().is_some();
    if let Some(script) = bundled_script() {
        let _ = REPORT.set(Path::new(&script).with_extension("errors.txt"));
        args.insert(0, script);
    }
    if double_clicked {
        // double-clicked programs close their console at once; keep errors readable
        std::panic::set_hook(Box::new(|p| {
            eprintln!("{}", p);
            pause_before_exit();
        }));
    }
    let finish = move |code: i32| -> ! {
        if double_clicked && code != 0 {
            pause_before_exit();
        }
        std::process::exit(code)
    };
    interp::set_script_args(script_args(&args));
    // scripts with a `scene` open a window, whose event loop must live on the main thread
    #[cfg(feature = "engine")]
    if let Some(f) = script_arg(&args) {
        if has_scene(f) {
            finish(engine::play(f));
        }
    }
    // big stack so deeply nested scripts don't overflow the tree-walking interpreter
    let child = std::thread::Builder::new().stack_size(256 << 20).spawn(move || run(args)).unwrap();
    finish(child.join().unwrap_or(1));
}

fn pause_before_exit() {
    eprintln!("\nPress Enter to close.");
    let _ = std::io::stdin().read_line(&mut String::new());
}

/// Copies a folder, skipping build output and version-control folders.
fn copy_dir(from: &Path, to: &Path, files: &mut u64, bytes: &mut u64) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_s = name.to_string_lossy();
        let p = entry.path();
        if p.is_dir() {
            if ["dist", "target", ".git", "node_modules", ".vscode"].contains(&name_s.as_ref()) {
                continue;
            }
            copy_dir(&p, &to.join(&name), files, bytes)?;
        } else {
            *bytes += std::fs::copy(&p, to.join(&name))?;
            *files += 1;
        }
    }
    Ok(())
}

/// `eza build game.eza`: makes dist/game/ with game.exe, the script and everything in its folder.
fn build_cmd(path: &str) -> i32 {
    let script = Path::new(path);
    if !script.is_file() || script.extension().and_then(|e| e.to_str()) != Some("eza") {
        eprintln!("eza build needs an .eza file, like: eza build game.eza");
        return 2;
    }
    // don't package a program that can't run (warnings are fine)
    let problems: Vec<_> = check::check_file(path).into_iter().filter(|d| !check::is_warning(d)).collect();
    if !problems.is_empty() {
        for d in &problems {
            eprintln!("{}\n", d);
        }
        eprintln!("Fix these first (they're what `eza check` finds), then build again.");
        return 1;
    }
    let dir = match script.parent() {
        Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let name = script.file_stem().and_then(|s| s.to_str()).unwrap_or("app").to_string();
    let out = dir.join("dist").join(&name);
    if out.exists() {
        if !out.join(".eza-build").exists() {
            eprintln!("{} already exists and wasn't made by eza build - move it away first", out.display());
            return 1;
        }
        if let Err(e) = std::fs::remove_dir_all(&out) {
            eprintln!("can't replace the old build in {}: {}", out.display(), e);
            return 1;
        }
    }
    let (mut files, mut bytes) = (0u64, 0u64);
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("can't find the eza program itself: {}", e);
            return 1;
        }
    };
    let result = copy_dir(&dir, &out, &mut files, &mut bytes)
        .and_then(|_| std::fs::copy(&exe, out.join(format!("{}.exe", name))).map(|b| bytes += b))
        .and_then(|_| std::fs::write(out.join(".eza-build"), "made by eza build - safe to delete and rebuild\n"));
    if let Err(e) = result {
        eprintln!("building failed: {}", e);
        return 1;
    }
    println!("Built {}", out.display());
    println!("  {}.exe + {} files, {:.1} MB in total", name, files, bytes as f64 / 1_048_576.0);
    println!("Share the whole folder (zip it). Double-clicking {}.exe runs {}.eza - no Eza install needed.", name, name);
    0
}

fn run(args: Vec<String>) -> i32 {
    match args.first().map(|s| s.as_str()) {
        None => repl(),
        Some("--version" | "-v") => {
            println!("eza {}", env!("CARGO_PKG_VERSION"));
            0
        }
        Some("--help" | "-h" | "help") => {
            println!("{}", USAGE);
            0
        }
        Some("check") => {
            // --plain: one line per problem with exact columns (what the VS Code extension reads)
            // --stdin: check the text piped in as if it were that file (an editor's unsaved changes)
            let plain = args.iter().any(|a| a == "--plain");
            let stdin = args.iter().any(|a| a == "--stdin");
            match args.iter().skip(1).find(|a| !a.starts_with("--")) {
                Some(f) => check(f, plain, stdin),
                None => {
                    eprintln!("{}", USAGE);
                    2
                }
            }
        }
        Some("explain") => diagnose::explain(args.get(1).map(|s| s.as_str())),
        Some("play") => match args.get(1) {
            // scripts with a scene were already sent to the window in main()
            #[cfg(not(feature = "engine"))]
            Some(f) if has_scene(f) => {
                eprintln!("this eza was built without the 3D engine; rebuild with: cargo install --path . --features engine");
                2
            }
            Some(f) => run_file(f),
            None => {
                eprintln!("{}", USAGE);
                2
            }
        },
        Some("build") => match args.get(1) {
            Some(f) => build_cmd(f),
            None => {
                eprintln!("{}", USAGE);
                2
            }
        },
        Some("test") => test_cmd(args.get(1).map(|s| s.as_str()).unwrap_or(".")),
        Some("run") => match args.get(1) {
            Some(f) => run_file(f),
            None => {
                eprintln!("{}", USAGE);
                2
            }
        },
        Some(f) => run_file(f),
    }
}

/// The script `eza <file>` or `eza play <file>` would run, if that's the command.
#[cfg(feature = "engine")]
fn script_arg(args: &[String]) -> Option<&str> {
    match args.first().map(|s| s.as_str())? {
        "play" => args.get(1).map(|s| s.as_str()),
        "run" | "check" | "test" | "build" | "explain" | "help" | "--help" | "-h" | "--version" | "-v" => None,
        f => Some(f),
    }
}

/// Whether the script, or any file it includes, declares a `scene` block. Unreadable or
/// broken scripts count as no scene, so the normal run path reports the error.
fn has_scene(path: &str) -> bool {
    file_has_scene(Path::new(path), &mut HashSet::new())
}

fn file_has_scene(path: &Path, seen: &mut HashSet<PathBuf>) -> bool {
    // same key the interpreter uses, so circular includes stop here too
    if !seen.insert(path.canonicalize().unwrap_or_else(|_| path.to_path_buf())) {
        return false;
    }
    let Ok(src) = std::fs::read_to_string(path) else { return false };
    let Ok(prog) = lexer::lex(&src).and_then(|t| parser::Parser::new(t).program()) else { return false };
    // includes are relative to the including file's folder
    let dir = path.parent().unwrap_or(Path::new(""));
    contains_scene(&prog, dir, seen)
}

fn contains_scene(stmts: &[ast::Stmt], dir: &Path, seen: &mut HashSet<PathBuf>) -> bool {
    use ast::StmtKind as K;
    stmts.iter().any(|s| match &s.kind {
        K::Scene(_) | K::Stage(_) | K::Gui(_) => true,
        K::Include(f) => file_has_scene(&dir.join(f), seen),
        K::Use { path, .. } => {
            let mut p = dir.join(path);
            if p.extension().is_none() {
                p.set_extension("eza");
            }
            file_has_scene(&p, seen)
        }
        K::If(arms, els) => {
            arms.iter().any(|(_, b)| contains_scene(b, dir, seen))
                || els.as_deref().map_or(false, |b| contains_scene(b, dir, seen))
        }
        K::Each(_, _, b) | K::While(_, b) => contains_scene(b, dir, seen),
        K::Attempt(a, _, h) => contains_scene(a, dir, seen) || contains_scene(h, dir, seen),
        _ => false,
    })
}

fn read(path: &str) -> Option<String> {
    match std::fs::read_to_string(path) {
        Ok(s) => Some(s),
        Err(e) => {
            eprintln!("can't open {}: {}", path, e);
            None
        }
    }
}

fn check(path: &str, plain: bool, stdin: bool) -> i32 {
    let diags = if stdin {
        let mut text = String::new();
        let _ = std::io::Read::read_to_string(&mut std::io::stdin(), &mut text);
        diagnose::use_text_for(path, text.clone());
        check::check_text(path, text)
    } else {
        check::check_file(path)
    };
    for d in &diags {
        if plain {
            eprintln!("{}", diagnose::plain(d));
        } else {
            eprintln!("{}\n", d);
        }
    }
    let warnings = diags.iter().filter(|d| check::is_warning(d)).count();
    let errors = diags.len() - warnings;
    let also = if warnings > 0 { format!(" ({} warning(s))", warnings) } else { String::new() };
    if errors == 0 {
        println!("OK{}", also);
        return 0;
    }
    eprintln!("{} problem(s) found{}", errors, also);
    1
}

/// All .eza files under `path` (or just `path` if it's a file), skipping build folders.
fn eza_files(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_file() {
        out.push(path.to_path_buf());
        return;
    }
    let Ok(rd) = std::fs::read_dir(path) else { return };
    let mut entries: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for p in entries {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if p.is_dir() {
            if !name.starts_with('.') && name != "target" && name != "node_modules" {
                eza_files(&p, out);
            }
        } else if name.ends_with(".eza") {
            out.push(p);
        }
    }
}

fn test_cmd(path: &str) -> i32 {
    let mut files = vec![];
    eza_files(Path::new(path), &mut files);
    let (mut passed, mut failed, mut ran) = (0, 0, 0);
    for f in files {
        let name = f.display().to_string();
        let Some(src) = read(&name) else { continue };
        // only files that contain tests are run, so games don't start
        if !src.lines().any(|l| l.trim_start().starts_with("test ")) {
            continue;
        }
        ran += 1;
        println!("{}", name);
        let mut it = interp::Interp::new(&f);
        it.test_mode = true;
        if let Err(e) = it.run_source(&src, &name) {
            eprintln!("  ERROR (outside any test)\n{}", indent(&e.to_string(), 4));
            failed += 1;
        }
        passed += it.tests_passed;
        failed += it.tests_failed;
    }
    if ran == 0 {
        println!("no tests found in {}", path);
        return 0;
    }
    println!("\n{} passed, {} failed", passed, failed);
    if failed > 0 {
        1
    } else {
        0
    }
}

/// Every line pushed right by `n` spaces.
pub fn indent(text: &str, n: usize) -> String {
    let pad = " ".repeat(n);
    text.lines().map(|l| format!("{}{}", pad, l)).collect::<Vec<_>>().join("\n")
}

fn run_file(path: &str) -> i32 {
    if read(path).is_none() {
        return 2;
    }
    match interp::Interp::start(Path::new(path), None, false) {
        Ok(_) => 0,
        Err(e) => {
            let text = e.to_string();
            eprintln!("{}", text);
            write_report(&text);
            1
        }
    }
}

const BLOCK_STARTERS: &[&str] = &[
    "if", "each", "while", "define", "data", "attempt", "on", "persist", "mimic", "scene", "gui",
];

fn repl() -> i32 {
    println!("Eza {} - type 'exit' to quit. Finish a block with an empty line.", env!("CARGO_PKG_VERSION"));
    let mut it = interp::Interp::new(Path::new("./repl.eza"));
    let stdin = std::io::stdin();
    let mut buf = String::new();
    let mut lines = stdin.lock().lines();
    loop {
        print!("{}", if buf.is_empty() { "eza> " } else { "...  " });
        std::io::stdout().flush().ok();
        let Some(Ok(line)) = lines.next() else { break };
        if buf.is_empty() && line.trim() == "exit" {
            break;
        }
        let first_word = line.split_whitespace().next().unwrap_or("");
        let opens_block = BLOCK_STARTERS.contains(&first_word) || line.contains("= define") || line.trim_end().ends_with("then");
        if buf.is_empty() && line.trim().is_empty() {
            continue;
        }
        buf.push_str(&line);
        buf.push('\n');
        let in_block = !buf.lines().next().map_or(true, |l| {
            let w = l.split_whitespace().next().unwrap_or("");
            !(BLOCK_STARTERS.contains(&w) || l.contains("= define") || l.trim_end().ends_with("then"))
        });
        if (in_block || opens_block) && !line.trim().is_empty() {
            continue;
        }
        if let Err(e) = it.run_source(&buf, "<repl>") {
            eprintln!("{}", e);
        }
        buf.clear();
    }
    0
}

