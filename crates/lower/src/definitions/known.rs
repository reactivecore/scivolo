//! The library symbols the lowering compares symbols with, found among those already entered.

use crate::Lower;
use tasty::defn::{
    ARRAY, EMPTY_TUPLE, JAVA_ENUM, NAMED_TUPLE, PAIR, PREDEF_CLASS, REFLECTIVE_SELECTABLE, REPEATED,
    TUPLES_CLASS,
};
use tasty::{Kind, SymbolId, Universe};

const STRING_CONTEXT: &str = "scala.StringContext";

/// A library symbol that a symbol is compared with; a comparison loads no TASTy for it.
#[derive(Clone, Copy)]
pub enum Known {
    Array,
    /// The `Array` object, its value, and its class.
    ArrayModule,
    ArrayModuleClass,
    /// The class of the `ClassTag` object.
    ClassTagModuleClass,
    /// The class of the `scala.runtime.ScalaRunTime` object.
    ScalaRunTimeClass,
    Enumeration,
    /// `StringContext`, its object's value and the object's class.
    StringContext,
    StringContextModule,
    StringContextModuleClass,
    Repeated,
    PredefClass,
    /// The class of non-empty tuples, `*:`.
    Pair,
    /// The `EmptyTuple` object, its value.
    EmptyTuple,
    /// The class of the `scala.runtime.Tuples` object.
    TuplesClass,
    /// The opaque type `NamedTuple.NamedTuple`.
    NamedTuple,
    Tuple1,
    Tuple2,
    JavaEnum,
    ReflectiveSelectable,
}

/// The number of `Known` symbols.
pub const COUNT: usize = 19;

impl Known {
    /// Returns the full name and the kind of the symbol.
    fn path(self) -> (&'static str, Kind) {
        match self {
            Known::Array => (ARRAY, Kind::Class),
            Known::ArrayModule => (ARRAY, Kind::Val),
            Known::ArrayModuleClass => ("scala.Array$", Kind::Class),
            Known::ClassTagModuleClass => ("scala.reflect.ClassTag$", Kind::Class),
            Known::ScalaRunTimeClass => ("scala.runtime.ScalaRunTime$", Kind::Class),
            Known::Enumeration => ("scala.Enumeration", Kind::Class),
            Known::StringContext => (STRING_CONTEXT, Kind::Class),
            Known::StringContextModule => (STRING_CONTEXT, Kind::Val),
            Known::StringContextModuleClass => ("scala.StringContext$", Kind::Class),
            Known::Repeated => (REPEATED, Kind::Class),
            Known::PredefClass => (PREDEF_CLASS, Kind::Class),
            Known::Pair => (PAIR, Kind::Class),
            Known::EmptyTuple => (EMPTY_TUPLE, Kind::Val),
            Known::TuplesClass => (TUPLES_CLASS, Kind::Class),
            Known::NamedTuple => (NAMED_TUPLE, Kind::Type),
            Known::Tuple1 => ("scala.Tuple1", Kind::Class),
            Known::Tuple2 => ("scala.Tuple2", Kind::Class),
            Known::JavaEnum => (JAVA_ENUM, Kind::Class),
            Known::ReflectiveSelectable => (REFLECTIVE_SELECTABLE, Kind::Class),
        }
    }
}

impl Lower<'_> {
    /// Determines whether `s` is the library symbol `k`, kept once entered; loads nothing.
    pub fn is_known(&self, s: SymbolId, k: Known) -> bool {
        let slot = &self.defs.known[k as usize];
        if slot.get().is_none() {
            slot.set(entered(self.u, k.path()));
        }
        slot.get() == Some(s)
    }
}

/// The symbol of full name `full` and kind `kind` if entered: its package's, or each owner's
/// declaration of its name entered so far.
fn entered(u: &Universe, (full, kind): (&str, Kind)) -> Option<SymbolId> {
    let (owner, name) = full.rsplit_once('.')?;
    let owner = match u.package(owner) {
        Some(p) => p,
        None => entered(u, (owner, Kind::Class))?,
    };
    u.symbol(owner).decls.iter().copied().find(|&d| u.symbol(d).name == name && u.symbol(d).kind == kind)
}
