//! A view on the qualifier of an application whose method does not apply to the arguments
//! (dotty's `tryWithImplicitOnQualifier`, `tryInsertImplicitOnQualifier`).

use super::implicits::conversions::{Goal, Viewed};
use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::{NodeId, Tree};
use tasty::{SymbolId, Type, TypeId};

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
        let ((fits, types), _, _) = self.speculatively(|t| {
            let mut types = Vec::new();
            for &a in &args {
                let typed = t.typed(a, Pt::Any, arg_ctx);
                let tpe = t.zonk(t.tpd.tpe(typed));
                let info = t.method_info(tpe).unwrap_or(tpe);
                if matches!(t.u.types().get(info), Type::Method { .. } | Type::Poly { .. }) {
                    return (true, types);
                }
                types.push(tpe);
            }
            let fits =
                syms.iter().any(|&s| t.applicable(s, Some(prefix), &types, true, arg_ctx) != Some(false));
            (fits, types)
        });
        let diagnosed = self.diags.items.len() > reported;
        self.diags.items.truncate(reported);
        if fits || diagnosed {
            return None;
        }
        let reported = self.diags.items.len();
        let (applies, clean, _) = self.speculatively(|t| match t.viewed(qual, Goal::Member(name), ctx) {
            Ok(Some(Viewed::Converted(q))) => Some(t.member_applies(q, name, &types, ctx)),
            Ok(None) => Some(false),
            _ => None,
        });
        self.diags.items.truncate(reported);
        match applies {
            _ if !clean => None,
            Some(true) => match self.viewed(qual, Goal::Member(name), ctx) {
                Ok(Some(Viewed::Converted(q))) => Some(q),
                _ => None,
            },
            Some(false) => self.arg_scope_view(qual, name, &types, ctx),
            None => None,
        }
    }

    /// Whether a member `name` of `q` applies to arguments of types `args`.
    fn member_applies(&mut self, q: TNodeId, name: &str, args: &[TypeId], ctx: Ctx) -> bool {
        let tpe = self.tpd.tpe(q);
        let members = self.type_members(tpe, name);
        let (applies, _, _) = self.speculatively(|t| {
            members.iter().any(|&s| t.applicable(s, Some(tpe), args, true, ctx) != Some(false))
        });
        applies
    }

    /// `qual` converted to have a member `name` by the only conversion of the implicit scopes of
    /// argument types `args` that gives it one (the scope of dotty's `SelectionProto` of a
    /// `FunProto`).
    fn arg_scope_view(&mut self, qual: TNodeId, name: &str, args: &[TypeId], ctx: Ctx) -> Option<TNodeId> {
        let mut candidates = Vec::new();
        for &a in args {
            for c in self.implicit_scope(a).ok()? {
                if !candidates.contains(&c) {
                    candidates.push(c);
                }
            }
        }
        let mut fitting = Vec::new();
        let reported = self.diags.items.len();
        for c in candidates {
            let (fits, clean, _) = self.speculatively(|t| {
                let converted = t.try_conversion(c, (qual, false), Goal::Member(name), ctx);
                converted.is_ok_and(|q| t.member_applies(q, name, args, ctx))
            });
            self.diags.items.truncate(reported);
            if fits && clean {
                fitting.push(c);
            }
        }
        let [c] = fitting[..] else { return None };
        self.try_conversion(c, (qual, false), Goal::Member(name), ctx).ok()
    }

    fn is_plain_arg(&self, a: NodeId) -> bool {
        match *self.ast.tree(a) {
            Tree::Literal { .. } | Tree::Number { .. } | Tree::Ident { .. } => true,
            Tree::Select { qualifier, .. } => self.is_plain_arg(qualifier),
            Tree::TypeApply { fun, .. } => self.is_plain_arg(fun),
            Tree::Apply { fun, args } => {
                self.is_plain_arg(fun) && self.ast.items(args).iter().all(|&a| self.is_plain_arg(a))
            }
            _ => false,
        }
    }
}
