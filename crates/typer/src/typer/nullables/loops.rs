// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Nullables.scala
//   compiler/src/dotty/tools/dotc/typer/ProtoTypes.scala
//! The facts about variables that loops and by-name arguments drop (dotty's `whileContext`,
//! `retractMutables`, `postProcessByNameArgs`).

use super::super::Typer;
use super::tracked::term_ref_sym;
use super::{Flow, NotNullInfo, Nullables, flow};
use crate::ctx::Ctx;
use crate::tpd::{Node, TNodeId, Tree as T};
use syntax::ast::Span;
use tasty::{SymbolId, Type, TypeId, Universe};

/// Holds a cast flow typing made of a variable, `x.$asInstanceOf$[x.type & T]` for a `var x` known
/// not null.
struct FlowCast {
    cast: TNodeId,
    /// The reference to the variable.
    qualifier: TNodeId,
    /// The type the use expects if it is a method's parameter.
    expected: Option<TypeId>,
}

impl Typer<'_> {
    /// Gives `ctx` of `while` loop `span` without the facts about the variables the loop assigns:
    /// in `while x != null do x = x.next` the condition does not know `x` not null from before
    /// the loop (dotty's `whileContext`).
    pub fn while_ctx(&mut self, span: Span, ctx: Ctx) -> Ctx {
        let (nullables, mut f) = flow(self);
        nullables.while_ctx(&mut f, span, ctx)
    }

    /// Drops from argument `i` of a call of method `meth` the casts flow typing made of
    /// variables outside the definitions in it if parameter `i` of the first clause of the
    /// monomorphic, not inline method is by-name: the argument may run after the variable
    /// changed. Refused where a use of such a variable expects its type without `Null` (dotty's
    /// `postProcessByNameArgs`).
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
        var_not_null_casts(self, arg, Some(formal), &mut casts);
        for c in &casts {
            let q = self.tpd.tpe(c.qualifier);
            if !c.expected.is_some_and(|e| self.types().conforms_unlifted(q, e)) {
                let what = "flow typing of a variable in a by-name argument";
                return self.unsupported_tree(self.tpd.span(c.cast), what);
            }
        }
        for c in casts {
            let q = self.tpd.get(c.qualifier);
            let node = Node { tree: q.tree.clone(), span: q.span, tpe: q.tpe, sym: q.sym };
            *self.tpd.get_mut(c.cast) = node;
        }
        arg
    }
}

impl Nullables {
    fn asserted_refs(&self, ctx: Ctx) -> Vec<TypeId> {
        let mut refs: Vec<TypeId> = Vec::new();
        for r in self.not_null_infos(ctx).flat_map(|i| i.asserted().iter().copied()) {
            if !refs.contains(&r) {
                refs.push(r);
            }
        }
        refs
    }

    /// Gives `ctx` of `while` loop `span` without the facts about the variables it assigns.
    fn while_ctx(&mut self, f: &mut Flow, span: Span, ctx: Ctx) -> Ctx {
        let mut retracted = Vec::new();
        for r in self.asserted_refs(ctx) {
            let Some(sym) = term_ref_sym(f.types.u, r) else { continue };
            let assigned = self.assignment_spans(f, sym).is_some_and(|s| s.iter().any(|&a| span.contains(a)));
            if assigned && self.implies_not_null(ctx, r) {
                retracted.push(r);
            }
        }
        self.with_not_null(ctx, NotNullInfo::retracting(retracted))
    }

    /// Gives `ctx` without the facts about mutable variables, for a by-name argument that may
    /// run after they changed (dotty's `retractMutables`).
    pub fn retract_mutables(&mut self, u: &Universe, ctx: Ctx) -> Ctx {
        let mut retracted = self.asserted_refs(ctx);
        retracted.retain(|&r| match *u.types().get(r) {
            Type::Sym(_, s) => u.symbol(s).is_mutable_var(),
            _ => false,
        });
        self.with_not_null(ctx, NotNullInfo::retracting(retracted))
    }
}

/// Adds the casts in `t` to `out`, `expected` the type `t` itself is expected to have.
fn var_not_null_casts(t: &mut Typer, tree: TNodeId, expected: Option<TypeId>, out: &mut Vec<FlowCast>) {
    match t.tpd.tree(tree).clone() {
        T::DefDef { .. } | T::TypeDef { .. } => return,
        T::ValDef { .. } if t.tpd.sym(tree).is_some_and(|s| t.u.symbol(s).flags.has(tasty::tags::LAZY)) => {
            return;
        }
        T::TypeApply { fun, .. } => {
            if let T::Select { qualifier, .. } = *t.tpd.tree(fun)
                && let Some(s) = t.tpd.sym(qualifier).filter(|&s| t.u.symbol(s).is_mutable_var())
                && let Type::And(a, _) = *t.u.types().get(t.tpd.tpe(tree))
                && a == t.tpd.tpe(qualifier)
                && matches!(t.u.types().get(a), Type::Sym(_, x) if *x == s)
            {
                out.push(FlowCast { cast: tree, qualifier, expected });
                return;
            }
        }
        T::Apply { fun, args } => {
            let fun_tpe = t.tpd.tpe(fun);
            let info = t.method_info(fun_tpe).unwrap_or(fun_tpe);
            let params = t.u.method(info).map(|m| m.param_types()).unwrap_or_default();
            var_not_null_casts(t, fun, None, out);
            for (j, a) in t.tpd.items(args).to_vec().into_iter().enumerate() {
                let param = params.get(j).map(|&p| match *t.u.types().get(p) {
                    Type::ByName(r) => r,
                    _ => p,
                });
                var_not_null_casts(t, a, param, out);
            }
            return;
        }
        _ => {}
    }
    for c in t.tpd.children(tree).to_vec() {
        var_not_null_casts(t, c, None, out);
    }
}
