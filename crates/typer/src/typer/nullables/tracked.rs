// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Nullables.scala
//! The references flow typing tracks (dotty's `TrackedRef`, `usedOutOfOrder`,
//! `toNotNullTermRef`).

use super::super::Typer;
use super::super::stability::is_stable_path;
use super::{Flow, Nullables};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use tasty::{Kind, SymbolId, Type, TypeId, Universe, tags};

impl Typer<'_> {
    /// Casts reference `tree` to `ref.type & T` if it is known not to be null and of type
    /// `T | Null`, as `x` in `if x != null then x.length else 0` (dotty's `toNotNullTermRef`);
    /// the cast of a selection keeps its span.
    pub fn to_not_null_ref(&mut self, tree: TNodeId, select: bool, ctx: Ctx) -> TNodeId {
        let r = self.tpd.tpe(tree);
        let Some(sym) = term_ref_sym(self.u, r) else { return tree };
        if !self.nullables.implies_not_null(ctx, r) || used_out_of_order(self.u, sym, ctx) {
            return tree;
        }
        let stripped = self.strip_null(r);
        if stripped == r {
            return tree;
        }
        let to = self.u.intern_type(Type::And(r, stripped));
        let span = self.tpd.span(tree);
        let cast_span = if select { span } else { span.to_synthetic() };
        match self.build().cast_to(tree, to, cast_span) {
            Ok(t) => {
                if let T::TypeApply { fun, args } = *self.tpd.tree(t) {
                    let tpt = self.tpd.items(args)[0];
                    self.tpd.get_mut(tpt).span = span.end_pos();
                    self.tpd.get_mut(fun).span = span.to_synthetic();
                }
                t
            }
            Err(what) => self.unsupported_tree(span, what),
        }
    }
}

impl Nullables {
    /// Finds the type of `t` if flow typing tracks it: a stable path, a field annotated
    /// `@stableNull` of one, or a local `var` assigned only where its block reaches (dotty's
    /// `isTracked`).
    pub fn tracked(&mut self, f: &mut Flow, t: TNodeId) -> Option<TypeId> {
        let r = f.tpd.tpe(t);
        let u = &mut *f.types.u;
        let sym = term_ref_sym(u, r)?;
        if is_stable_path(u, r) || is_null_stable_field(u, r, sym) {
            return Some(r);
        }
        self.assignment_spans(f, sym).is_some().then_some(r)
    }
}

/// Finds the term `r` refers to.
pub fn term_ref_sym(u: &Universe, r: TypeId) -> Option<SymbolId> {
    match *u.types().get(r) {
        Type::Sym(_, s) if u.symbol(s).kind.is_term() => Some(s),
        _ => None,
    }
}

/// Whether `r` selects field `sym` annotated `@stableNull` from a stable prefix.
fn is_null_stable_field(u: &mut Universe, r: TypeId, sym: SymbolId) -> bool {
    let Type::Sym(Some(p), _) = *u.types().get(r) else { return false };
    let s = u.symbol(sym);
    let field = s.kind == Kind::Val && s.owner.is_some_and(|o| u.symbol(o).kind == Kind::Class);
    field
        && u.annotations(sym).iter().any(|&c| u.full_name(c) == "scala.annotation.stableNull")
        && is_stable_path(u, p)
}

/// Whether a use of local variable `sym` in `ctx` is in a method, lazy value or class inside
/// its owner.
fn used_out_of_order(u: &Universe, sym: SymbolId, ctx: Ctx) -> bool {
    let Some(owner) = u.symbol(sym).owner else { return false };
    if !u.symbol(sym).is_mutable_var() || !u.symbol(owner).kind.is_term() {
        return false;
    }
    let mut s = Some(ctx.owner);
    while let Some(o) = s.filter(|&o| o != owner) {
        let os = u.symbol(o);
        if matches!(os.kind, Kind::Def | Kind::Class) || os.flags.has(tags::LAZY) {
            return true;
        }
        s = os.owner;
    }
    false
}
