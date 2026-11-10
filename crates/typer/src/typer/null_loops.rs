// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Nullables.scala
//   compiler/src/dotty/tools/dotc/typer/ProtoTypes.scala
//! The facts about variables that loops and by-name arguments drop (dotty's `whileContext`,
//! `retractMutables`, `postProcessByNameArgs`).

use super::Typer;
use super::nullables::NotNullInfo;
use crate::ctx::Ctx;
use crate::tpd::{Node, TNodeId, Tree as T};
use syntax::ast::Span;
use tasty::{SymbolId, Type, TypeId};

type Cast = (TNodeId, TNodeId, Option<TypeId>);

impl Typer<'_> {
    fn asserted_refs(&self, ctx: Ctx) -> Vec<TypeId> {
        let mut refs: Vec<TypeId> = Vec::new();
        for r in self.not_null_infos(ctx).flat_map(|i| i.asserted().iter().copied()) {
            if !refs.contains(&r) {
                refs.push(r);
            }
        }
        refs
    }

    /// `ctx` of `while` loop `span`: the variables known not null that the loop assigns are not
    /// (dotty's `whileContext`).
    pub fn while_ctx(&mut self, span: Span, ctx: Ctx) -> Ctx {
        let mut retracted = Vec::new();
        for r in self.asserted_refs(ctx) {
            let Some(sym) = self.term_ref_sym(r) else { continue };
            let assigned = self.assignment_spans(sym).is_some_and(|s| s.iter().any(|&a| span.contains(a)));
            if assigned && self.implies_not_null(ctx, r) {
                retracted.push(r);
            }
        }
        self.with_not_null(ctx, NotNullInfo::retracting(retracted))
    }

    /// `ctx` without the facts about mutable variables (dotty's `retractMutables`).
    pub fn retract_mutables(&mut self, ctx: Ctx) -> Ctx {
        let mut retracted = self.asserted_refs(ctx);
        retracted.retain(|&r| match *self.u.types().get(r) {
            Type::Sym(_, s) => self.is_mutable_var(s),
            _ => false,
        });
        self.with_not_null(ctx, NotNullInfo::retracting(retracted))
    }

    /// Argument `i` of a call of method `meth` without the casts flow typing made of variables
    /// outside the definitions in it if parameter `i` of the first clause of the monomorphic,
    /// not inline method is by-name; refused where a use of such a variable expects its type
    /// without `Null` (dotty's `postProcessByNameArgs`).
    pub fn by_name_flow_dropped(&mut self, arg: TNodeId, i: usize, meth: Option<SymbolId>) -> TNodeId {
        let Some(m) =
            meth.filter(|&m| self.explicit_nulls && !self.u.symbol(m).flags.has(tasty::tags::INLINE))
        else {
            return arg;
        };
        let info = self.u.info(m);
        let Type::Method { params, .. } = self.u.types().get(info) else { return arg };
        let Some(&(_, p)) = params.get(i) else { return arg };
        let Type::ByName(formal) = *self.u.types().get(p) else { return arg };
        let mut casts = Vec::new();
        self.var_not_null_casts(arg, Some(formal), &mut casts);
        for &(cast, qualifier, expected) in &casts {
            let q = self.tpd.tpe(qualifier);
            if !expected.is_some_and(|e| self.types().conforms_unlifted(q, e)) {
                let what = "flow typing of a variable in a by-name argument";
                return self.unsupported_tree(self.tpd.span(cast), what);
            }
        }
        for (cast, qualifier, _) in casts {
            let q = self.tpd.get(qualifier);
            let node = Node { tree: q.tree.clone(), span: q.span, tpe: q.tpe, sym: q.sym };
            *self.tpd.get_mut(cast) = node;
        }
        arg
    }

    /// The casts in `t` with their qualifiers and the type their use expects if it is a
    /// method's parameter, `expected` for `t` itself.
    fn var_not_null_casts(&mut self, t: TNodeId, expected: Option<TypeId>, out: &mut Vec<Cast>) {
        match self.tpd.tree(t).clone() {
            T::DefDef { .. } | T::TypeDef { .. } => return,
            T::ValDef { .. }
                if self.tpd.sym(t).is_some_and(|s| self.u.symbol(s).flags.has(tasty::tags::LAZY)) =>
            {
                return;
            }
            T::TypeApply { fun, .. } => {
                if let T::Select { qualifier, .. } = *self.tpd.tree(fun)
                    && let Some(s) = self.tpd.sym(qualifier).filter(|&s| self.is_mutable_var(s))
                    && let Type::And(a, _) = *self.u.types().get(self.tpd.tpe(t))
                    && a == self.tpd.tpe(qualifier)
                    && matches!(self.u.types().get(a), Type::Sym(_, x) if *x == s)
                {
                    out.push((t, qualifier, expected));
                    return;
                }
            }
            T::Apply { fun, args } => {
                let fun_tpe = self.tpd.tpe(fun);
                let info = self.method_info(fun_tpe).unwrap_or(fun_tpe);
                let params = self.u.method(info).map(|m| m.param_types()).unwrap_or_default();
                self.var_not_null_casts(fun, None, out);
                for (j, a) in self.tpd.items(args).to_vec().into_iter().enumerate() {
                    let param = params.get(j).map(|&p| match *self.u.types().get(p) {
                        Type::ByName(r) => r,
                        _ => p,
                    });
                    self.var_not_null_casts(a, param, out);
                }
                return;
            }
            _ => {}
        }
        for c in self.tpd.children(t).to_vec() {
            self.var_not_null_casts(c, None, out);
        }
    }
}
