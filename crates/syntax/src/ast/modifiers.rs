//! Modifiers of definitions (dotty's `untpd.Modifiers` and `Mod`).

use super::flags::{self, FlagSet};
use super::span::Span;
use super::tree::List;
use crate::names::Name;

/// A modifier as written, with its position.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ModKind {
    Private,
    Protected,
    Var,
    Update,
    Implicit,
    Given,
    Erased,
    Final,
    Sealed,
    Opaque,
    Open,
    Override,
    Abstract,
    Lazy,
    Inline,
    Transparent,
    Infix,
    Tracked,
    Into,
    Impure,
}

impl ModKind {
    pub fn flags(self) -> FlagSet {
        use ModKind::*;
        match self {
            Private => flags::PRIVATE,
            Protected => flags::PROTECTED,
            Var | Update => flags::MUTABLE,
            Implicit => flags::IMPLICIT,
            Given => flags::GIVEN,
            Erased => flags::ERASED,
            Final => flags::FINAL,
            Sealed => flags::SEALED,
            Opaque => flags::OPAQUE,
            Open => flags::OPEN,
            Override => flags::OVERRIDE,
            Abstract => flags::ABSTRACT,
            Lazy => flags::LAZY,
            Inline => flags::INLINE,
            Transparent => flags::TRANSPARENT,
            Infix => flags::INFIX,
            Tracked => flags::TRACKED,
            Into => flags::INTO,
            Impure => flags::IMPURE,
        }
    }

    /// dotty's class name of this modifier.
    pub fn class_name(self) -> String {
        format!("{self:?}")
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Mod {
    pub kind: ModKind,
    pub span: Span,
}

#[derive(Clone, PartialEq, Debug, Default)]
pub struct Modifiers {
    pub flags: FlagSet,
    /// The `q` of `private[q]`, a type name.
    pub private_within: Option<Name>,
    pub annotations: List,
    pub mods: Vec<Mod>,
}

impl Modifiers {
    pub fn with_flags(flags: FlagSet) -> Modifiers {
        Modifiers { flags, ..Modifiers::default() }
    }

    /// These modifiers with `flags` added, unchecked (dotty's `|`).
    pub fn or(self, flags: FlagSet) -> Modifiers {
        Modifiers { flags: self.flags.or(flags), ..self }
    }

    pub fn and_not(self, flags: FlagSet) -> Modifiers {
        Modifiers { flags: self.flags.and_not(flags), ..self }
    }

    /// These modifiers without `flags` and the written modifiers that set them (dotty's
    /// `withoutFlags`).
    pub fn without_flags(self, flags: FlagSet) -> Modifiers {
        if !self.is_one_of(flags) {
            return self;
        }
        let mods = self.mods.into_iter().filter(|m| !m.kind.flags().is_one_of(flags)).collect();
        Modifiers { flags: self.flags.and_not(flags), mods, ..self }
    }

    pub fn is(&self, flag: FlagSet) -> bool {
        self.flags.is(flag)
    }

    pub fn is_one_of(&self, flags: FlagSet) -> bool {
        self.flags.is_one_of(flags)
    }
}
