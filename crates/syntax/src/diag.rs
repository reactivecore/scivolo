//! Errors and warnings, positioned at a byte offset.

use crate::source::Offset;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    Error,
    Warning,
}

#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub level: Level,
    pub offset: Offset,
    pub message: String,
}

#[derive(Default)]
pub struct Diagnostics {
    pub items: Vec<Diagnostic>,
}

impl Diagnostics {
    pub fn error(&mut self, offset: Offset, message: impl Into<String>) {
        self.items.push(Diagnostic { level: Level::Error, offset, message: message.into() });
    }

    pub fn warning(&mut self, offset: Offset, message: impl Into<String>) {
        self.items.push(Diagnostic { level: Level::Warning, offset, message: message.into() });
    }

    pub fn has_errors(&self) -> bool {
        self.items.iter().any(|d| d.level == Level::Error)
    }
}
