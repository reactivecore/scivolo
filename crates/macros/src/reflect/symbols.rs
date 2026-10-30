// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The symbols of `quotes.reflect`: owners, names, flags and kinds. Flags are the bits of the
//! TASTy modifier tags, with bits 2 to 5 for `Method`, `Package`, `Param` and `AbsOverride`,
//! bit 50 for `JavaDefined`, bit 51 for `Scala2x` and bit 53 for `Deferred`.

use super::Fallible;
use crate::host::Host;
use tasty::tags::{
    ABSTRACT, FINAL, LAZY, MACRO, OBJECT, OPAQUE, OVERRIDE, PROTECTED, PROTECTEDqualified, SYNTHETIC,
};
use tasty::{Flags, Kind, Symbol, SymbolId, Type, TypeId, Universe};

const METHOD: u32 = 2;
const PACKAGE: u32 = 3;
const PARAM: u32 = 4;
const ABS_OVERRIDE: u32 = 5;
const JAVA_DEFINED: u32 = 50;
const SCALA2X: u32 = 51;
const DEFERRED: u32 = 53;

/// The symbols an expansion makes: the owner of its splice and the empty package.
#[derive(Default)]
pub struct Made {
    macro_owner: Option<SymbolId>,
    empty_package: Option<SymbolId>,
}

impl Host {
    /// The value `macro` that owns the splice (dotty's `Splicer.splice`).
    pub fn macro_owner(&mut self) -> SymbolId {
        let flags = Flags::default().with(MACRO).with(SYNTHETIC);
        let owner = self.owner;
        *self.made.macro_owner.get_or_insert_with(|| local(&mut self.u, "macro", Kind::Val, owner, flags))
    }

    /// The empty package, the owner of the definitions at the root (dotty's `EmptyPackageClass`).
    pub fn empty_package(&mut self) -> SymbolId {
        let root = self.u.root();
        let made = &mut self.made.empty_package;
        *made.get_or_insert_with(|| local(&mut self.u, "<empty>", Kind::Package, root, Flags::default()))
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
        let aliases = self.u.package("scala").and_then(|p| self.u.decl(p, "AnyRef").ok());
        let alias = aliases.into_iter().flatten().find(|&a| self.u.symbol(a).kind == Kind::Type);
        alias.ok_or("a library without scala.AnyRef")
    }

    pub fn predef_class_of(&mut self) -> Fallible<SymbolId> {
        let predef = self.u.package("scala").and_then(|p| self.u.module_class_in(p, "Predef"));
        let class_of = predef.and_then(|c| self.u.decl(c, "classOf").ok()?.first().copied());
        class_of.ok_or("a library without scala.Predef.classOf")
    }

    /// The owner of `s`, the empty package for a definition at the root.
    pub fn owner_of(&mut self, s: SymbolId) -> Option<SymbolId> {
        match self.u.symbol(s).owner {
            Some(o) if o == self.u.root() && self.u.symbol(s).kind != Kind::Package => {
                Some(self.empty_package())
            }
            owner => owner,
        }
    }

    pub fn name(&self, s: SymbolId) -> String {
        let name = self.u.symbol(s).name.to_string();
        match name.is_empty() {
            true if s == self.u.root() => "<root>".into(),
            true => "<empty>".into(),
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
            || self.u.symbol(encl).name == "<empty>"
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
            Kind::Package => Some(s),
            _ if self.u.is_module_class(s) => Some(s),
            _ if self.u.is_module(s) => self.u.module_class(s),
            _ => None,
        }
    }

    /// The flags of `s` as dotty's `TreeUnpickler` reads them (`readModifiers`, `normalizeFlags`),
    /// but for `ParamAccessor`, `NoInits` and `StableRealizable`.
    pub fn flags(&mut self, s: SymbolId) -> u64 {
        let deferred = if self.is_deferred(s) { 1 << DEFERRED } else { 0 };
        let sym = self.u.symbol(s);
        let has = |tag: u8| sym.flags.has(tag);
        let bit = |tag: u8| 1u64 << tag;
        let mut bits = sym.flags.0 & !0b11;
        if has(PROTECTEDqualified) {
            bits |= bit(PROTECTED);
        }
        if sym.kind.is_term() && has(ABSTRACT) && has(OVERRIDE) {
            bits = bits & !bit(ABSTRACT) | 1 << ABS_OVERRIDE;
        }
        if has(OBJECT) {
            bits |= bit(FINAL) | if sym.kind.is_term() { bit(LAZY) } else { 0 };
        }
        let in_class = sym.owner.is_some_and(|o| self.u.symbol(o).kind == Kind::Class);
        bits | deferred
            | match sym.kind {
                Kind::Class if self.u.is_scala2(s) => 1 << SCALA2X,
                Kind::Def => 1 << METHOD,
                Kind::Package => 1 << PACKAGE | 1 << JAVA_DEFINED | bit(OBJECT) | bit(FINAL),
                Kind::Param if in_class => 0,
                Kind::Param | Kind::TypeParam => 1 << PARAM,
                _ => 0,
            }
    }

    /// Whether `s` is a value or method member of a class without a definition, but for a
    /// constructor, an abstract type member or an opaque type alias.
    fn is_deferred(&mut self, s: SymbolId) -> bool {
        let sym = self.u.symbol(s);
        let (kind, tasty, opaque) = (sym.kind, sym.tree.is_some(), sym.flags.has(OPAQUE));
        if !sym.owner.is_some_and(|o| self.u.is_class(o)) || self.u.is_constructor(s) {
            return false;
        }
        match kind {
            Kind::Val | Kind::Def if tasty => self.u.body(s).is_none(),
            Kind::Val | Kind::Def => self.u.is_deferred(s),
            Kind::Type => {
                let mut info = self.u.info(s);
                if let Type::Lambda { result, .. } = self.u.types().get(info) {
                    info = *result;
                }
                opaque || matches!(self.u.types().get(info), Type::Bounds(..))
            }
            _ => false,
        }
    }
}

/// Symbol `name` of `kind` with `flags`, owned by `owner` but none of its declarations.
fn local(u: &mut Universe, name: &str, kind: Kind, owner: SymbolId, flags: Flags) -> SymbolId {
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
