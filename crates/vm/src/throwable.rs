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
    /// The names of its class and of that class's superclasses, innermost first.
    pub classes: Vec<String>,
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
        let mut seen = self.seen(exc);
        let thrown = self.thrown_after(exc, &mut seen);
        std::hint::black_box(seen.blob);
        thrown
    }

    /// The set of throwables holding only `exc`.
    fn seen(&mut self, exc: usize) -> Seen {
        let mut seen = Seen { blob: self.blob(8 * MAX_THROWABLES).unwrap_or(0), len: 0 };
        seen.insert(exc);
        seen
    }

    /// Throwable `exc`, without the causes and suppressed exceptions in `seen`, which it extends.
    fn thrown_after(&mut self, exc: usize, seen: &mut Seen) -> Thrown {
        let description = self.describe_throwable(exc);
        let classes = std::iter::successors(Some(self.class_of(exc)), |&c| self.class(c).parent)
            .map(|c| self.class_name(c).to_owned())
            .collect();
        let mut chain = self.seen(exc);
        let header = self.header(exc, &mut chain);
        std::hint::black_box(chain.blob);
        let frames = self.stack_trace(exc).unwrap_or_default();
        let cause = self.call_getter(exc, "getCause", THROWABLE).unwrap_or(0) as usize;
        let cause = seen.insert(cause).then(|| Box::new(self.thrown_after(cause, seen)));
        let suppressed = self.suppressed(exc, seen);
        std::hint::black_box(exc);
        Thrown { description, classes, header, frames, cause, suppressed }
    }

    fn suppressed(&mut self, exc: usize, seen: &mut Seen) -> Vec<Thrown> {
        let arr = self.call_getter(exc, "getSuppressed", "LAL19java.lang.Throwable_").unwrap_or(0) as usize;
        if arr == 0 {
            return Vec::new();
        }
        let len: i32 = heap::read(arr + ARRAY_LENGTH);
        let mut out = Vec::new();
        for i in 0..len as usize {
            let s = heap::read(arr + ARRAY_HEADER as usize + 8 * i);
            if seen.insert(s) {
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
    /// whose header is being built, `exc` included.
    fn header(&mut self, exc: usize, seen: &mut Seen) -> String {
        let name = self.runtime_class_name(exc);
        let text = match self.call_getter(exc, "getMessage", STRING).unwrap_or(0) as usize {
            0 => match self.call_getter(exc, "getCause", THROWABLE).unwrap_or(0) as usize {
                cause if seen.insert(cause) => Some(self.header(cause, seen)),
                _ => None,
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

/// A set of at most `MAX_THROWABLES` throwables, kept in a blob the collector scans; full if the
/// blob could not be allocated.
struct Seen {
    blob: usize,
    len: usize,
}

impl Seen {
    /// Adds `exc` unless it is null, already in the set or the set is full; whether it did.
    fn insert(&mut self, exc: usize) -> bool {
        let full = self.blob == 0 || self.len == MAX_THROWABLES;
        if exc == 0 || full || (0..self.len).any(|i| heap::read::<usize>(self.blob + 8 * i) == exc) {
            return false;
        }
        heap::write(self.blob + 8 * self.len, exc);
        self.len += 1;
        true
    }
}
