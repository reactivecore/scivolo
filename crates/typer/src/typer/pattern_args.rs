// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/Desugar.scala
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Argument patterns of extractors before they are typed: the type trees `typedUnApply` types
//! against a tuple selector, named patterns reordered by the case accessors (dotty's
//! `adaptPatternArgs`).

use super::{Pt, Typer};
use crate::ctx::{Ctx, Mode};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, TName, Tree};
use tasty::{Kind, Type, TypeId};

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

    /// The type whose case accessors `n` named patterns of an extractor with result type `result`
    /// name: the result if a product of at least `n` selectors, else its `get` if one of more
    /// than one.
    pub fn named_patterns_type(&mut self, result: TypeId, n: usize) -> Result<TypeId, &'static str> {
        if self.derives_from_named(result, "scala.Product") && n <= self.product_selectors(result)?.len() {
            return Ok(result);
        }
        let get = self.get_match(result)?.ok_or("named patterns of an extractor without a product")?;
        let arity = self.product_selectors(get)?.len();
        let product = self.derives_from_named(get, "scala.Product");
        if !product || self.derives_from_named(get, "scala.collection.Seq") || n > arity {
            return Err("named patterns of an extractor without a product");
        }
        if arity < 2 {
            return Err("named pattern of a `get` with one selector");
        }
        Ok(get)
    }

    /// Named patterns `args` placed at the case accessors of case class type `pt` they name, a
    /// wildcard at the others.
    pub fn adapt_pattern_args(&mut self, args: &[NodeId], pt: TypeId) -> Result<Vec<NodeId>, &'static str> {
        let class = self.u.class_symbol(pt).ok_or("named pattern of a type that is no case class")?;
        let tuple =
            self.u.full_name(class).strip_prefix("scala.Tuple").is_some_and(|n| n.parse::<usize>().is_ok());
        if !self.u.symbol(class).flags.has(tasty::tags::CASE) || tuple {
            return Err("named pattern of a type that is no case class");
        }
        self.complete(class);
        let accessors: Vec<String> = (self.u.decls(class).unwrap_or_default().into_iter())
            .filter(|&d| matches!(self.u.symbol(d).kind, Kind::Val | Kind::Param))
            .filter(|&d| self.u.symbol(d).flags.has(tasty::tags::CASEaccessor))
            .map(|d| self.u.symbol(d).name.clone())
            .collect();
        let mut placed = vec![None; accessors.len()];
        for &a in args {
            let Tree::NamedArg { name, .. } = *self.ast.tree(a) else {
                return Err("named and unnamed patterns");
            };
            let i = accessors
                .iter()
                .position(|n| n == self.text(name.name))
                .ok_or("named pattern of no element")?;
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
