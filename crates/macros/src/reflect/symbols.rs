// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The symbols of `quotes.reflect`: owners, names and kinds.

use super::Fallible;
use crate::host::Host;
use names::nme;
use tasty::{Flags, Kind, Symbol, SymbolId, TypeId, Universe};

/// The flag bit of `Method`.
pub const METHOD: u32 = 2;

/// The symbols an expansion makes: the owner of its splice, the empty package and the root
/// package's value; the arguments of the macro.
#[derive(Default)]
pub struct Made {
    pub macro_owner: Option<SymbolId>,
    pub empty_package: Option<SymbolId>,
    root_package: Option<SymbolId>,
    pub args: super::from_symbol::Args,
}

impl Host {
    /// The empty package, the owner of the definitions at the root (dotty's `EmptyPackageClass`).
    pub fn empty_package(&mut self) -> SymbolId {
        let root = self.u.root();
        let made = &mut self.made.empty_package;
        *made.get_or_insert_with(|| {
            local(&mut self.u, nme::EMPTY_PACKAGE, Kind::Package, root, Flags::default())
        })
    }

    /// The value `_root_` whose module class is the root (dotty's `RootPackage`).
    pub fn root_package(&mut self) -> SymbolId {
        let root = self.u.root();
        let made = &mut self.made.root_package;
        *made.get_or_insert_with(|| local(&mut self.u, nme::ROOT_PKG, Kind::Package, root, Flags::default()))
    }

    pub fn companion_module(&mut self, s: Option<SymbolId>) -> Option<SymbolId> {
        self.reflect().companion_module(s?)
    }

    pub fn declared_method(&mut self, s: Option<SymbolId>, name: String) -> Vec<SymbolId> {
        s.map(|s| self.reflect().declared_methods(s, &name)).unwrap_or_default()
    }

    /// The owner of the splice: the annotated definition's while annotating.
    pub fn splice_owner(&mut self) -> SymbolId {
        if self.annotating { self.owner } else { self.macro_owner() }
    }

    pub fn root(&self) -> SymbolId {
        self.u.root()
    }

    pub fn any_ref_alias(&mut self) -> Fallible<SymbolId> {
        let aliases = self.u.package(nme::SCALA).and_then(|p| self.u.decl(p, "AnyRef").ok());
        let alias = aliases.into_iter().flatten().find(|&a| self.u.symbol(a).kind == Kind::Type);
        alias.ok_or("a library without scala.AnyRef")
    }

    pub fn predef_class_of(&mut self) -> Fallible<SymbolId> {
        let predef = self.u.package(nme::SCALA).and_then(|p| self.u.module_class_in(p, "Predef"));
        let class_of = predef.and_then(|c| self.u.decl(c, nme::CLASS_OF).ok()?.first().copied());
        class_of.ok_or("a library without scala.Predef.classOf")
    }

    /// The owner of `s`, the empty package for a definition at the root, the macro owner for one
    /// in a macro argument.
    pub fn owner_of(&mut self, s: SymbolId) -> Option<SymbolId> {
        match self.u.symbol(s).owner {
            Some(o) if o == self.u.root() && self.u.symbol(s).kind != Kind::Package => {
                Some(self.empty_package())
            }
            owner => owner.map(|o| self.spliced_owner(s, o)),
        }
    }

    pub fn name(&self, s: SymbolId) -> String {
        let name = self.u.symbol(s).name.to_string();
        match name.is_empty() {
            true if s == self.u.root() => nme::ROOT.into(),
            true => nme::EMPTY_PACKAGE.into(),
            false => name,
        }
    }

    /// The names of `s`'s enclosing classes and packages and `s`, joined by `.`, with a `_$` for
    /// each term or type member between `s` and its enclosing class (dotty's `fullName`).
    pub fn full_name(&self, s: SymbolId) -> String {
        let sym = self.u.symbol(s);
        let Some(mut encl) = sym.owner else { return sym.name.to_string() };
        let mut filler = String::new();
        while !matches!(self.u.symbol(encl).kind, Kind::Class | Kind::Package) {
            let Some(o) = self.u.symbol(encl).owner else { break };
            encl = o;
            filler.push_str("_$");
        }
        match encl == self.u.root()
            || self.u.symbol(encl).name.is_empty()
            || self.u.symbol(encl).name == nme::EMPTY_PACKAGE
        {
            true => format!("{filler}{}", sym.name),
            false => format!("{}.{filler}{}", self.full_name(encl), sym.name),
        }
    }

    pub fn info(&mut self, s: SymbolId) -> TypeId {
        self.u.info(s)
    }

    /// The kind of `s` as the overlay numbers it, `-1` for another.
    pub fn symbol_kind(&self, s: SymbolId) -> i32 {
        let kind = self.u.symbol(s).kind;
        let kinds =
            [Kind::Package, Kind::Class, Kind::Type, Kind::TypeParam, Kind::Param, Kind::Val, Kind::Def];
        kinds.iter().position(|&k| k == kind).map_or(-1, |n| n as i32)
    }

    pub fn module_class(&mut self, s: SymbolId) -> Option<SymbolId> {
        match self.u.symbol(s).kind {
            _ if Some(s) == self.made.root_package => Some(self.u.root()),
            Kind::Package => Some(s),
            _ if self.u.is_module_class(s) => Some(s),
            _ if self.u.is_module(s) => self.u.module_class(s),
            _ => None,
        }
    }
}

/// Symbol `name` of `kind` with `flags`, owned by `owner` but none of its declarations.
pub fn local(u: &mut Universe, name: &str, kind: Kind, owner: SymbolId, flags: Flags) -> SymbolId {
    let s = u.define(Symbol {
        name: name.to_owned(),
        kind,
        owner: None,
        flags,
        target: None,
        is_extern: false,
        derived: false,
        decls: Vec::new(),
        tree: None,
    });
    u.set_owner(s, owner);
    s
}
