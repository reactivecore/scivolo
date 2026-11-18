// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Checking.scala
//   compiler/src/dotty/tools/dotc/typer/TypeAssigner.scala
//! Private definitions named in the signatures of definitions visible outside their class (dotty's
//! `checkNoPrivateLeaks`).

mod walk;

use crate::typer::Typer;
use names::nme;
use syntax::ast::Span;
use syntax::ast::flags as f;
use tasty::tags::OPAQUE;
use tasty::{Kind, SymbolId, Type, TypeId};

/// The walk over the signature of one definition.
struct PrivateLeaks {
    /// The class, package or term up to which the definition is visible.
    boundary: SymbolId,
    /// The private definitions found to leak, in order.
    leaks: Vec<SymbolId>,
    /// The private aliases being expanded.
    expanding: Vec<SymbolId>,
}

impl Typer<'_> {
    /// Checks the signature of `sym`, defined at `span` and just completed, for private definitions
    /// that code seeing `sym` cannot see, as `def f: P` beside `private class P` (dotty's
    /// `avoidPrivateLeaks`). Covers members of classes and packages other than private, synthetic
    /// and Java ones, class parents included. A private alias is replaced by its right-hand side
    /// in the info if that leaks nothing; any other leak is an error, the last one found reported
    /// as dotty's reporter keeps one error per position.
    pub fn avoid_private_leaks(&mut self, sym: SymbolId, span: Span) {
        if !self.is_leak_checked(sym) {
            return;
        }
        let boundary = self.leak_boundary(sym);
        let mut walk = PrivateLeaks { boundary, leaks: Vec::new(), expanding: Vec::new() };
        let is_class = self.u.is_class(sym);
        let signature = match is_class {
            true => walk.check_parents(self, sym),
            false => walk.check_info(self, sym),
        };
        if let Some(&leak) = walk.leaks.last() {
            let shown: Vec<String> = signature.into_iter().map(|t| self.u.show_user(t)).collect();
            let parameterless =
                self.u.symbol(sym).kind == Kind::Def && !shown.concat().starts_with(['(', '[']);
            let shown = match is_class {
                true => format!("{} {{...}}", shown.join(" with ")),
                false if parameterless => format!("=> {}", shown.concat()),
                false => shown.concat(),
            };
            let (what, other) = (self.located(sym), self.syms.described(self.u, leak));
            let text = format!("non-private {what} refers to private {other}\nin its type signature {shown}");
            self.error_at(span, text);
        }
    }

    /// Whether dotty checks the signature of `sym`: a member of a class or package that is neither
    /// private, synthetic nor Java-defined, nor a class's type parameter, which is private there.
    fn is_leak_checked(&self, sym: SymbolId) -> bool {
        let s = self.u.symbol(sym);
        let excluded = f::PRIVATE.or(f::SYNTHETIC).or(f::ARTIFACT).or(f::JAVA_DEFINED);
        s.owner.is_some_and(|o| is_class_like(self.u.symbol(o).kind))
            && s.kind != Kind::TypeParam
            && !s.is_private()
            && !self.syms.flags_of(sym).is_one_of(excluded)
    }

    /// The class, package or term up to which `sym` is visible (dotty's `accessBoundary` of
    /// `checkNoPrivateLeaks`).
    fn leak_boundary(&mut self, sym: SymbolId) -> SymbolId {
        let mut s = sym;
        loop {
            let Some(owner) = self.u.symbol(s).owner else { return s };
            if self.u.symbol(s).is_private() || !is_class_like(self.u.symbol(owner).kind) {
                return owner;
            }
            if let Some(within) = self.u.access_qualifier(s) {
                return within;
            }
            if self.u.symbol(s).kind == Kind::Package {
                return s;
            }
            s = owner;
        }
    }

    /// Whether `sym` lies in `boundary` or is it.
    fn is_contained_in(&self, sym: SymbolId, boundary: SymbolId) -> bool {
        let mut s = Some(sym);
        while let Some(x) = s {
            if x == boundary {
                return true;
            }
            s = self.u.symbol(x).owner;
        }
        false
    }

    /// `sym` by kind and name with its owner, as dotty's `showLocated`.
    fn located(&self, sym: SymbolId) -> String {
        let shown = self.syms.described(self.u, sym);
        match self.u.symbol(sym).owner.filter(|&o| self.u.symbol(o).name != nme::EMPTY_PACKAGE) {
            Some(o) => format!("{shown} in {}", self.syms.described(self.u, o)),
            None => shown,
        }
    }
}

impl PrivateLeaks {
    /// Checks the info of non-class `sym`, the right-hand side of an opaque type left out, and
    /// replaces it if an alias was expanded; the info checked.
    fn check_info(&mut self, t: &mut Typer<'_>, sym: SymbolId) -> Vec<TypeId> {
        let info = t.u.info(sym);
        if t.u.symbol(sym).flags.has(OPAQUE) && matches!(*t.u.ty(info), Type::Alias(_)) {
            return Vec::new();
        }
        let mapped = self.apply(t, info);
        if mapped != info {
            t.u.set_info(sym, mapped);
            if let Some(&result) = t.syms.def_results.get(&sym) {
                let result = self.apply(t, result);
                t.syms.def_results.insert(sym, result);
            }
        }
        vec![info]
    }

    /// Checks the parents of class `cls` and replaces them if an alias was expanded; the parents
    /// checked.
    fn check_parents(&mut self, t: &mut Typer<'_>, cls: SymbolId) -> Vec<TypeId> {
        let parents = t.u.parents(cls);
        let mapped: Vec<TypeId> = parents.iter().map(|&p| self.apply(t, p)).collect();
        if mapped != parents {
            t.u.set_parents(cls, mapped);
        }
        parents
    }
}

/// Whether a symbol of `kind` is a class to dotty, package classes included.
fn is_class_like(kind: Kind) -> bool {
    matches!(kind, Kind::Class | Kind::Package)
}
