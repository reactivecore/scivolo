// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/NameOps.scala
use crate::names::{NameRef, Names};

/// A name to pickle, its parts held by value.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Name {
    Utf8(String),
    Qualified(Box<Name>, Box<Name>),
    Expanded(Box<Name>, Box<Name>),
    ExpandPrefix(Box<Name>, Box<Name>),
    Unique { under: Option<Box<Name>>, sep: String, num: u32 },
    DefaultGetter(Box<Name>, u32),
    SuperAccessor(Box<Name>),
    InlineAccessor(Box<Name>),
    BodyRetainer(Box<Name>),
    ObjectClass(Box<Name>),
    Signed { orig: Box<Name>, target: Box<Name>, result: Box<Name>, params: Vec<ParamSig> },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ParamSig {
    TypeParams(u32),
    Term(Name),
}

impl Name {
    pub fn utf8(s: &str) -> Name {
        Name::Utf8(s.to_owned())
    }

    /// Name `r` of a decoded name table.
    pub fn read(names: &Names, r: NameRef) -> Option<Name> {
        use crate::names::{Name as N, ParamSig as P};
        let at = |r| Name::read(names, r).map(Box::new);
        Some(match names.get(r)? {
            N::Utf8(s) => Name::utf8(s),
            N::Qualified(a, b) => Name::Qualified(at(*a)?, at(*b)?),
            N::Expanded(a, b) => Name::Expanded(at(*a)?, at(*b)?),
            N::ExpandPrefix(a, b) => Name::ExpandPrefix(at(*a)?, at(*b)?),
            N::Unique { sep, num, under } => {
                let under = match under {
                    Some(u) => Some(at(*u)?),
                    None => None,
                };
                Name::Unique { under, sep: names.show(*sep), num: *num }
            }
            N::DefaultGetter(u, i) => Name::DefaultGetter(at(*u)?, *i),
            N::SuperAccessor(u) => Name::SuperAccessor(at(*u)?),
            N::InlineAccessor(u) => Name::InlineAccessor(at(*u)?),
            N::BodyRetainer(u) => Name::BodyRetainer(at(*u)?),
            N::ObjectClass(u) => Name::ObjectClass(at(*u)?),
            N::Signed { orig, target, result, params } => Name::Signed {
                orig: at(*orig)?,
                target: at(target.unwrap_or(*orig))?,
                result: at(*result)?,
                params: params
                    .iter()
                    .map(|p| match p {
                        P::TypeParams(n) => Some(ParamSig::TypeParams(*n)),
                        P::Term(t) => Name::read(names, *t).map(ParamSig::Term),
                    })
                    .collect::<Option<_>>()?,
            },
        })
    }

    pub fn is_empty(&self) -> bool {
        matches!(self, Name::Utf8(s) if s.is_empty())
    }

    /// Whether a signed name with this original name needs no separate `target`.
    pub fn matches_target(&self, target: &Name) -> bool {
        self == target || self.is_empty() || target.is_empty()
    }
}
