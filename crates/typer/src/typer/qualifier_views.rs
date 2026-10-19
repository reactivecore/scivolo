//! A view on the qualifier of an application whose method does not apply to the arguments
//! (dotty's `tryWithImplicitOnQualifier`, `tryInsertImplicitOnQualifier`).

use super::implicits::conversions::{Goal, Viewed};
use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::{NodeId, Tree};
use tasty::{Kind, SymbolId, TypeId};

impl Typer<'_> {
    /// `qual` converted to have a member `name` when none of `syms`, its members as seen from
    /// `prefix`, applies to the literal or path arguments of `pt`; `None` otherwise.
    pub fn qualifier_view(
        &mut self,
        qual: TNodeId,
        (prefix, syms): (TypeId, &[SymbolId]),
        name: &str,
        pt: Pt,
        ctx: Ctx,
    ) -> Option<TNodeId> {
        let Pt::Fun(proto) = pt else { return None };
        let args = self.untupled_args(proto, syms).unwrap_or_else(|| self.protos[proto].args.clone());
        let arg_ctx = self.protos[proto].ctx;
        if name == "<init>"
            || self.protos[proto].typed.is_some()
            || !args.iter().all(|&a| self.is_plain_arg(a))
        {
            return None;
        }
        let reported = self.diags.items.len();
        let (fits, _, _) = self.speculatively(|t| {
            let mut types = Vec::new();
            for &a in &args {
                let typed = t.typed(a, Pt::Any, arg_ctx);
                let tpe = t.tpd.tpe(typed);
                let method =
                    t.tpd.sym(t.tpd.meth_part(typed)).is_some_and(|s| t.u.symbol(s).kind == Kind::Def);
                if method || t.method_info(tpe).is_some() {
                    return true;
                }
                types.push(tpe);
            }
            syms.iter().any(|&s| t.applicable(s, Some(prefix), &types, true, arg_ctx) != Some(false))
        });
        let diagnosed = self.diags.items.len() > reported;
        self.diags.items.truncate(reported);
        if fits || diagnosed {
            return None;
        }
        match self.viewed(qual, Goal::Member(name), ctx) {
            Ok(Some(Viewed::Converted(q))) => Some(q),
            _ => None,
        }
    }

    fn is_plain_arg(&self, a: NodeId) -> bool {
        match *self.ast.tree(a) {
            Tree::Literal { .. } | Tree::Ident { .. } => true,
            Tree::Select { qualifier, .. } => self.is_plain_arg(qualifier),
            Tree::TypeApply { fun, .. } => self.is_plain_arg(fun),
            Tree::Apply { fun, args } => {
                self.is_plain_arg(fun) && self.ast.items(args).iter().all(|&a| self.is_plain_arg(a))
            }
            _ => false,
        }
    }
}
