// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/NameBuffer.scala
use super::buffer::{Addr, Buffer};
use super::name::{Name, ParamSig};
use crate::names::{self as tags, NameRef};
use rustc_hash::FxHashMap;

/// The name table, in the order names are first registered.
#[derive(Debug, Default)]
pub struct NameTable {
    names: Vec<Name>,
    refs: FxHashMap<Name, NameRef>,
}

impl NameTable {
    /// The index of `name`, registering its parts first when it is new.
    pub fn index(&mut self, name: &Name) -> NameRef {
        if let Some(&r) = self.refs.get(name) {
            return r;
        }
        match name {
            Name::Signed { orig, target, result, params } => {
                self.index(orig);
                if !orig.matches_target(target) {
                    self.index(target);
                }
                self.index(result);
                for p in params {
                    if let ParamSig::Term(t) = p {
                        self.index(t);
                    }
                }
            }
            Name::Qualified(a, b) | Name::Expanded(a, b) | Name::ExpandPrefix(a, b) => {
                self.index(a);
                self.index(b);
            }
            Name::Unique { under, sep, .. } => {
                self.index(&Name::utf8(sep));
                if let Some(u) = under.as_deref().filter(|u| !u.is_empty()) {
                    self.index(u);
                }
            }
            Name::DefaultGetter(u, _)
            | Name::SuperAccessor(u)
            | Name::InlineAccessor(u)
            | Name::BodyRetainer(u)
            | Name::ObjectClass(u) => {
                self.index(u);
            }
            Name::Utf8(_) => {}
        }
        let r = NameRef(self.names.len() as u32);
        self.names.push(name.clone());
        self.refs.insert(name.clone(), r);
        r
    }

    pub fn utf8(&mut self, s: &str) -> NameRef {
        self.index(&Name::utf8(s))
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// The index of a registered name.
    pub fn get(&self, name: &Name) -> Option<NameRef> {
        self.refs.get(name).copied()
    }

    /// The encoded table.
    pub fn assemble(&self) -> Buffer {
        let mut buf = Buffer::default();
        for name in &self.names {
            self.pickle(&mut buf, name);
        }
        buf
    }

    fn write_ref(&self, buf: &mut Buffer, name: &Name) {
        buf.write_nat(self.refs[name].0 as u64);
    }

    fn pickle(&self, buf: &mut Buffer, name: &Name) {
        let (tag, width) = match name {
            Name::Utf8(s) => {
                buf.write_byte(tags::UTF8);
                buf.write_utf8(s);
                return;
            }
            Name::Signed { orig, target, params, .. } => {
                let tag = if orig.matches_target(target) { tags::SIGNED } else { tags::TARGETSIGNED };
                (tag, if (params.len() + 3) * 3 <= 127 { 1 } else { 2 })
            }
            Name::Qualified(..) => (tags::QUALIFIED, 1),
            Name::Expanded(..) => (tags::EXPANDED, 1),
            Name::ExpandPrefix(..) => (tags::EXPANDPREFIX, 1),
            Name::Unique { .. } => (tags::UNIQUE, 1),
            Name::DefaultGetter(..) => (tags::DEFAULTGETTER, 1),
            Name::SuperAccessor(_) => (tags::SUPERACCESSOR, 1),
            Name::InlineAccessor(_) => (tags::INLINEACCESSOR, 1),
            Name::BodyRetainer(_) => (tags::BODYRETAINER, 1),
            Name::ObjectClass(_) => (tags::OBJECTCLASS, 1),
        };
        buf.write_byte(tag);
        let at = buf.current_addr();
        buf.write_bytes(&[0; 2][..width]);
        self.pickle_parts(buf, name);
        let len = buf.len() - at.0 - width;
        buf.put_nat(Addr(at.0), len, width);
    }

    fn pickle_parts(&self, buf: &mut Buffer, name: &Name) {
        match name {
            Name::Qualified(a, b) | Name::Expanded(a, b) | Name::ExpandPrefix(a, b) => {
                self.write_ref(buf, a);
                self.write_ref(buf, b);
            }
            Name::Unique { under, sep, num } => {
                self.write_ref(buf, &Name::utf8(sep));
                buf.write_nat(*num as u64);
                if let Some(u) = under.as_deref().filter(|u| !u.is_empty()) {
                    self.write_ref(buf, u);
                }
            }
            Name::DefaultGetter(u, num) => {
                self.write_ref(buf, u);
                buf.write_nat(*num as u64);
            }
            Name::SuperAccessor(u)
            | Name::InlineAccessor(u)
            | Name::BodyRetainer(u)
            | Name::ObjectClass(u) => self.write_ref(buf, u),
            Name::Signed { orig, target, result, params } => {
                self.write_ref(buf, orig);
                if !orig.matches_target(target) {
                    self.write_ref(buf, target);
                }
                self.write_ref(buf, result);
                for p in params {
                    match p {
                        ParamSig::TypeParams(n) => buf.write_long_int(-(*n as i64)),
                        ParamSig::Term(t) => buf.write_long_int(self.refs[t].0 as i64),
                    }
                }
            }
            Name::Utf8(_) => {}
        }
    }
}
