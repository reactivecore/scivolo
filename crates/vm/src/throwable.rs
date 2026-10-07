//! What an uncaught throwable reports: its `toString()`, the header dotty's REPL prints for it and
//! its stack trace.

use crate::layout::{ARRAY_HEADER, ARRAY_LENGTH};
use crate::{Vm, heap};
use std::fmt;

const STRING: &str = "L16java.lang.String";

/// A stack trace element: method `method` of class `class`, at `line` of `file`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub class: String,
    pub method: String,
    pub file: Option<String>,
    pub line: i32,
}

/// An uncaught throwable: its `toString()`, its header and its stack trace, innermost frame first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thrown {
    pub description: String,
    /// The class name, then `: ` and the message or else the cause's header, if there is one.
    pub header: String,
    pub frames: Vec<Frame>,
}

impl fmt::Display for Thrown {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.description)
    }
}

impl Vm {
    /// Throwable `exc`; no frames if its stack trace cannot be read.
    pub fn thrown(&mut self, exc: usize) -> Thrown {
        let description = self.describe_throwable(exc);
        let header = self.header(exc, &mut Vec::new());
        let frames = self.stack_trace(exc).unwrap_or_default();
        Thrown { description, header, frames }
    }

    /// `toString()` of throwable `exc`, or its class name if that fails too.
    pub fn describe_throwable(&mut self, exc: usize) -> String {
        let name = self.class_name(self.class_of(exc)).to_owned();
        self.string_getter(exc, "toString").unwrap_or(name)
    }

    /// The header of `exc` as dotty's `StackTraceOps` prints it; `seen` holds the throwables
    /// whose header is being built.
    fn header(&mut self, exc: usize, seen: &mut Vec<usize>) -> String {
        seen.push(exc);
        let class = self.call_getter(exc, "getClass", "L15java.lang.Class").unwrap_or(0) as usize;
        let name = match self.string_getter(class, "getName") {
            Some(name) => name,
            None => self.class_name(self.class_of(exc)).to_owned(),
        };
        let text = match self.call_getter(exc, "getMessage", STRING).unwrap_or(0) as usize {
            0 => match self.call_getter(exc, "getCause", "L19java.lang.Throwable").unwrap_or(0) as usize {
                0 => None,
                cause if seen.contains(&cause) => None,
                cause => Some(self.header(cause, seen)),
            },
            msg => self.read_string(msg).ok(),
        };
        match text {
            Some(text) => format!("{name}: {text}"),
            None => name,
        }
    }

    fn stack_trace(&mut self, exc: usize) -> Option<Vec<Frame>> {
        let arr = self.call_getter(exc, "getStackTrace", "LAL27java.lang.StackTraceElement_")? as usize;
        if arr == 0 {
            return None;
        }
        let len: i32 = heap::read(arr + ARRAY_LENGTH);
        let elem = |i: usize| heap::read(arr + ARRAY_HEADER as usize + 8 * i);
        Some((0..len as usize).filter_map(|i| self.frame(elem(i))).collect())
    }

    fn frame(&mut self, elem: usize) -> Option<Frame> {
        let class = self.string_getter(elem, "getClassName")?;
        let method = self.string_getter(elem, "getMethodName")?;
        let file = match self.call_getter(elem, "getFileName", STRING)? {
            0 => None,
            s => Some(self.read_string(s as usize).ok()?),
        };
        let line = self.call_getter(elem, "getLineNumber", "i")? as i32;
        Some(Frame { class, method, file, line })
    }

    fn string_getter(&mut self, obj: usize, name: &str) -> Option<String> {
        let s = self.call_getter(obj, name, STRING)?;
        self.read_string(s as usize).ok()
    }

    /// What the public no-argument method `name` of `obj`, returning mangled type `ret`, returns;
    /// `None` if `obj` is null or the call fails.
    fn call_getter(&mut self, obj: usize, name: &str, ret: &str) -> Option<u64> {
        if obj == 0 {
            return None;
        }
        let mut sig = String::from("D");
        nir::sig::ident(&mut sig, name);
        sig.push_str(ret);
        sig.push_str("EO");
        let sig = nir::StrId(self.world.pool.strings.intern(sig.into()));
        let f = self.virtual_method(obj, sig).ok()?;
        self.call(f, &[obj as u64]).ok()
    }
}
