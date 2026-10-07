//! Errors and warnings, positioned at a point within a range of the source, or unpositioned.

use crate::source::Offset;

/// dotty's `Message.rewriteNotice`, appended to messages about rewritable syntax.
pub fn rewrite_notice(what: &str, version: &str, options: &str) -> String {
    let source = if version.is_empty() { String::new() } else { format!("-source {version}") };
    let opts = match (source.is_empty(), options.is_empty()) {
        (_, true) => source,
        (true, false) => options.to_string(),
        (false, false) => format!("{source} {options}"),
    };
    format!("\n{what} can be rewritten automatically under -rewrite {opts}.")
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Level {
    Warning,
    Error,
}

/// A message as dotty's `Message` class tells it: text, `errorId` number, `kind` and whether
/// `explain` is non-empty; plain text has no id and no kind.
#[derive(Clone, Debug, Default)]
pub struct Message {
    pub text: String,
    pub code: Option<u16>,
    pub kind: &'static str,
    pub explained: bool,
}

impl Message {
    /// A `SyntaxMsg` with error id `code` and an explanation.
    pub fn syntax(code: u16, text: impl Into<String>) -> Self {
        Message { text: text.into(), code: Some(code), kind: "Syntax", explained: true }
    }

    pub fn without_explanation(self) -> Self {
        Message { explained: false, ..self }
    }
}

impl From<String> for Message {
    fn from(text: String) -> Self {
        Message { text, ..Default::default() }
    }
}

impl From<&str> for Message {
    fn from(text: &str) -> Self {
        text.to_string().into()
    }
}

#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub level: Level,
    pub start: Offset,
    pub end: Offset,
    /// Where the `^` goes; `None` for a diagnostic without a position.
    pub offset: Option<Offset>,
    pub message: String,
    /// dotty's `ErrorMessageID` number, `None` for `NoExplanationID`.
    pub code: Option<u16>,
    /// dotty's `MessageKind` text, empty for `NoKind`.
    pub kind: &'static str,
    /// Whether dotty has a longer explanation of the message.
    pub explained: bool,
    /// Whether the error is about input ending where more was expected.
    pub incomplete: bool,
}

impl Diagnostic {
    /// The offset as the oracle prints it: a number, or `-` without a position.
    pub fn pos(&self) -> String {
        self.offset.map_or("-".into(), |o| o.to_string())
    }
}

#[derive(Default)]
pub struct Diagnostics {
    pub items: Vec<Diagnostic>,
}

impl Diagnostics {
    /// An error with plain text `message` at `offset`.
    pub fn error(&mut self, offset: Offset, message: impl Into<String>) {
        self.error_at(offset, offset, offset, message.into());
    }

    pub fn error_at(&mut self, start: Offset, end: Offset, offset: Offset, message: impl Into<Message>) {
        self.push(Level::Error, start, end, Some(offset), message.into(), false);
    }

    pub fn error_unpositioned(&mut self, message: impl Into<Message>) {
        self.push(Level::Error, 0, 0, None, message.into(), false);
    }

    /// An error at `offset` about input ending where more was expected.
    pub fn incomplete_input_error(&mut self, offset: Offset, message: impl Into<Message>) {
        self.push(Level::Error, offset, offset, Some(offset), message.into(), true);
    }

    pub fn warning(&mut self, offset: Offset, message: impl Into<Message>) {
        self.warning_at(offset, offset, offset, message);
    }

    pub fn warning_at(&mut self, start: Offset, end: Offset, offset: Offset, message: impl Into<Message>) {
        self.push(Level::Warning, start, end, Some(offset), message.into(), false);
    }

    fn push(
        &mut self,
        level: Level,
        start: Offset,
        end: Offset,
        offset: Option<Offset>,
        m: Message,
        incomplete: bool,
    ) {
        let Message { text: message, code, kind, explained } = m;
        self.items.push(Diagnostic { level, start, end, offset, message, code, kind, explained, incomplete });
    }

    pub fn has_errors(&self) -> bool {
        self.items.iter().any(|d| d.level == Level::Error)
    }

    /// Whether the only errors are incomplete-input errors, and there is one (dotty's
    /// `ParseResult.isIncomplete`).
    pub fn is_incomplete(&self) -> bool {
        let errors = || self.items.iter().filter(|d| d.level == Level::Error);
        errors().any(|d| d.incomplete) && errors().all(|d| d.incomplete)
    }

    /// The diagnostics dotty's reporter shows: one per position unless of higher severity
    /// (`UniqueMessagePositions`); all unpositioned ones.
    pub fn unique(&self) -> Vec<&Diagnostic> {
        let mut shown: Vec<&Diagnostic> = Vec::new();
        let mut at: rustc_hash::FxHashMap<Offset, Level> = Default::default();
        for d in &self.items {
            if d.offset.is_some() {
                if (d.start..=d.end).any(|o| at.get(&o).is_some_and(|&l| l >= d.level)) {
                    continue;
                }
                for o in d.start..=d.end {
                    at.insert(o, d.level);
                }
            }
            shown.push(d);
        }
        shown
    }
}
