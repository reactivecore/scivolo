// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TastyUnpickler.scala
//   tasty/src/dotty/tools/tasty/TastyFormat.scala
use crate::cursor::{Cursor, Result};
use std::fmt::Write;

/// 0-based index into the name table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NameRef(pub u32);

#[derive(Debug, Clone, Copy)]
pub enum ParamSig {
    TypeParams(u32),
    Term(NameRef),
}

#[derive(Debug, Clone)]
pub enum Name<'a> {
    Utf8(&'a str),
    Qualified(NameRef, NameRef),
    Expanded(NameRef, NameRef),
    ExpandPrefix(NameRef, NameRef),
    Unique { sep: NameRef, num: u32, under: Option<NameRef> },
    DefaultGetter(NameRef, u32),
    SuperAccessor(NameRef),
    InlineAccessor(NameRef),
    BodyRetainer(NameRef),
    ObjectClass(NameRef),
    Signed { orig: NameRef, target: Option<NameRef>, result: NameRef, params: Vec<ParamSig> },
}

const UTF8: u8 = 1;
const QUALIFIED: u8 = 2;
const EXPANDED: u8 = 3;
const EXPANDPREFIX: u8 = 4;
const UNIQUE: u8 = 10;
const DEFAULTGETTER: u8 = 11;
const SUPERACCESSOR: u8 = 20;
const INLINEACCESSOR: u8 = 21;
const BODYRETAINER: u8 = 22;
const OBJECTCLASS: u8 = 23;
const TARGETSIGNED: u8 = 62;
const SIGNED: u8 = 63;

pub struct Names<'a> {
    entries: Vec<Name<'a>>,
}

impl<'a> Names<'a> {
    pub fn parse(cur: &mut Cursor<'a>, end: usize) -> Result<Self> {
        let mut entries = Vec::new();
        while cur.pos() < end {
            let name = read_name(cur, entries.len() as u32)?;
            entries.push(name);
        }
        if cur.pos() != end {
            return cur.err("name table overrun");
        }
        Ok(Names { entries })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, r: NameRef) -> Option<&Name<'a>> {
        self.entries.get(r.0 as usize)
    }

    pub fn iter(&self) -> impl Iterator<Item = (NameRef, &Name<'a>)> {
        self.entries.iter().enumerate().map(|(i, n)| (NameRef(i as u32), n))
    }

    /// Source-level spelling; signed names drop their signature.
    pub fn show(&self, r: NameRef) -> String {
        let mut s = String::new();
        self.write(&mut s, r, false);
        s
    }

    /// Like `show` but keeps signatures as `name(p1,p2)result`.
    pub fn show_signed(&self, r: NameRef) -> String {
        let mut s = String::new();
        self.write(&mut s, r, true);
        s
    }

    fn write(&self, s: &mut String, r: NameRef, sig: bool) {
        let Some(n) = self.get(r) else {
            let _ = write!(s, "<bad name {}>", r.0);
            return;
        };
        let pair = |s: &mut String, a, sep: &str, b| {
            self.write(s, a, sig);
            s.push_str(sep);
            self.write(s, b, sig);
        };
        match n {
            Name::Utf8(t) => s.push_str(t),
            Name::Qualified(a, b) => pair(s, *a, ".", *b),
            Name::Expanded(a, b) => pair(s, *a, "$$", *b),
            Name::ExpandPrefix(a, b) => pair(s, *a, "$", *b),
            Name::Unique { sep, num, under } => {
                if let Some(u) = under {
                    self.write(s, *u, sig);
                }
                self.write(s, *sep, sig);
                let _ = write!(s, "{num}");
            }
            Name::DefaultGetter(u, i) => {
                self.write(s, *u, sig);
                let _ = write!(s, "$default${}", i + 1);
            }
            Name::SuperAccessor(u) => self.write_prefixed(s, "super$", *u, sig),
            Name::InlineAccessor(u) => self.write_prefixed(s, "inline$", *u, sig),
            Name::BodyRetainer(u) => {
                self.write(s, *u, sig);
                s.push_str("$retainedBody");
            }
            Name::ObjectClass(u) => {
                self.write(s, *u, sig);
                s.push('$');
            }
            Name::Signed { orig, target, result, params } => {
                self.write(s, *orig, sig);
                if let Some(t) = target.filter(|t| t != orig) {
                    s.push_str("@targetName(");
                    self.write(s, t, sig);
                    s.push(')');
                }
                if sig {
                    s.push('(');
                    for (i, p) in params.iter().enumerate() {
                        if i > 0 {
                            s.push(',');
                        }
                        match p {
                            ParamSig::TypeParams(n) => {
                                let _ = write!(s, "[{n}]");
                            }
                            ParamSig::Term(t) => self.write(s, *t, sig),
                        }
                    }
                    s.push(')');
                    self.write(s, *result, sig);
                }
            }
        }
    }

    /// `prefix` before the last segment of `u`: `p$C$$inline$f` for `inline$` and `p$C$$f`
    /// (dotty's `qualToString`).
    fn write_prefixed(&self, s: &mut String, prefix: &str, u: NameRef, sig: bool) {
        let split = match self.get(u) {
            Some(Name::Qualified(a, b)) => Some((*a, ".", *b)),
            Some(Name::Expanded(a, b)) => Some((*a, "$$", *b)),
            Some(Name::ExpandPrefix(a, b)) => Some((*a, "$", *b)),
            _ => None,
        };
        let last = match split {
            Some((first, sep, last)) => {
                self.write(s, first, sig);
                s.push_str(sep);
                last
            }
            None => u,
        };
        s.push_str(prefix);
        self.write(s, last, sig);
    }
}

