// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Extractors `C[Ts](p1, ..., pn)` whose `C.unapply` takes the type arguments `Ts` (dotty's
//! `tryWithTypeArgs` in `trySelectUnapply`).

use super::super::Typer;
use super::UnapplyPattern;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{List, NodeId, Span, Tree};
use tasty::{Type, TypeId};

impl UnapplyPattern {
    /// Splits `C[Ts]` into the extractor `C` and its type arguments `Ts` if `C`'s extractor
    /// method, selected speculatively, has as many type parameters as `Ts` are.
    pub fn unapply_type_args(&self, t: &mut Typer<'_>, fun: NodeId) -> Option<(NodeId, List)> {
        let Tree::TypeApply { fun: qual, args } = *t.ast.tree(fun) else { return None };
        let n = t.ast.items(args).len();
        let reported = t.diags.items.len();
        let (found, clean, _) = t.speculatively(|t| {
            let (unapp, _) = self.unapply_fun(t, qual).ok()?;
            let tpe = t.tpd.tpe(unapp);
            let info = t.method_info(tpe).unwrap_or(tpe);
            Some(matches!(t.u.types().get(info), Type::Poly { params, .. } if params.len() == n))
        });
        t.retract_diags(reported);
        (clean && found == Some(true)).then_some((qual, args))
    }

    /// Applies extractor method `unapp` to type arguments `args` of type application `id`, at
    /// the span from `unapp` to the last argument.
    pub fn with_unapply_type_args(
        &self,
        t: &mut Typer<'_>,
        id: NodeId,
        unapp: TNodeId,
        args: List,
    ) -> Result<TNodeId, &'static str> {
        let arg_ids = t.ast.items(args).to_vec();
        let typed: Vec<TNodeId> = arg_ids.iter().map(|&a| t.typed_type(a, self.ctx)).collect();
        let types: Vec<TypeId> = typed.iter().map(|&a| t.adapted_type_arg(a)).collect::<Result<_, _>>()?;
        let tpe = t.instantiated(unapp, &types)?;
        let end = typed.last().map_or(t.ast.span(id).end(), |&a| t.tpd.span(a).end());
        let span = Span::synthetic(t.tpd.span(unapp).start(), end);
        let args = t.tpd.list(&typed);
        Ok(t.tpd.node(T::TypeApply { fun: unapp, args }, span, Some(tpe), None))
    }
}
