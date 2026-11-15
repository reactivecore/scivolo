//! The origins of an annotated type in a type tree without a span of its own: the annotated type
//! trees in a span, the qualifier of a flow-typing cast and the method parameter of an
//! eta-expanded closure's parameter.

use super::{AnnotationOrigins, Found};
use crate::tpd::{TNodeId, Tpd, Tree as T};
use tasty::tags::OBJECT;
use tasty::{Kind, SymbolId, Type, TypeId, Universe};

/// Parameter `index` of value parameter clause `clause` of `method`.
pub struct ParamOf {
    method: SymbolId,
    clause: usize,
    index: usize,
}

impl AnnotationOrigins {
    /// The annotation tree within the span of type tree `tpt` whose annotated type and tree
    /// `matching` holds of, `Some(None)` if several, `None` if none.
    pub fn derived(&self, tpd: &Tpd, tpt: TNodeId, matching: impl Fn(TypeId, TNodeId) -> bool) -> Found {
        let span = tpd.span(tpt);
        if !span.exists() || span.start() == span.end() {
            return None;
        }
        let mut annots = (self.annotated_at.iter())
            .filter(|&&(start, end, t, annot)| {
                start >= span.start() && end <= span.end() && matching(t, annot)
            })
            .map(|&(.., annot)| annot);
        let first = annots.next()?;
        Some(annots.all(|a| a == first).then_some(first))
    }

    /// Records the right-hand side of tree `t` if it is an ascription `expr: <inferred>`, or a
    /// flow-typing cast `q.$asInstanceOf$[q.type & T]` (dotty's `Typer.typedIdent` /
    /// `typedSelect` of a path known not to be `null`), whose type tree takes its annotations
    /// from `q`.
    pub fn add_ascription_or_cast(&mut self, u: &Universe, tpd: &Tpd, t: TNodeId) {
        match *tpd.tree(t) {
            T::Typed { expr, tpt } if matches!(tpd.tree(tpt), T::TypeTree { inferred: true }) => {
                self.rhs_of.insert(tpt, expr);
            }
            T::TypeApply { fun, args } => {
                let T::Select { qualifier, .. } = *tpd.tree(fun) else { return };
                let &[tpt] = tpd.items(args) else { return };
                let span = tpd.span(tpt);
                let written = matches!(tpd.tree(tpt), T::TypeTree { inferred: false });
                let cast =
                    |c: TypeId| matches!(*u.ty(c), Type::And(q, _) if Some(q) == tpd.get(qualifier).tpe);
                if written && span.start() == span.end() && tpd.get(tpt).tpe.is_some_and(cast) {
                    self.rhs_of.insert(tpt, qualifier);
                }
            }
            _ => {}
        }
    }

    /// Records the value parameter type trees of method definition `t`, and of its parameters
    /// without a span that its call passes on unchanged, the method parameter they stand for (an
    /// eta-expansion `(x) => f(x)`, dotty's `EtaExpansion.etaExpand`).
    pub fn add_params(&mut self, tpd: &Tpd, t: TNodeId) {
        let T::DefDef { paramss, rhs, .. } = *tpd.tree(t) else { return };
        let Some(sym) = tpd.sym(t) else { return };
        let clauses: Vec<Vec<TNodeId>> = (tpd.clause_items(paramss).iter())
            .map(|&c| tpd.items(c).to_vec())
            .filter(|ps| ps.iter().all(|&p| matches!(tpd.tree(p), T::ValDef { .. })))
            .collect();
        let tpts = clauses.iter().map(|ps| ps.iter().filter_map(|&p| tpt_of(tpd, p)).collect());
        self.params.insert(sym, tpts.collect());
        if let Some(rhs) = rhs {
            self.add_eta(tpd, &clauses, rhs);
        }
    }

    /// Records the parameters among `params` without a span that call `rhs` passes on unchanged.
    fn add_eta(&mut self, tpd: &Tpd, params: &[Vec<TNodeId>], mut call: TNodeId) {
        while let T::Block { expr, .. } = *tpd.tree(call) {
            call = expr;
        }
        let mut arg_lists = Vec::new();
        while let T::Apply { fun, args } = *tpd.tree(call) {
            arg_lists.push(args);
            call = fun;
        }
        while let T::TypeApply { fun, .. } = *tpd.tree(call) {
            call = fun;
        }
        let Some(method) = tpd.sym(call) else { return };

        for (clause, &args) in arg_lists.iter().rev().enumerate() {
            for (index, &arg) in tpd.items(args).iter().enumerate() {
                let Some(s) = tpd.sym(arg).filter(|_| matches!(tpd.tree(arg), T::Ident { .. })) else {
                    continue;
                };
                let param = params.iter().flatten().find(|&&p| tpd.sym(p) == Some(s));
                let Some(tpt) = param.and_then(|&p| tpt_of(tpd, p)) else { continue };
                let span = tpd.span(tpt);
                if span.start() == span.end() {
                    self.eta.insert(tpt, ParamOf { method, clause, index });
                }
            }
        }
    }

    /// The method parameter type tree eta-expanded parameter type tree `tpt` stands for.
    pub fn eta_param(&self, tpt: TNodeId) -> Option<TNodeId> {
        let p = self.eta.get(&tpt)?;
        self.params.get(&p.method)?.get(p.clause)?.get(p.index).copied()
    }
}

/// The type tree of value parameter definition `p`.
fn tpt_of(tpd: &Tpd, p: TNodeId) -> Option<TNodeId> {
    match *tpd.tree(p) {
        T::ValDef { tpt, .. } => Some(tpt),
        _ => None,
    }
}

/// Whether type `t` may have part `part`: not if it is a reference to a class, object, type
/// parameter or constant, or made of such parts by type application, intersection, union and
/// annotation.
pub fn may_have_part(u: &Universe, t: TypeId, part: TypeId) -> bool {
    if t == part {
        return true;
    }
    match *u.ty(t) {
        Type::Applied(tycon, args) => {
            may_have_part(u, tycon, part) || u.types_of(args).iter().any(|&a| may_have_part(u, a, part))
        }
        Type::And(a, b) | Type::Or(a, b, _) => may_have_part(u, a, part) || may_have_part(u, b, part),
        Type::Annotated(p, _) | Type::Flexible(p) => may_have_part(u, p, part),
        Type::Sym(prefix, s) => {
            let symbol = u.symbol(s);
            let opaque = symbol.kind == Kind::Type || symbol.kind.is_term() && !symbol.flags.has(OBJECT);
            opaque || prefix.is_some_and(|p| may_have_part(u, p, part))
        }
        Type::This(_) | Type::Package(_) | Type::PackageName(_) | Type::Constant(..) | Type::Str(_) => false,
        Type::Param(..) | Type::Local(..) => false,
        _ => true,
    }
}