/// The name at the cursor, whose references must be to the `next` names before it.
fn read_name<'a>(cur: &mut Cursor<'a>, next: u32) -> Result<Name<'a>> {
    let tag = cur.byte()?;
    if tag == UTF8 {
        return Ok(Name::Utf8(cur.utf8()?));
    }
    let end = cur.end()?;
    let r = |cur: &mut Cursor<'a>| match cur.nat32()? {
        n if n < next => Ok(NameRef(n)),
        n => cur.err(format!("name {n} referenced before its definition")),
    };
    let name = match tag {
        QUALIFIED => Name::Qualified(r(cur)?, r(cur)?),
        EXPANDED => Name::Expanded(r(cur)?, r(cur)?),
        EXPANDPREFIX => Name::ExpandPrefix(r(cur)?, r(cur)?),
        UNIQUE => {
            let sep = r(cur)?;
            let num = cur.nat32()?;
            let under = if cur.pos() < end { Some(r(cur)?) } else { None };
            Name::Unique { sep, num, under }
        }
        DEFAULTGETTER => Name::DefaultGetter(r(cur)?, cur.nat32()?),
        SUPERACCESSOR => Name::SuperAccessor(r(cur)?),
        INLINEACCESSOR => Name::InlineAccessor(r(cur)?),
        BODYRETAINER => Name::BodyRetainer(r(cur)?),
        OBJECTCLASS => Name::ObjectClass(r(cur)?),
        SIGNED | TARGETSIGNED => {
            let orig = r(cur)?;
            let target = if tag == TARGETSIGNED { Some(r(cur)?) } else { None };
            let result = r(cur)?;
            let mut params = Vec::new();
            while cur.pos() < end {
                let v = cur.long_int()?;
                params.push(match u32::try_from(v) {
                    Ok(n) if n < next => ParamSig::Term(NameRef(n)),
                    Ok(n) => return cur.err(format!("name {n} referenced before its definition")),
                    Err(_) => ParamSig::TypeParams(v.unsigned_abs() as u32),
                });
            }
            Name::Signed { orig, target, result, params }
        }
        _ => return cur.err(format!("unknown name tag {tag}")),
    };
    if cur.pos() != end {
        return cur.err(format!("name tag {tag} length mismatch"));
    }
    Ok(name)
}
