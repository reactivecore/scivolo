// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeOps.scala
//   compiler/src/dotty/tools/dotc/typer/TypeAssigner.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Types without references to local symbols (dotty's `ensureNoLocalRefs`, `TypeOps.avoid`).

mod class_bound;
mod map;
mod select;

use crate::tpd::{TNodeId, Tree as T};
use crate::typer::{Pt, Typer};
use tasty::{SymbolId, Type, TypeId, Universe};

/// One approximation of a type by a type that no longer mentions definitions going out of scope
/// (dotty's `AvoidMap`), as the type of `{ class C; val c = new C; c }` becomes a type the code
/// outside the block can name. It gives up when the type mentions an opaque type whose bounds
/// are unknown, or comes back to a definition or bound it is already expanding, as a local class
/// whose members mention the class itself. A nested part is approximated by a copy that tracks
/// what is being expanded there.
#[derive(Clone, Copy)]
pub struct Avoid<'a> {
    /// The definitions going out of scope.
    locals: &'a [SymbolId],
    /// The opaque types whose bounds are unknown.
    opaque: &'a [SymbolId],
    /// The local definitions being expanded.
    visiting: &'a [SymbolId],
    /// The type members whose bounds are being expanded.
    expanding: &'a [SymbolId],
    /// The type member whose own declared bounds are being read: a reference back to it stands
    /// for any type (dotty's `LazyRef` of an F-bound).
    own: Option<SymbolId>,
    /// Whether the bound of a type variable is being read: type variables in it stay as they are.
    in_var_bound: bool,
}

impl<'a> Avoid<'a> {
    /// Starts an approximation that avoids `locals` and gives up on the opaque types `opaque`.
    pub fn new(locals: &'a [SymbolId], opaque: &'a [SymbolId]) -> Self {
        Avoid { locals, opaque, visiting: &[], expanding: &[], own: None, in_var_bound: false }
    }

    /// Copies the approximation for a part outside the bounds of the member being expanded.
    fn elsewhere(self) -> Self {
        Avoid { own: None, ..self }
    }
}

impl Typer<'_> {
    /// `{ defs; tree }`, or `tree` if there are no definitions (dotty's `wrapDefs`), its type
    /// avoiding the definitions (dotty's `avoidingType`); outside the subset if it still refers
    /// to them or is not a supertype of the original.
    pub fn wrap_defs(&mut self, defs: Vec<TNodeId>, tree: TNodeId) -> TNodeId {
        if defs.is_empty() {
            return tree;
        }
        let span = defs.iter().fold(self.tpd.span(tree), |s, &d| s.union(self.tpd.span(d)));
        let span = syntax::ast::Span::synthetic(span.start(), span.end());
        let syms: Vec<SymbolId> = defs.iter().filter_map(|&d| self.tpd.sym(d)).collect();
        let original = self.tpd.get(tree).tpe;
        let tpe = original.map(|t| {
            let avoided = Avoid::new(&syms, &[]).at(self, t, 1);
            match avoided {
                Some(a) if self.refers_to(t, &syms) => self.normalized(a.hi()),
                _ => t,
            }
        });
        let widened = original.zip(tpe).is_some_and(|(t, a)| t != a && !self.u.is_subtype(t, a));
        if widened || tpe.is_some_and(|t| self.refers_to(t, &syms)) {
            self.unsupported_at(span, "type of a lifted call referring to a lifted value");
        }
        let stats = self.tpd.list(&defs);
        self.tpd.node(T::Block { stats, expr: tree }, span, tpe, None)
    }

    /// Block result `expr`, ascribed a type without references to `locals` if its own has some
    /// (dotty's `ensureNoLocalRefs`): the upper approximation of `TypeOps.avoid`, or `pt` if that
    /// does not conform to `pt`; `None` if a reference remains, the approximation is out of the
    /// subset, or a type of `opaque`, whose bounds are unknown, is reached.
    pub fn no_local_refs(
        &mut self,
        expr: TNodeId,
        locals: &[SymbolId],
        opaque: &[SymbolId],
        pt: Pt,
    ) -> Option<TNodeId> {
        let tpe = self.tpd.tpe(expr);
        let tpe = self.zonk(tpe);
        if !self.types().escapes(tpe, locals) && !self.types().var_bounds_refer_to(tpe, locals) {
            return Some(expr);
        }
        let avoided = Avoid::new(locals, opaque).at(self, tpe, 1)?.hi();
        let mut avoided = self.normalized(avoided);
        if let Pt::Type(p) = pt
            && let p = self.zonk(p)
            && self.is_fully_defined(p)
            && !self.u.is_subtype(avoided, p)
        {
            avoided = p;
        }
        let checked = self.without_rec_refinements(avoided);
        if self.refers_to(avoided, locals)
            || self.types().var_bounds_refer_to(avoided, locals)
            || !self.u.is_subtype(tpe, checked)
        {
            return None;
        }
        Some(self.ascribed(expr, avoided))
    }
}

/// Checks whether `t` is a singleton type: a path, `this`, a constant or a skolem.
fn is_path(u: &mut Universe, t: TypeId) -> bool {
    match *u.types().get(t) {
        Type::TermRef(..) | Type::This(_) | Type::Constant(..) | Type::Str(_) | Type::Skolem(..) => true,
        Type::Sym(_, s) => u.symbol(s).kind.is_term(),
        _ => false,
    }
}
