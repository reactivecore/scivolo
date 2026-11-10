// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! The argument patterns of an `unapply`, named ones placed at the elements they name (dotty's
//! `typedUnApply` and `productUnapplySelectors`).

use super::super::extractor_members::{PRODUCT, SEQ, get_match, product_selectors};
use super::super::named_tuple_patterns::adapt_pattern_args;
use super::super::unapply_args::UnapplyArgs;
use super::super::{Pt, Typer};
use super::{UnapplyPattern, is_named_pattern};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, Tree};
use tasty::{Kind, TypeId};

/// The element names named patterns select, with the element types of a named tuple.
type NamedElements = Result<(Vec<String>, Option<Vec<TypeId>>), &'static str>;

impl UnapplyPattern {
    /// Types the argument patterns of an `unapply` with result type `result` against the types it
    /// gives them; named patterns `C(x = p)` first placed at the elements they name, the others
    /// wildcards. Refuses names of no element and a number of patterns the result has no types
    /// for.
    pub fn typed_product_patterns(
        &mut self,
        t: &mut Typer<'_>,
        result: TypeId,
    ) -> Result<Vec<TNodeId>, &'static str> {
        let mut values = None;
        if self.args.iter().any(|&a| is_named_pattern(t, a)) {
            let names;
            (names, values) = named_pattern_elements(t, &self.args, result)?;
            self.args = adapt_pattern_args(t, &self.args, &names)?;
        }

        let arg_types = match values {
            Some(values) => values,
            None => UnapplyArgs { result, n: self.args.len() }.arg_types(t)?,
        };
        Ok((self.args.iter().zip(arg_types))
            .map(|(&a, tpe)| typed_pattern_arg(t, a, tpe, self.ctx))
            .collect())
    }
}

/// Finds the element names the named argument patterns `args` of an extractor with result type
/// `result` select, with the element types if those are a named tuple's (dotty's
/// `productUnapplySelectors`): those of the result if a named tuple, or a product of at least as
/// many selectors unless its only selector is a named tuple and a name is no case accessor; else
/// those of its `get`. Refuses names of a sequence, of a `get` without a product or with one
/// selector.
fn named_pattern_elements(t: &mut Typer<'_>, args: &[NodeId], result: TypeId) -> NamedElements {
    let widened = t.u.widen(result);
    if let Some((names, values)) = t.named_tuple_parts(widened) {
        return Ok((names, Some(values)));
    }

    let product = t.derives_from_named(result, PRODUCT);
    let selectors = if product { product_selectors(t, result)? } else { Vec::new() };
    if product && args.len() <= selectors.len() {
        let names = pattern_element_names(t, result)?;
        let single_named = match selectors[..] {
            [s] => {
                let widened = t.u.widen(s);
                t.named_tuple_parts(widened).is_some()
            }
            _ => false,
        };
        if !single_named || args.iter().all(|&a| t.arg_name(a).is_none_or(|n| names.contains(&n))) {
            return Ok((names, None));
        }
    }

    let get = get_match(t, result)?.ok_or("named patterns of an extractor without a product")?;
    let widened = t.u.widen(get);
    if let Some((names, values)) = t.named_tuple_parts(widened) {
        return Ok((names, Some(values)));
    }
    let arity = product_selectors(t, get)?.len();
    let product = t.derives_from_named(get, PRODUCT);
    if !product || t.derives_from_named(get, SEQ) || args.len() > arity {
        return Err("named patterns of an extractor without a product");
    }
    if arity < 2 {
        return Err("named pattern of a `get` with one selector");
    }
    Ok((pattern_element_names(t, get)?, None))
}

/// Types pattern `a`, possibly named, against expected type `tpe`.
fn typed_pattern_arg(t: &mut Typer<'_>, a: NodeId, tpe: TypeId, ctx: Ctx) -> TNodeId {
    let Tree::NamedArg { name, arg } = *t.ast.tree(a) else { return t.typed(a, Pt::Type(tpe), ctx) };
    let arg = t.typed(arg, Pt::Type(tpe), ctx);
    let tpe = t.tpd.tpe(arg);
    t.tpd.node(T::NamedArg { name, arg }, t.ast.span(a), Some(tpe), None)
}

/// The element names of named tuple `pt`, else the case accessors of case class `pt`.
fn pattern_element_names(t: &mut Typer<'_>, pt: TypeId) -> Result<Vec<String>, &'static str> {
    let widened = t.u.widen(pt);
    if let Some((names, _)) = t.named_tuple_parts(widened) {
        return Ok(names);
    }
    let class = t.u.class_symbol(pt).ok_or("named pattern of a type that is no case class")?;
    let tuple = t.u.full_name(class).strip_prefix("scala.Tuple").is_some_and(|n| n.parse::<usize>().is_ok());
    if !t.u.symbol(class).is_case() || tuple {
        return Err("named pattern of a type that is no case class");
    }
    t.complete(class);
    Ok((t.u.decls(class).unwrap_or_default().into_iter())
        .filter(|&d| matches!(t.u.symbol(d).kind, Kind::Val | Kind::Param))
        .filter(|&d| t.u.symbol(d).is_case_accessor())
        .map(|d| t.u.symbol(d).name.clone())
        .collect())
}
