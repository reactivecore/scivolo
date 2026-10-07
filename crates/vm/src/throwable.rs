//! What an uncaught throwable reports: its `toString()`, the header dotty's REPL prints for it and
//! its stack trace.

use crate::layout::{ARRAY_HEADER, ARRAY_LENGTH};
use crate::{Vm, heap};
use std::fmt;

const STRING: &str = "L16java.lang.String";
const THROWABLE: &str = "L19java.lang.Throwable";

/// A stack trace element: method `method` of class `class`, at `line` of `file`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub class: String,
    pub method: String,
    pub file: Option<String>,
    pub line: i32,
}

/// How many throwables a `Thrown` holds at most, its causes and suppressed ones included.
const MAX_THROWABLES: usize = 64;

/// An uncaught throwable: its `toString()`, its header, its stack trace, innermost frame first,
/// its cause and the exceptions it suppressed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thrown {
    pub description: String,
    /// The runtime class name.
    pub class: String,
    /// The class name, then `: ` and the message or else the cause's header, if there is one.
    pub header: String,
    pub frames: Vec<Frame>,
    /// The cause, unless it is a throwable met before in the order dotty's `StackTraceOps`
    /// prints them: a throwable, its cause, then its suppressed ones.
    pub cause: Option<Box<Thrown>>,
    /// The suppressed exceptions but those met before.
    pub suppressed: Vec<Thrown>,
}

impl fmt::Display for Thrown {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.description)
    }
}

impl Vm {
    /// Throwable `exc`; no frames if its stack trace cannot be read.
    pub fn thrown(&mut self, exc: usize) -> Thrown {
        self.thrown_after(exc, &mut vec![exc])
    }

    /// Throwable `exc`, without the causes and suppressed exceptions in `seen`, which it extends.
    fn thrown_after(&mut self, exc: usize, seen: &mut Vec<usize>) -> Thrown {
        let description = self.describe_throwable(exc);
        let class = self.runtime_class_name(exc);
        let header = self.header(exc, &mut Vec::new());
        let frames = self.stack_trace(exc).unwrap_or_default();
        let cause = self.call_getter(exc, "getCause", THROWABLE).unwrap_or(0) as usize;
        let cause = unseen(cause, seen).map(|c| Box::new(self.thrown_after(c, seen)));
        let suppressed = self.suppressed(exc, seen);
        std::hint::black_box(exc);
        Thrown { description, class, header, frames, cause, suppressed }
    }

    fn suppressed(&mut self, exc: usize, seen: &mut Vec<usize>) -> Vec<Thrown> {
        let arr = self.call_getter(exc, "getSuppressed", "LAL19java.lang.Throwable_").unwrap_or(0) as usize;
        if arr == 0 {
            return Vec::new();
        }
        let len: i32 = heap::read(arr + ARRAY_LENGTH);
        let mut out = Vec::new();
        for i in 0..len as usize {
            if let Some(s) = unseen(heap::read(arr + ARRAY_HEADER as usize + 8 * i), seen) {
                out.push(self.thrown_after(s, seen));
            }
        }
        std::hint::black_box(arr);
        out
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
        let name = self.runtime_class_name(exc);
        let text = match self.call_getter(exc, "getMessage", STRING).unwrap_or(0) as usize {
            0 => match self.call_getter(exc, "getCause", THROWABLE).unwrap_or(0) as usize {
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

    /// `getClass.getName` of `exc`, or else the name of its class in the VM.
    fn runtime_class_name(&mut self, exc: usize) -> String {
        let class = self.call_getter(exc, "getClass", "L15java.lang.Class").unwrap_or(0) as usize;
        match self.string_getter(class, "getName") {
            Some(name) => name,
            None => self.class_name(self.class_of(exc)).to_owned(),
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

/// `exc`, now in `seen`, unless it is null, already in `seen` or `seen` is full.
fn unseen(exc: usize, seen: &mut Vec<usize>) -> Option<usize> {
    if exc == 0 || seen.contains(&exc) || seen.len() >= MAX_THROWABLES {
        return None;
    }
    seen.push(exc);
    Some(exc)
}
