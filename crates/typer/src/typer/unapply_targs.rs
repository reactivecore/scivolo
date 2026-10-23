// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Extractors `C[Ts](p1, ..., pn)` whose `C.unapply` takes the type arguments `Ts` (dotty's
//! `tryWithTypeArgs` in `trySelectUnapply`).

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{List, NodeId, Span, Tree};
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// The extractor and type arguments of `C[Ts]` if `C`'s extractor method has as many type
    /// parameters as `Ts` are, for a value of type `sel`.
    pub fn unapply_type_args(&mut self, fun: NodeId, sel: TypeId, ctx: Ctx) -> Option<(NodeId, List)> {
        let Tree::TypeApply { fun: qual, args } = *self.ast.tree(fun) else { return None };
        let n = self.ast.items(args).len();
        let reported = self.diags.items.len();
        let (found, clean, _) = self.speculatively(|t| {
            let (unapp, _) = t.unapply_fun(qual, sel, ctx).ok()?;
            let tpe = t.tpd.tpe(unapp);
            let info = t.method_info(tpe).unwrap_or(tpe);
            Some(matches!(t.u.types().get(info), Type::Poly { params, .. } if params.len() == n))
        });
        self.diags.items.truncate(reported);
        (clean && found == Some(true)).then_some((qual, args))
    }

    /// Extractor method `unapp` applied to type arguments `args` of type application `id`, at
    /// the span from `unapp` to the last argument.
    pub fn with_unapply_type_args(
        &mut self,
        id: NodeId,
        unapp: TNodeId,
        args: List,
        ctx: Ctx,
    ) -> Result<TNodeId, &'static str> {
        let arg_ids = self.ast.items(args).to_vec();
        let typed: Vec<TNodeId> = arg_ids.iter().map(|&a| self.typed_type(a, ctx)).collect();
        let types: Vec<TypeId> = typed.iter().map(|&a| self.adapted_type_arg(a)).collect::<Result<_, _>>()?;
        let tpe = self.instantiated(unapp, &types)?;
        let end = typed.last().map_or(self.ast.span(id).end(), |&a| self.tpd.span(a).end());
        let span = Span::synthetic(self.tpd.span(unapp).start(), end);
        let args = self.tpd.list(&typed);
        Ok(self.tpd.node(T::TypeApply { fun: unapp, args }, span, Some(tpe), None))
    }
}
