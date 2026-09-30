//! What a symbol is: its kind, flags, owner and declarations.

use crate::NodeId;
use crate::tags::{PRIVATEqualified, PROTECTEDqualified};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SymbolId(pub u32);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct FileId(pub u32);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Package,
    /// A class, trait or module class, with its members.
    Class,
    /// A type alias, abstract type or opaque type.
    Type,
    /// A type parameter of a class or method.
    TypeParam,
    /// A value parameter of a class or method.
    Param,
    Val,
    Def,
}

impl Kind {
    /// Whether a symbol of this kind is a term: a value, parameter or method.
    pub fn is_term(self) -> bool {
        matches!(self, Kind::Val | Kind::Param | Kind::Def)
    }
}

/// Modifiers: one bit per TASTy modifier tag below 64, and bits 0 and 1 (no modifier's tag) for
/// `PRIVATEqualified` and `PROTECTEDqualified`.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Flags(pub u64);

impl Flags {
    /// The bit of modifier tag `tag`, if it has one.
    fn bit(tag: u8) -> Option<u32> {
        match tag {
            PRIVATEqualified => Some(0),
            PROTECTEDqualified => Some(1),
            2..64 => Some(tag as u32),
            _ => None,
        }
    }

    pub fn with(self, tag: u8) -> Flags {
        Flags(self.0 | Self::bit(tag).map_or(0, |b| 1 << b))
    }

    pub fn has(self, tag: u8) -> bool {
        Self::bit(tag).is_some_and(|b| self.0 & 1 << b != 0)
    }
}

pub struct Symbol {
    pub name: String,
    pub kind: Kind,
    pub owner: Option<SymbolId>,
    pub flags: Flags,
    /// The name a `@targetName` annotation gives it at run time.
    pub target: Option<String>,
    /// Whether Scala Native's `@extern` annotates it: its members are C functions and values.
    pub is_extern: bool,
    /// The symbols declared in a package or class, or the parameters of a method, in file order.
    pub decls: Vec<SymbolId>,
    /// The file and tree that define it; `None` for packages.
    pub tree: Option<(FileId, NodeId)>,
}
