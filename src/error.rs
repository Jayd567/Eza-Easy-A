use crate::ast::Expr;
use std::fmt;
use std::ops::{Deref, DerefMut};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ErrKind {
    Syntax,
    Runtime,
    /// not a real error: `go to` unwinding the current script
    Switch,
    /// found by `eza check` before running
    Check,
    /// `eza check`: probably a mistake, but the program can still run
    Warning,
}

/// What a label underlines on its line.
#[derive(Debug, Clone)]
pub enum Target {
    /// a piece of code; it's found in the line by its tokens
    Expr(Expr),
    /// a name, like a variable or function
    Name(String),
    /// exact columns (0-based, end exclusive), from the lexer
    Cols(usize, usize),
}

/// A spot in the code to point at, with a short message under it.
#[derive(Debug, Clone)]
pub struct Label {
    pub line: usize,
    /// empty = the error's own file
    pub file: String,
    pub at: Target,
    pub msg: String,
    /// the main spot (^^^) rather than extra information (---)
    pub primary: bool,
}

#[derive(Debug, Clone)]
pub struct ErrInfo {
    pub kind: ErrKind,
    pub msg: String,
    pub line: usize,
    pub file: String,
    /// (function name, line it was called from), innermost first
    pub trace: Vec<(String, usize)>,
    pub labels: Vec<Label>,
    /// explanation lines: the values involved and how they got that way
    pub notes: Vec<String>,
    /// suggested fixes
    pub help: Vec<String>,
    /// where in a running game it happened (frame, `on` block)
    pub context: Vec<String>,
    /// set once the error has been explained, so outer code doesn't explain it again
    pub explained: bool,
}

/// Boxed, so a successful result (by far the common case) stays small and cheap to pass around.
#[derive(Debug, Clone)]
pub struct EzaError(Box<ErrInfo>);

impl Deref for EzaError {
    type Target = ErrInfo;
    fn deref(&self) -> &ErrInfo {
        &self.0
    }
}

impl DerefMut for EzaError {
    fn deref_mut(&mut self) -> &mut ErrInfo {
        &mut self.0
    }
}

impl EzaError {
    fn new(kind: ErrKind, line: usize, msg: String) -> Self {
        EzaError(Box::new(ErrInfo {
            kind,
            msg,
            line,
            file: String::new(),
            trace: vec![],
            labels: vec![],
            notes: vec![],
            help: vec![],
            context: vec![],
            explained: false,
        }))
    }
    pub fn syntax(line: usize, msg: impl Into<String>) -> Self {
        Self::new(ErrKind::Syntax, line, msg.into())
    }
    pub fn runtime(line: usize, msg: impl Into<String>) -> Self {
        Self::new(ErrKind::Runtime, line, msg.into())
    }
    pub fn check(line: usize, msg: impl Into<String>, warning: bool) -> Self {
        Self::new(if warning { ErrKind::Warning } else { ErrKind::Check }, line, msg.into())
    }
    pub fn switch(line: usize) -> Self {
        Self::new(ErrKind::Switch, line, String::new())
    }
    pub fn is_switch(&self) -> bool {
        self.kind == ErrKind::Switch
    }
    pub fn in_file(mut self, file: &str) -> Self {
        if self.file.is_empty() {
            self.file = file.to_string();
        }
        self
    }
    /// Points at a spot on this error's own line.
    pub fn label(&mut self, at: Target, msg: impl Into<String>, primary: bool) {
        let line = self.line;
        self.labels.push(Label { line, file: String::new(), at, msg: msg.into(), primary });
    }
    /// The first line of the message: `[Runtime Error] game.eza:12: can't divide by zero (E003)`.
    pub fn headline(&self) -> String {
        let k = match self.kind {
            ErrKind::Syntax => "Syntax Error",
            ErrKind::Runtime => "Runtime Error",
            ErrKind::Switch => "Switch",
            ErrKind::Check => "Check Error",
            ErrKind::Warning => "Check Warning",
        };
        let code = crate::diagnose::code_of(self);
        let code = if code.is_empty() { String::new() } else { format!("  ({})", code) };
        if self.file.is_empty() {
            format!("[{}] line {}: {}{}", k, self.line, self.msg, code)
        } else {
            format!("[{}] {}:{}: {}{}", k, self.file, self.line, self.msg, code)
        }
    }
}

/// The full picture: the headline, the code with the problem underlined, the values involved,
/// how they got that way, a suggested fix, and the chain of function calls.
impl fmt::Display for EzaError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&crate::diagnose::render(self))
    }
}

pub type R<T> = Result<T, EzaError>;
