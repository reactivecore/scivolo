// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuoteMatcher.scala
//! What the quote matcher reads off trees: term holes, type trees, types, classes and constants.

use super::Matcher;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::TName;
use syntax::ast::constant::Constant;
use tasty::tags::{LAZY, MUTABLE};
use tasty::{SymbolId, Type, TypeId};

const PATTERNS: &str = "scala.quoted.runtime.Patterns$";

impl Matcher<'_> {
    /// The type tree of term hole `patternHole[T]` at `t`.
    pub fn hole(&self, t: TNodeId) -> Option<TNodeId> {
        let T::TypeApply { fun, args } = *self.tpd.tree(t) else { return None };
        let [tpt] = self.tpd.items(args)[..] else { return None };
        self.is_patterns(fun, "patternHole").then_some(tpt)
    }

    pub fn has_hole(&self, t: TNodeId) -> bool {
        self.tpd.fold(false, t, &mut |found, t| found || self.hole(t).is_some())
    }

    /// Whether `t` refers to member `name` of `scala.quoted.runtime.Patterns`.
    pub fn is_patterns(&self, t: TNodeId, name: &str) -> bool {
        let Some(s) = self.tpd.sym(t) else { return false };
        let sym = self.u.symbol(s);
        sym.name == name && sym.owner.is_some_and(|o| self.u.full_name(o) == PATTERNS)
    }

    /// Whether the type of `s` conforms to the type of pattern tree `p`.
    pub fn conforms(&mut self, s: TNodeId, p: TNodeId) -> bool {
        let (Some(a), Some(b)) = (self.type_of(s), self.tpd.get(p).tpe) else { return false };
        self.bindings.conforms(self.u, a, b)
    }

    /// The type of `t`; a sequence literal of elements `T` the typer has not typed yet is given
    /// type `Seq[T]` (`Typer::type_seq_literal`).
    fn type_of(&mut self, t: TNodeId) -> Option<TypeId> {
        match *self.tpd.tree(t) {
            T::SeqLiteral { elemtpt, .. } if self.tpd.get(t).tpe.is_none() => {
                let seq = self.u.class_ref("scala.collection.immutable.Seq");
                let elem = self.tpd.get(elemtpt).tpe?;
                let tpe = self.u.intern_type(Type::Applied(seq, vec![elem]));
                self.tpd.get_mut(t).tpe = Some(tpe);
                Some(tpe)
            }
            _ => self.tpd.get(t).tpe,
        }
    }

    pub fn same_type(&mut self, a: TypeId, b: TypeId) -> bool {
        a == b || self.u.is_subtype(a, b) && self.u.is_subtype(b, a)
    }

    pub fn is_repeated(&mut self, tpt: TNodeId) -> bool {
        let Some(t) = self.tpd.get(tpt).tpe else { return false };
        self.u.class_symbol(t).is_some_and(|c| self.u.full_name(c) == "scala.<repeated>")
    }

    pub fn is_type_tree(&self, t: TNodeId) -> bool {
        match self.tpd.tree(t) {
            T::Ident { .. } | T::Select { .. } => {
                self.tpd.sym(t).is_some_and(|s| self.u.symbol(s).kind.is_type())
            }
            T::TypeTree { .. }
            | T::SingletonTypeTree { .. }
            | T::ByNameTypeTree { .. }
            | T::AppliedTypeTree { .. }
            | T::Annotated { .. }
            | T::LambdaTypeTree { .. }
            | T::RefinedTypeTree { .. } => true,
            _ => false,
        }
    }

    /// A reference to the prefix of identifier `s` that a selection's qualifier in the pattern
    /// matches (dotty's `tpd.ref(qual)` in `=?=`): the value it is a member of, or the object
    /// of the module class whose `this` it is; `None` for other prefixes.
    pub fn prefix_ref(&mut self, s: TNodeId) -> Option<TNodeId> {
        let prefix = match *self.u.types().get(self.tpd.get(s).tpe?) {
            Type::Sym(Some(p), _) | Type::TermRef(p, _) => p,
            _ => return None,
        };
        let (sym, tpe) = match *self.u.types().get(prefix) {
            Type::Sym(_, v) if self.u.symbol(v).kind.is_term() => (v, prefix),
            Type::This(c) => {
                let c = self.u.class_symbol(c).filter(|&c| self.u.is_module_class(c))?;
                let m = self.u.source_module(c)?;
                (m, self.u.intern_type(Type::Sym(None, m)))
            }
            _ => return None,
        };
        let name = TName { name: self.names.intern(&self.u.symbol(sym).name.clone()), is_type: false };
        let span = self.tpd.span(s);
        Some(self.tpd.node(T::Ident { name }, span, Some(tpe), Some(sym)))
    }

    /// Member `a` selected from `qualifier` as the member of the qualifier's class that overrides
    /// it, if there is one (dotty's `overridingSymbol` in `symbolMatch`).
    pub fn devirtualized(&mut self, a: SymbolId, qualifier: TNodeId) -> SymbolId {
        let Some(tpe) = self.tpd.get(qualifier).tpe else { return a };
        let widened = self.u.widen(tpe);
        let Some(c) = self.u.class_symbol(widened).filter(|&c| self.u.is_class(c)) else { return a };
        let name = self.u.symbol(a).name.clone();
        let site = self.u.this_type(c);
        for base in self.u.linearization(c) {
            for d in self.u.decl(base, &name).unwrap_or_default() {
                if d == a || self.u.symbol(d).kind.is_term() && self.u.matches(site, d, a) {
                    return d;
                }
            }
        }
        a
    }

    /// Whether member `a` overrides member `b` of a base class of its owner (dotty's
    /// `allOverriddenSymbols.contains`).
    pub fn overrides(&mut self, a: SymbolId, b: SymbolId) -> bool {
        let (Some(oa), Some(ob)) = (self.u.symbol(a).owner, self.u.symbol(b).owner) else { return false };
        if oa == ob || !self.u.derives_from(oa, ob) || self.u.symbol(a).name != self.u.symbol(b).name {
            return false;
        }
        let site = self.u.this_type(oa);
        self.u.matches(site, a, b)
    }

    /// Whether definitions `s` and `p` are both or neither `lazy` and both or neither `var`.
    pub fn same_val_flags(&self, s: TNodeId, p: TNodeId) -> bool {
        let (Some(a), Some(b)) = (self.tpd.sym(s), self.tpd.sym(p)) else { return false };
        let (a, b) = (&self.u.symbol(a).flags, &self.u.symbol(b).flags);
        a.has(LAZY) == b.has(LAZY) && a.has(MUTABLE) == b.has(MUTABLE)
    }

    pub fn class_of(&mut self, tpt: TNodeId) -> Option<SymbolId> {
        let t = self.tpd.get(tpt).tpe?;
        let t = self.u.dealias(t);
        self.u.class_symbol(t)
    }

    /// The class or package `this` tree `t` refers to.
    pub fn this_class(&mut self, t: TNodeId) -> Option<SymbolId> {
        let tpe = self.tpd.get(t).tpe?;
        let Type::This(c) = *self.u.types().get(tpe) else { return None };
        match *self.u.types().get(c) {
            Type::Package(p) => Some(p),
            _ => self.u.class_symbol(c),
        }
    }
}

/// dotty's `Constant.equals`: floating point numbers by their bits.
pub fn same_constant(a: &Constant, b: &Constant) -> bool {
    match (a, b) {
        (Constant::Float(x), Constant::Float(y)) => x.to_bits() == y.to_bits(),
        (Constant::Double(x), Constant::Double(y)) => x.to_bits() == y.to_bits(),
        _ => a == b,
    }
}
