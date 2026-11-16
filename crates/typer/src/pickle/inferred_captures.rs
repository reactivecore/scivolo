// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/PostTyper.scala
//   compiler/src/dotty/tools/dotc/cc/CaptureOps.scala
//! Capture sets in inferred type trees of capture-checked files, each emptied (dotty's
//! `CleanupRetains` of an inferred `TypeTree` in PostTyper).

use super::TreePickler;
use crate::tpd::{TNodeId, Tpd, Tree};
use crate::typer::capture_types::{has_captures, is_retains_like};
use crate::typer::seen_from::{resolve_class_params, resolve_class_params_keeping_captures};
use tasty::{Kind, SymbolId, Type, TypeId, Universe};

impl TreePickler<'_> {
    /// Records the type inferred type tree `tpt` of a capture-checked file is pickled with if the
    /// type it was inferred from has capture sets and is its type but for them: that type with
    /// each capture set but `retainsCap`'s empty, `B^{}` of `y` in `def g(b: B^{a}) = { val y =
    /// b; .. }`. The type inferred from is the one the typer recorded (a function value's
    /// parameter's), else, unless definition `sym` is a member that may inherit its type, the
    /// info of the definition that its right-hand side `rhs` refers to or calls.
    pub fn empty_captures(&mut self, sym: SymbolId, tpt: TNodeId, rhs: Option<TNodeId>) {
        if !self.file.is_some_and(|f| f.attributes.features.capture_checking()) {
            return;
        }
        let (Tree::TypeTree { inferred: true }, Some(tpe)) = (self.tpd.tree(tpt), self.tpd.get(tpt).tpe)
        else {
            return;
        };
        let rhs = rhs.filter(|_| !may_inherit(self.u, sym));
        let from = self.tpd.captured_type(tpt);
        let Some(from) = from.or_else(|| rhs.and_then(|r| rhs_type(self.u, self.tpd, r))) else { return };
        if !has_captures(self.u, from) {
            return;
        }
        let emptied = emptied(self.u, from);
        let kept = resolve_class_params_keeping_captures(self.u, emptied);
        if kept != tpe && resolve_class_params(self.u, kept) == tpe {
            self.kept_captures.insert(tpt, kept);
        }
    }
}

/// Whether `sym` is a member of a class with a base class that has a term member of its name,
/// whose type it may take (dotty's `inherited` of `inferredResultType`).
fn may_inherit(u: &mut Universe, sym: SymbolId) -> bool {
    let Some(cls) = u.symbol(sym).owner.filter(|&o| u.symbol(o).kind == Kind::Class) else { return false };
    let bases: Vec<SymbolId> = u.linearization(cls).into_iter().skip(1).collect();
    let name = &u.symbol(sym).name;
    bases
        .iter()
        .any(|&b| u.symbol(b).decls.iter().any(|&m| &u.symbol(m).name == name && u.symbol(m).kind.is_term()))
}

/// The declared type of right-hand side `rhs`, a reference to a value or a call of a method
/// without type arguments: the referenced definition's info as seen from the qualifier, the
/// result of a method's after a clause per argument list.
fn rhs_type(u: &mut Universe, tpd: &Tpd, rhs: TNodeId) -> Option<TypeId> {
    let (mut fun, mut clauses) = (rhs, 0);
    loop {
        match *tpd.tree(fun) {
            Tree::Block { expr, .. } if clauses == 0 => fun = expr,
            Tree::Apply { fun: f, .. } => (fun, clauses) = (f, clauses + 1),
            _ => break,
        }
    }
    let s = tpd.sym(fun)?;
    if !matches!(u.symbol(s).kind, Kind::Val | Kind::Param | Kind::Def) {
        return None;
    }
    let mut info = match *tpd.tree(fun) {
        Tree::Ident { .. } => u.info(s),
        Tree::Select { qualifier, .. } => {
            let prefix = tpd.get(qualifier).tpe?;
            match *u.ty(prefix) {
                Type::Package(_) | Type::PackageName(_) => u.info(s),
                _ => u.member_info(prefix, s),
            }
        }
        _ => return None,
    };
    for _ in 0..clauses {
        info = u.method(info)?.result;
    }
    match *u.ty(info) {
        Type::ByName(r) => Some(r),
        _ => u.method(info).is_none().then_some(info),
    }
}

/// `t` with the capture set of each capture-checking annotation but `retainsCap` empty,
/// `retains[Nothing]` (dotty's `CleanupRetains` where capture checking is enabled).
fn emptied(u: &mut Universe, t: TypeId) -> TypeId {
    u.map_type(t, &mut |u, part| {
        let (inner, annot) = u.annotated(part)?;
        if !is_retains_like(u, annot) {
            return None;
        }
        let inner = emptied(u, inner);
        let annot = match u.applied(annot) {
            Some((tycon, _)) => u.intern_applied(tycon, &[tasty::NOTHING]),
            None => annot,
        };
        Some(u.intern_type(Type::Annotated(inner, annot)))
    })
}
