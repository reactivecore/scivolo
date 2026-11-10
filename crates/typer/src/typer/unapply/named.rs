// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Argument patterns of extractors: the type trees `typedUnApply` types against a tuple
//! selector, named patterns and the elements they name (dotty's `productUnapplySelectors`).

use super::super::extractor_members::{SEQ, get_match, product_selectors};
use super::super::{Pt, Typer};
use super::UnapplyPattern;
use crate::ctx::{Ctx, Mode};
use crate::defn::PRODUCT;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, Tree};
use tasty::{Kind, TypeId};

/// The element names named patterns select, with the element types of a named tuple.
type NamedElements = Result<(Vec<String>, Option<Vec<TypeId>>), &'static str>;

impl UnapplyPattern {
    /// Types the applied type trees of typed patterns, as `List[Int]` of `(x: List[Int], y)`,
    /// against a tuple scrutinee of their arity once more, the result unused.
    pub fn retype_tuple_arg_types(&self, t: &mut Typer<'_>) {
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

    /// The element names the named argument patterns of an extractor with result type `result`
    /// select, with the element types if those are a named tuple's (dotty's
    /// `productUnapplySelectors`): those of the result if a named tuple, or a product of at least
    /// as many selectors unless its only selector is a named tuple and a name is no case
    /// accessor; else those of its `get`. Refuses names of a sequence, of a `get` without a
    /// product or with one selector.
    pub fn named_pattern_elements(&self, t: &mut Typer<'_>, result: TypeId) -> NamedElements {
        let args = &self.args;
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
}

/// Whether pattern `a` is named, `x = p`.
pub fn is_named_pattern(t: &Typer<'_>, a: NodeId) -> bool {
    matches!(t.ast.tree(a), Tree::NamedArg { .. })
}

/// Types pattern `a`, possibly named, against expected type `tpe`.
pub fn typed_pattern_arg(t: &mut Typer<'_>, a: NodeId, tpe: TypeId, ctx: Ctx) -> TNodeId {
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
