//! What an uncaught throwable reports: its `toString()` and its stack trace.

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

/// An uncaught throwable: its `toString()` and its stack trace, innermost frame first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thrown {
    pub description: String,
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
        let frames = self.stack_trace(exc).unwrap_or_default();
        Thrown { description, frames }
    }

    /// `toString()` of throwable `exc`, or its class name if that fails too.
    pub fn describe_throwable(&mut self, exc: usize) -> String {
        let name = self.class_name(self.class_of(exc)).to_owned();
        let text = self.call_getter(exc, "toString", STRING).and_then(|s| self.read_string(s as usize).ok());
        text.unwrap_or(name)
    }

    fn stack_trace(&mut self, exc: usize) -> Option<Vec<Frame>> {
        let arr = self.call_getter(exc, "getStackTrace", "LAL27java.lang.StackTraceElement_")? as usize;
        if arr == 0 {
            return None;
        }
        let len: i32 = heap::read(arr + ARRAY_LENGTH);
        (0..len as usize).map(|i| self.frame(heap::read(arr + ARRAY_HEADER as usize + 8 * i))).collect()
    }

    fn frame(&mut self, elem: usize) -> Option<Frame> {
        if elem == 0 {
            return None;
        }
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

    /// What the public no-argument method `name` of `obj`, returning mangled type `ret`, returns.
    fn call_getter(&mut self, obj: usize, name: &str, ret: &str) -> Option<u64> {
        let mut sig = String::from("D");
        nir::sig::ident(&mut sig, name);
        sig.push_str(ret);
        sig.push_str("EO");
        let sig = nir::StrId(self.world.pool.strings.intern(sig.into()));
        let f = self.virtual_method(obj, sig).ok()?;
        self.call(f, &[obj as u64]).ok()
    }
}
