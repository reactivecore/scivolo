// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/Desugar.scala
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Named patterns placed at the elements they name: named tuple patterns `(x = p, y = q)` and
//! those of extractors (dotty's `adaptPatternArgs`).

use super::small_tuples::MAX_TUPLE_ARITY;
use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::{List, NodeId, TName, Tree};

impl Typer<'_> {
    /// Types named tuple pattern `id` of elements `trees`, as `case (name = n, age = a) =>`: the
    /// patterns placed at the elements of named tuple `pt` they name, unnamed, as the
    /// `TupleN(…)` extractor pattern (dotty's `desugar.tuple` in pattern mode). Refused against
    /// a type that is no named tuple.
    pub fn typed_named_tuple_pattern(&mut self, id: NodeId, trees: List, pt: Pt, ctx: Ctx) -> TNodeId {
        let names = match pt {
            Pt::Type(t) => {
                let zonked = self.zonk(t);
                let widened = self.u.widen(zonked);
                self.named_tuple_parts(widened).map(|(names, _)| names)
            }
            _ => None,
        };
        let Some(names) = names else {
            return self.unsupported(id, "named pattern of a type that is no named tuple");
        };
        let args = self.ast.items(trees).to_vec();
        let placed = match adapt_pattern_args(self, &args, &names) {
            Ok(placed) => placed,
            Err(what) => return self.unsupported(id, what),
        };

        let values: Vec<NodeId> = (placed.into_iter())
            .map(|a| match *self.ast.tree(a) {
                Tree::NamedArg { arg, .. } => arg,
                _ => a,
            })
            .collect();
        let span = self.ast.span(id);
        let app = if values.len() > MAX_TUPLE_ARITY {
            let trees = self.ast.list(&values);
            let tuple = self.ast.node(Tree::Tuple { trees });
            self.ast.with_span(tuple, span);
            tuple
        } else {
            let at = values.first().map_or(span, |&v| self.ast.span(v)).start_pos();
            self.tuple_apply(&values, at, span)
        };
        self.typed(app, pt, ctx)
    }
}

/// Places named patterns `args` at the elements `names` they name, a wildcard at the others
/// (dotty's `adaptPatternArgs`). Refuses unnamed patterns among them, names of no element and
/// an element named twice.
pub fn adapt_pattern_args(
    t: &mut Typer<'_>,
    args: &[NodeId],
    names: &[String],
) -> Result<Vec<NodeId>, &'static str> {
    let mut placed = vec![None; names.len()];
    for &a in args {
        let Some(name) = t.arg_name(a) else { return Err("named and unnamed patterns") };
        let i = names.iter().position(|n| *n == name).ok_or("named pattern of no element")?;
        if placed[i].replace(a).is_some() {
            return Err("duplicate named pattern");
        }
    }

    let span = t.ast.span(args[0]).start_pos();
    let wildcard = TName { name: t.name("_"), is_type: false };
    Ok(placed
        .into_iter()
        .map(|a| {
            a.unwrap_or_else(|| {
                let w = t.ast.node(Tree::Ident { name: wildcard });
                t.ast.with_span(w, span);
                w
            })
        })
        .collect())
}
