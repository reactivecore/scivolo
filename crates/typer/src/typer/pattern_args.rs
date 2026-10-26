// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/Desugar.scala
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Argument patterns of extractors before they are typed: the type trees `typedUnApply` types
//! against a tuple selector, named patterns reordered by the case accessors or named tuple
//! elements (dotty's `adaptPatternArgs`).

use super::small_tuples::MAX_TUPLE_ARITY;
use super::{Pt, Typer};
use crate::ctx::{Ctx, Mode};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{List, NodeId, TName, Tree};
use tasty::{Kind, Type, TypeId};

/// The element names named patterns select, with the element types of a named tuple.
type NamedElements = Result<(Vec<String>, Option<Vec<TypeId>>), &'static str>;

impl Typer<'_> {
    /// Types the applied type trees of typed patterns `args` against tuple type `sel` of their
    /// arity once more, the result unused.
    pub fn retype_tuple_arg_types(&mut self, sel: TypeId, args: &[NodeId], ctx: Ctx) {
        let Type::Applied(tycon, targs) = self.u.types().get(sel).clone() else { return };
        let tuple = format!("scala.Tuple{}", targs.len());
        if targs.len() != args.len()
            || self.u.class_symbol(tycon).is_none_or(|c| self.u.full_name(c) != tuple)
        {
            return;
        }
        for &a in args {
            if let Tree::Typed { tpt, .. } = *self.ast.tree(a)
                && matches!(self.ast.tree(tpt), Tree::AppliedTypeTree { .. })
            {
                self.typed_type(tpt, ctx.with_mode(Mode::PATTERN_TYPE));
            }
        }
    }

    /// Whether pattern `a` is named.
    pub fn is_named_pattern(&self, a: NodeId) -> bool {
        matches!(self.ast.tree(a), Tree::NamedArg { .. })
    }

    /// Pattern `a`, possibly named, of expected type `t`.
    pub fn typed_pattern_arg(&mut self, a: NodeId, t: TypeId, ctx: Ctx) -> TNodeId {
        let Tree::NamedArg { name, arg } = *self.ast.tree(a) else { return self.typed(a, Pt::Type(t), ctx) };
        let arg = self.typed(arg, Pt::Type(t), ctx);
        let tpe = self.tpd.tpe(arg);
        self.tpd.node(T::NamedArg { name, arg }, self.ast.span(a), Some(tpe), None)
    }

    /// The element names named patterns `args` of an extractor with result type `result` select,
    /// with the element types if those are a named tuple's (dotty's `productUnapplySelectors`):
    /// those of the result if a named tuple, or a product of at least as many selectors unless
    /// its only selector is a named tuple and a name is no case accessor; else those of its `get`.
    pub fn named_pattern_elements(&mut self, result: TypeId, args: &[NodeId]) -> NamedElements {
        let widened = self.u.widen(result);
        if let Some((names, values)) = self.named_tuple_parts(widened) {
            return Ok((names, Some(values)));
        }
        let product = self.derives_from_named(result, "scala.Product");
        let selectors = if product { self.product_selectors(result)? } else { Vec::new() };
        if product && args.len() <= selectors.len() {
            let names = self.pattern_element_names(result)?;
            let single_named = match selectors[..] {
                [s] => {
                    let widened = self.u.widen(s);
                    self.named_tuple_parts(widened).is_some()
                }
                _ => false,
            };
            if !single_named || args.iter().all(|&a| self.arg_name(a).is_none_or(|n| names.contains(&n))) {
                return Ok((names, None));
            }
        }
        let get = self.get_match(result)?.ok_or("named patterns of an extractor without a product")?;
        let widened = self.u.widen(get);
        if let Some((names, values)) = self.named_tuple_parts(widened) {
            return Ok((names, Some(values)));
        }
        let arity = self.product_selectors(get)?.len();
        let product = self.derives_from_named(get, "scala.Product");
        if !product || self.derives_from_named(get, "scala.collection.Seq") || args.len() > arity {
            return Err("named patterns of an extractor without a product");
        }
        if arity < 2 {
            return Err("named pattern of a `get` with one selector");
        }
        Ok((self.pattern_element_names(get)?, None))
    }

    /// Named tuple pattern `id` of elements `trees`: the patterns placed at the elements of named
    /// tuple `pt` they name, unnamed, as the `TupleN(…)` extractor pattern (dotty's
    /// `desugar.tuple` in pattern mode).
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
        let placed = match self.adapt_pattern_args(&args, &names) {
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
            self.tuple_apply(&values, span.start_pos(), span)
        };
        self.typed(app, pt, ctx)
    }

    /// The element names of named tuple `pt`, else the case accessors of case class `pt`.
    fn pattern_element_names(&mut self, pt: TypeId) -> Result<Vec<String>, &'static str> {
        let widened = self.u.widen(pt);
        if let Some((names, _)) = self.named_tuple_parts(widened) {
            return Ok(names);
        }
        let class = self.u.class_symbol(pt).ok_or("named pattern of a type that is no case class")?;
        let tuple =
            self.u.full_name(class).strip_prefix("scala.Tuple").is_some_and(|n| n.parse::<usize>().is_ok());
        if !self.u.symbol(class).flags.has(tasty::tags::CASE) || tuple {
            return Err("named pattern of a type that is no case class");
        }
        self.complete(class);
        Ok((self.u.decls(class).unwrap_or_default().into_iter())
            .filter(|&d| matches!(self.u.symbol(d).kind, Kind::Val | Kind::Param))
            .filter(|&d| self.u.symbol(d).flags.has(tasty::tags::CASEaccessor))
            .map(|d| self.u.symbol(d).name.clone())
            .collect())
    }

    /// Named patterns `args` placed at the elements `names` they name, a wildcard at the others
    /// (dotty's `adaptPatternArgs`).
    pub fn adapt_pattern_args(
        &mut self,
        args: &[NodeId],
        names: &[String],
    ) -> Result<Vec<NodeId>, &'static str> {
        let mut placed = vec![None; names.len()];
        for &a in args {
            let Some(name) = self.arg_name(a) else { return Err("named and unnamed patterns") };
            let i = names.iter().position(|n| *n == name).ok_or("named pattern of no element")?;
            if placed[i].replace(a).is_some() {
                return Err("duplicate named pattern");
            }
        }
        let span = self.ast.span(args[0]).start_pos();
        let wildcard = TName { name: self.name("_"), is_type: false };
        Ok(placed
            .into_iter()
            .map(|a| {
                a.unwrap_or_else(|| {
                    let w = self.ast.node(Tree::Ident { name: wildcard });
                    self.ast.with_span(w, span);
                    w
                })
            })
            .collect())
    }
}
