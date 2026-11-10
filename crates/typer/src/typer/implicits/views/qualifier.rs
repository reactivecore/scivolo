// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! A view on the qualifier of an application whose method does not apply to the arguments
//! (dotty's `tryWithImplicitOnQualifier`, `tryInsertImplicitOnQualifier`).

use super::super::super::{Pt, Typer};
use super::{Goal, ViewSearch, Viewed};
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::{Ast, NodeId, Tree};
use syntax::config::Features;
use tasty::{SymbolId, Type, TypeId};

/// How a selection whose members do not apply to the arguments is retried.
pub enum QualifierView {
    /// As a call of the extension methods in scope.
    Extension,
    /// On the qualifier converted.
    Converted(TNodeId),
}

impl Typer<'_> {
    /// Decides how selection `qual.name` is retried when none of `syms`, its members as seen
    /// from `prefix`, applies to the literal or path arguments of `pt`, as `1.max("a")`: as an
    /// extension call if one in scope takes `qual`, else on `qual` converted to have a member
    /// `name`; `None` otherwise.
    pub fn qualifier_view(
        &mut self,
        qual: TNodeId,
        (prefix, syms): (TypeId, &[SymbolId]),
        name: &str,
        pt: Pt,
        ctx: Ctx,
    ) -> Option<QualifierView> {
        let Pt::Fun(proto) = pt else { return None };
        let args = self.untupled_args(proto, syms).unwrap_or_else(|| self.protos[proto].args.clone());
        let arg_ctx = self.protos[proto].ctx;
        if name == "<init>"
            || self.protos[proto].typed.is_some()
            || !args.iter().all(|&a| is_plain_arg(self.ast, a))
            || auto_tupled(self, (prefix, syms), args.len(), ctx)
        {
            return None;
        }
        let (reported, uniques) = (self.diags.items.len(), self.syms.uniques.clone());
        let ((fits, types), _, _) = self.speculatively(|t| {
            let (mut types, mut typed_args) = (Vec::new(), Vec::new());
            for &a in &args {
                let typed = t.typed(a, Pt::Any, arg_ctx);
                let tpe = t.zonk(t.tpd.tpe(typed));
                let info = t.method_info(tpe).unwrap_or(tpe);
                if matches!(t.u.ty(info), Type::Method { .. } | Type::Poly { .. }) {
                    return (true, types);
                }
                types.push(tpe);
                typed_args.push(typed);
            }
            let adapted = match syms {
                [s] => constants_adapted(t, *s, prefix, &typed_args, &types),
                _ => types.clone(),
            };
            let fits =
                syms.iter().any(|&s| t.applicable(s, Some(prefix), &adapted, true, arg_ctx) != Some(false));
            (fits, types)
        });
        let diagnosed = self.diags.items.len() > reported;
        self.retract_diags(reported);
        if fits {
            self.syms.uniques = uniques;
        }
        if fits || diagnosed {
            return None;
        }
        if self.extension_visible(name, ctx) && !self.extensions_reject(qual, name, ctx) {
            return Some(QualifierView::Extension);
        }
        let reported = self.diags.items.len();
        let (applies, clean, _) = self.speculatively(|t| match t.viewed(qual, Goal::Member(name), ctx) {
            Ok(Some(Viewed::Converted(q))) => Some(member_applies(t, q, name, &types, ctx)),
            Ok(None) => Some(false),
            _ => None,
        });
        self.retract_diags(reported);
        let converted = match applies {
            _ if !clean => None,
            Some(true) => match self.viewed(qual, Goal::Member(name), ctx) {
                Ok(Some(Viewed::Converted(q))) => Some(q),
                _ => None,
            },
            Some(false) => ViewSearch::new(qual, Goal::Member(name), ctx).arg_scope_view(self, name, &types),
            None => None,
        };
        converted.map(QualifierView::Converted)
    }
}

/// Tells whether argument `a` is a literal or a path, or an application of one to such.
pub fn is_plain_arg(ast: &Ast, a: NodeId) -> bool {
    match *ast.tree(a) {
        Tree::Literal { .. } | Tree::Number { .. } | Tree::Ident { .. } => true,
        Tree::Select { qualifier, .. } => is_plain_arg(ast, qualifier),
        Tree::TypeApply { fun, .. } => is_plain_arg(ast, fun),
        Tree::Apply { fun, args } => {
            is_plain_arg(ast, fun) && ast.items(args).iter().all(|&a| is_plain_arg(ast, a))
        }
        _ => false,
    }
}

/// Tells whether `n` arguments are passed to the only one of `syms` as a tuple of their arity.
fn auto_tupled(t: &mut Typer<'_>, (prefix, syms): (TypeId, &[SymbolId]), n: usize, ctx: Ctx) -> bool {
    let &[s] = syms else { return false };
    if n < 2 || !t.is_unary(s) || t.feature_enabled(Features::NO_AUTO_TUPLING, "noAutoTupling", ctx) {
        return false;
    }
    let Some(class) = t.resolve_full_name(&format!("scala.Tuple{n}"), true) else { return true };
    let tuple = crate::defn::applied(t.u, class, vec![tasty::NOTHING; n]);
    let (fits, _, _) = t.speculatively(|t| {
        t.param_types(s, Some(prefix), true).is_none_or(|(params, _)| t.conforms(tuple, params[0]))
    });
    fits
}

/// Gives `types`, those of constant arguments `args` that convert to their parameter of `alt`
/// as that parameter's type (dotty's `adaptConstant` of an argument of a single alternative).
fn constants_adapted(
    t: &mut Typer<'_>,
    alt: SymbolId,
    prefix: TypeId,
    args: &[TNodeId],
    types: &[TypeId],
) -> Vec<TypeId> {
    let params = t.param_types(alt, Some(prefix), true).map_or_else(Vec::new, |(p, _)| p);
    let mut adapted = types.to_vec();
    for ((&a, &p), ty) in args.iter().zip(&params).zip(&mut adapted) {
        if let Some(c) = t.constant_of(a)
            && t.convert_constant(&c, p).is_some()
        {
            *ty = p;
        }
    }
    adapted
}

/// Tells whether a member `name` of `q` applies to arguments of types `args`.
fn member_applies(t: &mut Typer<'_>, q: TNodeId, name: &str, args: &[TypeId], ctx: Ctx) -> bool {
    let tpe = t.tpd.tpe(q);
    let members = t.type_members(tpe, name);
    let (applies, _, _) = t.speculatively(|t| {
        members.iter().any(|&s| t.applicable(s, Some(tpe), args, true, ctx) != Some(false))
    });
    applies
}

impl ViewSearch<'_> {
    /// Converts the tree to have a member `name` by the only conversion of the implicit scopes
    /// of argument types `args` that gives it one (the scope of dotty's `SelectionProto` of a
    /// `FunProto`).
    fn arg_scope_view(&self, t: &mut Typer<'_>, name: &str, args: &[TypeId]) -> Option<TNodeId> {
        let mut candidates = Vec::new();
        for &a in args {
            for c in t.implicit_scope(a).ok()? {
                if !candidates.contains(&c) {
                    candidates.push(c);
                }
            }
        }
        let mut fitting = Vec::new();
        let reported = t.diags.items.len();
        for c in candidates {
            let (fits, clean, _) = t.speculatively(|t| {
                let converted = self.try_conversion(t, c, false);
                converted.is_ok_and(|q| member_applies(t, q, name, args, self.ctx))
            });
            t.retract_diags(reported);
            if fits && clean {
                fitting.push(c);
            }
        }
        let [c] = fitting[..] else { return None };
        self.try_conversion(t, c, false).ok()
    }
}
