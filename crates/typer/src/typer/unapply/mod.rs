// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala

mod alias;
mod extractor;
mod fun;
mod implicits;
mod own_type;
mod product_patterns;
mod proto;
mod seq_patterns;
mod transparent;
mod type_args;

use super::{Pt, Typer};
use crate::ctx::{Ctx, Mode};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{List, NodeId, Tree};
use tasty::TypeId;

/// The written name of a sequence wildcard's type, `x: _*`.
const SEQ_WILDCARD: &str = names::nme::WILDCARD_STAR;

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

        let own = self.pattern_type(t, extractor.method.arg_type, extractor.vars, unapp)?;
        let sel = t.zonk(self.sel);
        let (normal_own, normal_sel) = (t.normalized(own), t.normalized(sel));
        let tested = t.stripped(normal_own) != t.stripped(normal_sel);

        let (result, implicits) = self.applied_extractor(t, &extractor, own)?;
        let patterns = if extractor.seq {
            self.typed_seq_patterns(t, result)?
        } else {
            self.typed_product_patterns(t, result)?
        };

        let (implicits, patterns) = (t.tpd.list(&implicits), t.tpd.list(&patterns));
        let span = t.ast.span(self.id);
        let tree = t.tpd.node(T::UnApply { fun: unapp, implicits, patterns }, span, Some(own), None);
        t.record_inline_extractor(unapp, self.ctx);
        if !tested || t.is_bottom_type(sel) {
            return Ok(tree);
        }
        t.type_tested_unapply(self.id, tree, own, sel, self.ctx)
    }

    /// Types the applied type trees of typed patterns, as `List[Int]` of `(x: List[Int], y)`,
    /// against a tuple scrutinee of their arity once more, the result unused.
    fn retype_tuple_arg_types(&self, t: &mut Typer<'_>) {
        let sel = self.scrutinee;
        let Some((tycon, n)) = t.u.applied(sel).map(|(c, targs)| (c, targs.len())) else { return };
        let tuple = format!("scala.Tuple{n}");
        if n != self.args.len() || t.u.class_symbol(tycon).is_none_or(|c| t.u.full_name(c) != tuple) {
            return;
        }
        for &a in &self.args {
            if let Tree::Typed { tpt, .. } = *t.ast.tree(a)
                && matches!(t.ast.tree(tpt), Tree::AppliedTypeTree { .. })
            {
                t.typed_type(tpt, self.ctx.with_mode(Mode::PATTERN_TYPE));
            }
        }
    }
}

/// Whether pattern `a` is named, `x = p`.
fn is_named_pattern(t: &Typer<'_>, a: NodeId) -> bool {
    matches!(t.ast.tree(a), Tree::NamedArg { .. })
}

/// Whether pattern `a` is a sequence wildcard `x*` or `_*`.
fn is_seq_wildcard(t: &Typer<'_>, a: NodeId) -> bool {
    let Tree::Typed { tpt, .. } = *t.ast.tree(a) else { return false };
    matches!(*t.ast.tree(tpt), Tree::Ident { name } if name.is_type && t.text(name.name) == SEQ_WILDCARD)
}
