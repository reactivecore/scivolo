//! What a symbol is: its kind, flags, owner and declarations.

use crate::NodeId;
use crate::tags::{
    CASE, CASEaccessor, CONTRAVARIANT, COVARIANT, EXTENSION, FINAL, GIVEN, IMPLICIT, MUTABLE, OPAQUE,
    PRIVATE, PRIVATEqualified, PROTECTEDqualified, SYNTHETIC,
};

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

    /// Whether a symbol of this kind is a type: a class, type member or type parameter.
    pub fn is_type(self) -> bool {
        matches!(self, Kind::Class | Kind::Type | Kind::TypeParam)
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

    pub fn without(self, tag: u8) -> Flags {
        Flags(self.0 & !Self::bit(tag).map_or(0, |b| 1 << b))
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
    /// Whether its name is of a kind other than dotty's `SimpleNameKind`: a default getter, a
    /// unique, expanded or super accessor name, a module class.
    pub derived: bool,
    /// The symbols declared in a package or class, or the parameters of a method, in file order.
    pub decls: Vec<SymbolId>,
    /// The file and tree that define it; `None` for packages.
    pub tree: Option<(FileId, NodeId)>,
}

impl Symbol {
    pub fn is_term(&self) -> bool {
        self.kind.is_term()
    }

    pub fn is_class(&self) -> bool {
        self.kind == Kind::Class
    }

    pub fn is_package(&self) -> bool {
        self.kind == Kind::Package
    }

    /// A term with the `implicit` or `given` modifier.
    pub fn is_implicit(&self) -> bool {
        self.is_term() && (self.flags.has(IMPLICIT) || self.flags.has(GIVEN))
    }

    /// With the `implicit` modifier, a Scala 2 style implicit.
    pub fn is_old_implicit(&self) -> bool {
        self.flags.has(IMPLICIT)
    }

    pub fn is_given(&self) -> bool {
        self.flags.has(GIVEN)
    }

    pub fn is_private(&self) -> bool {
        self.flags.has(PRIVATE)
    }

    pub fn is_extension(&self) -> bool {
        self.flags.has(EXTENSION)
    }

    /// A `val`, not a `var`.
    pub fn is_stable_val(&self) -> bool {
        self.kind == Kind::Val && !self.flags.has(MUTABLE)
    }

    /// A term member of a class: a term other than a parameter.
    pub fn is_term_member(&self) -> bool {
        self.is_term() && self.kind != Kind::Param
    }

    /// The name it has at run time: its `@targetName`, else its name.
    pub fn run_name(&self) -> &str {
        self.target.as_deref().unwrap_or(&self.name)
    }

    /// 1 if covariant, -1 if contravariant, else 0.
    pub fn variance(&self) -> i32 {
        if self.flags.has(CONTRAVARIANT) { -1 } else { i32::from(self.flags.has(COVARIANT)) }
    }

    /// A `case class`, `case object` or the module class of one, or a `case` of an `enum`.
    pub fn is_case(&self) -> bool {
        self.flags.has(CASE)
    }

    /// A `final` class or member.
    pub fn is_final(&self) -> bool {
        self.flags.has(FINAL)
    }

    /// An `opaque type`.
    pub fn is_opaque(&self) -> bool {
        self.flags.has(OPAQUE)
    }

    /// Made by the compiler, as the `apply` and `unapply` of a case class's companion.
    pub fn is_synthetic(&self) -> bool {
        self.flags.has(SYNTHETIC)
    }

    /// A parameter of a case class's first parameter list, or its field.
    pub fn is_case_accessor(&self) -> bool {
        self.flags.has(CASEaccessor)
    }
}
