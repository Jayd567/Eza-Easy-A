use std::fmt;
use std::ops::{Deref, DerefMut};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ErrKind {
    Syntax,
    Runtime,
    /// not a real error: `go to` unwinding the current script
    Switch,
}

#[derive(Debug, Clone)]
pub struct ErrInfo {
    pub kind: ErrKind,
    pub msg: String,
    pub line: usize,
    pub file: String,
    /// (function name, line it was called from), innermost first
    pub trace: Vec<(String, usize)>,
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
        EzaError(Box::new(ErrInfo { kind, msg, line, file: String::new(), trace: vec![] }))
    }
    pub fn syntax(line: usize, msg: impl Into<String>) -> Self {
        Self::new(ErrKind::Syntax, line, msg.into())
    }
    pub fn runtime(line: usize, msg: impl Into<String>) -> Self {
        Self::new(ErrKind::Runtime, line, msg.into())
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
}

impl fmt::Display for EzaError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let k = match self.kind {
            ErrKind::Syntax => "Syntax Error",
            ErrKind::Runtime => "Runtime Error",
            ErrKind::Switch => "Switch",
        };
        if self.file.is_empty() {
            write!(f, "[{}] line {}: {}", k, self.line, self.msg)?;
        } else {
            write!(f, "[{}] {}:{}: {}", k, self.file, self.line, self.msg)?;
        }
        for (name, line) in self.trace.iter().take(6) {
            write!(f, "\n  in {} (called from line {})", name, line)?;
        }
        if self.trace.len() > 6 {
            write!(f, "\n  ... and {}+ more calls", self.trace.len() - 6)?;
        }
        Ok(())
    }
}

pub type R<T> = Result<T, EzaError>;
