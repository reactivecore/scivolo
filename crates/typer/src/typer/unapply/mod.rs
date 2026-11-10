// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala

mod extractor;
mod fun;
mod implicits;
mod named;
mod own_type;
mod proto;
mod seq_patterns;
mod transparent;
mod type_args;

use super::unapply_args::UnapplyArgs;
use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{List, NodeId};
use tasty::TypeId;

/// One extractor pattern `C(p1, ..., pn)` against a value of the scrutinee's type (dotty's
/// `typedUnApply`): `C.unapply`, or `C.unapplySeq` for `C(p, xs*)`, selected on `C`, applied to
/// the type arguments of `C[T](...)` and to its implicit clauses, a transparent inline one
/// expanded. The result's product selectors `_1`, ..., `_n`, the `get` of an `isEmpty`/`get`
/// result, a `Boolean` result or a generic tuple give the patterns their expected types; named
/// patterns `C(x = p)` are placed at the elements they name. Refuses, among others, extension
/// methods as extractors, extractors with further parameter lists and patterns that would
/// constrain invariant type arguments of an open class.
pub struct UnapplyPattern {
    /// The pattern tree.
    id: NodeId,
    /// The argument patterns, named ones placed at the elements they name once those are known.
    args: Vec<NodeId>,
    /// The expected type, or its values tuple if it is a named tuple.
    sel: TypeId,
    /// The expected type instantiated and without the names of a named tuple, on which the
    /// extractor is selected.
    scrutinee: TypeId,
    ctx: Ctx,
}

impl Typer<'_> {
    /// Types extractor pattern `id`, `fun(args)`, against the matched value of type `pt`; refused
    /// if the extractor is outside the subset or an alphanumeric infix one under
    /// `-source future`.
    pub fn typed_unapply(&mut self, id: NodeId, fun: NodeId, args: List, pt: Pt, ctx: Ctx) -> TNodeId {
        match UnapplyPattern::new(self, id, args, pt, ctx).and_then(|p| p.run(self, fun)) {
            Ok(tree) => {
                let unapply = match *self.tpd.tree(tree) {
                    T::UnApply { fun, .. } => self.tpd.sym(self.tpd.meth_part(fun)),
                    _ => None,
                };
                if self.is_invalid_infix_op(id, fun, unapply) {
                    return self.unsupported(id, "alphanumeric infix extractor under -source future");
                }
                tree
            }
            Err(what) => self.unsupported(id, what),
        }
    }
}

impl UnapplyPattern {
    /// Starts pattern `id` with argument patterns `args` against expected type `pt`; refused
    /// without an expected type.
    fn new(t: &mut Typer<'_>, id: NodeId, args: List, pt: Pt, ctx: Ctx) -> Result<Self, &'static str> {
        let Pt::Type(sel) = pt else { return Err("extractor without expected type") };
        let args = t.ast.items(args).to_vec();
        let zonked = t.zonk(sel);
        let scrutinee = t.strip_named_tuple(zonked);
        let sel = if scrutinee == zonked { sel } else { scrutinee };
        Ok(UnapplyPattern { id, args, sel, scrutinee, ctx })
    }

    /// Types the pattern with extractor `fun`: the extractor, the pattern's own type, the
    /// patterns against the extractor's result, then a type test if the own type is narrower
    /// than the expected one.
    fn run(mut self, t: &mut Typer<'_>, fun: NodeId) -> Result<TNodeId, &'static str> {
        self.retype_tuple_arg_types(t);
        let extractor = self.extractor(t, fun)?;
        let unapp = extractor.fun;
        let method = extractor::method_shape(t, unapp)?;

        let own = self.pattern_type(t, method.arg_type, extractor.vars, unapp)?;
        let sel = t.zonk(self.sel);
        let (normal_own, normal_sel) = (t.normalized(own), t.normalized(sel));
        let tested = t.stripped(normal_own) != t.stripped(normal_sel);

        let sym = t.tpd.sym(t.tpd.meth_part(unapp));
        let result = implicits::applied_unapply_result(&mut t.types(), sym, &method, own);
        let (result, implicits) = self.unapply_implicits(t, unapp, result, own)?;
        let result = t.simplified(result);
        let patterns = self.typed_patterns(t, result, extractor.seq)?;

        let (implicits, patterns) = (t.tpd.list(&implicits), t.tpd.list(&patterns));
        let span = t.ast.span(self.id);
        let tree = t.tpd.node(T::UnApply { fun: unapp, implicits, patterns }, span, Some(own), None);
        t.record_inline_extractor(unapp, self.ctx);
        if !tested || t.is_bottom_type(sel) {
            return Ok(tree);
        }
        t.type_tested_unapply(self.id, tree, own, sel, self.ctx)
    }

    /// Types the argument patterns against the types extractor result `result` gives them, as
    /// those of an `unapplySeq` if `seq`.
    fn typed_patterns(
        &mut self,
        t: &mut Typer<'_>,
        result: TypeId,
        seq: bool,
    ) -> Result<Vec<TNodeId>, &'static str> {
        let named = self.args.iter().any(|&a| named::is_named_pattern(t, a));
        if seq {
            if named {
                return Err("named pattern of a sequence extractor");
            }
            let types = UnapplyArgs { result, n: self.args.len() }.seq_arg_types(t)?;
            return self.typed_seq_patterns(t, types);
        }

        let mut values = None;
        if named {
            let names;
            (names, values) = self.named_pattern_elements(t, result)?;
            self.args = super::named_tuple_patterns::adapt_pattern_args(t, &self.args, &names)?;
        }
        let arg_types = match values {
            Some(values) => values,
            None => UnapplyArgs { result, n: self.args.len() }.arg_types(t)?,
        };
        Ok((self.args.iter().zip(arg_types))
            .map(|(&a, tpe)| named::typed_pattern_arg(t, a, tpe, self.ctx))
            .collect())
    }
}
