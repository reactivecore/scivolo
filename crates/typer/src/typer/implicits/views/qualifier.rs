// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! A view on the qualifier of an application whose method does not apply to the arguments
//! (dotty's `tryWithImplicitOnQualifier`, `tryInsertImplicitOnQualifier`).

use super::super::super::{Pt, Typer};
use super::super::divergence::THROUGH_CONVERSION;
use super::member_fits::member_applies;
use super::{Goal, ViewSearch, Viewed};
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use names::nme;
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
        if name == nme::CONSTRUCTOR
            || self.protos[proto].typed.is_some()
            || !args.iter().all(|&a| is_plain_arg(self.ast, a) && !self.is_sequence_arg(a))
            || auto_tupled(self, (prefix, syms), args.len(), ctx)
        {
            return None;
        }
        let (reported, uniques, counters) =
            (self.diags.items.len(), self.syms.uniques.clone(), self.fresh.counters());
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
            let direct =
                syms.iter().any(|&s| t.applicable(s, Some(prefix), &adapted, false, arg_ctx) == Some(true));
            let fits = direct
                || syms
                    .iter()
                    .any(|&s| t.applicable(s, Some(prefix), &adapted, true, arg_ctx) != Some(false));
            (fits, types)
        });
        let diagnosed = self.diags.items.len() > reported;
        self.retract_diags(reported);
        if fits {
            self.syms.uniques = uniques;
            self.fresh.reset_counters(counters);
        }
        if fits || diagnosed {
            return None;
        }
        if self.extension_visible(name, ctx) && !self.extensions_reject(qual, name, ctx) {
            return Some(QualifierView::Extension);
        }
        let all_args = args == self.protos[proto].args;
        let types = tupled_dual(self, syms, types, ctx);
        let view_proto = self.applied_view_proto(proto, types.clone(), all_args);
        let goal = Goal::Applied(name, &view_proto);
        let reported = self.diags.items.len();
        let (applies, clean, _) = self.speculatively(|t| match t.viewed(qual, goal, ctx) {
            Ok(Some(Viewed::Converted(q))) => Ok(Some(member_applies(t, q, name, &types, ctx))),
            Ok(None) => Ok(Some(false)),
            Err(THROUGH_CONVERSION) => Err(THROUGH_CONVERSION),
            _ => Ok(None),
        });
        self.retract_diags(reported);
        let converted = match applies {
            Err(what) => {
                self.unsupported_at(self.tpd.span(qual), what);
                None
            }
            _ if !clean => None,
            Ok(Some(true)) => match self.viewed(qual, goal, ctx) {
                Ok(Some(Viewed::Converted(q))) => Some(q),
                _ => None,
            },
            Ok(Some(false)) => ViewSearch::new(qual, goal, ctx).arg_scope_view(self, name, &types),
            Ok(None) => None,
        };
        converted.map(QualifierView::Converted)
    }
}

/// Tells whether argument `a` is a literal, a path or an instance creation without a body, an
/// application of one to such, a conditional of such, or, defining no member, a function of
/// declared parameter types or an ascription.
pub fn is_plain_arg(ast: &Ast, a: NodeId) -> bool {
    match *ast.tree(a) {
        Tree::Literal { .. } | Tree::Number { .. } | Tree::Ident { .. } => true,
        Tree::Select { qualifier, .. } | Tree::Parens { t: qualifier } => is_plain_arg(ast, qualifier),
        Tree::Block { stats, expr: Some(expr) } if ast.items(stats).is_empty() => is_plain_arg(ast, expr),
        Tree::If { cond, thenp, elsep: Some(elsep), .. } => {
            [cond, thenp, elsep].iter().all(|&t| is_plain_arg(ast, t))
        }
        Tree::Function { args, body, mods: None, .. } => {
            defines_no_member(ast, body)
                && ast.items(args).iter().all(|&p| {
                    matches!(*ast.tree(p), Tree::ValDef { tpt, .. } if !matches!(ast.tree(tpt), Tree::TypeTree))
                })
        }
        Tree::Typed { expr, .. } => defines_no_member(ast, expr),
        Tree::New { tpt } => !matches!(ast.tree(tpt), Tree::Template { .. }),
        Tree::TypeApply { fun, .. } => is_plain_arg(ast, fun),
        Tree::Apply { fun, args } => {
            is_plain_arg(ast, fun) && ast.items(args).iter().all(|&a| is_plain_arg(ast, a))
        }
        _ => false,
    }
}

/// Tells whether `t` defines no class, object, type or method.
fn defines_no_member(ast: &Ast, t: NodeId) -> bool {
    !ast.exists_subtree(t, &|ast, n| {
        matches!(
            ast.tree(n),
            Tree::Template { .. } | Tree::TypeDef { .. } | Tree::ModuleDef { .. } | Tree::DefDef { .. }
        )
    })
}

/// Tells whether `n` arguments are passed to the only one of `syms` as a tuple of their arity.
fn auto_tupled(t: &mut Typer<'_>, (prefix, syms): (TypeId, &[SymbolId]), n: usize, ctx: Ctx) -> bool {
    let &[s] = syms else { return false };
    if n < 2 || !t.is_unary(s) || t.feature_enabled(Features::NO_AUTO_TUPLING, nme::NO_AUTO_TUPLING, ctx) {
        return false;
    }
    let Some(class) = t.resolve_full_name(&format!("scala.Tuple{n}"), true) else { return true };
    let tuple = crate::defn::applied(t.u, class, vec![tasty::NOTHING; n]);
    let (fits, _, _) = t.speculatively(|t| {
        t.param_types(s, Some(prefix), true).is_none_or(|(params, _)| t.conforms(tuple, params[0]))
    });
    fits
}

/// Gives argument types `types` as the one type of their tuple if the only one of `syms` takes
/// one parameter and auto-tupling is enabled (dotty's `FunProto.tupledDual`).
fn tupled_dual(t: &mut Typer<'_>, syms: &[SymbolId], types: Vec<TypeId>, ctx: Ctx) -> Vec<TypeId> {
    let (&[s], n) = (syms, types.len()) else { return types };
    if n < 2 || !t.is_unary(s) || t.feature_enabled(Features::NO_AUTO_TUPLING, nme::NO_AUTO_TUPLING, ctx) {
        return types;
    }
    match t.resolve_full_name(&format!("scala.Tuple{n}"), true) {
        Some(class) => vec![crate::defn::applied(t.u, class, types)],
        None => types,
    }
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
