// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//   compiler/src/dotty/tools/dotc/transform/PostTyper.scala
//! Named type arguments `f[A = T]` (`language.experimental.namedTypeArguments`): dotty's
//! `typedNamedArgs`, with the type parameters left out as type variables, the arguments in
//! parameter order as `PostTyper.normalizeTypeArgs` leaves them.

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::{NodeId, Tree};
use tasty::{Type, TypeId};

/// Typed type arguments in parameter order, their types, and whether some are type variables.
pub type NamedTypeArgs = (Vec<TNodeId>, Vec<TypeId>, bool);

impl Typer<'_> {
    /// Whether one of type arguments `args` is named.
    pub fn has_named_type_arg(&self, args: &[NodeId]) -> bool {
        args.iter().any(|&a| matches!(self.ast.tree(a), Tree::NamedArg { .. }))
    }

    /// Positional type arguments `typed` with their types.
    pub fn typed_type_args(&mut self, typed: Vec<TNodeId>) -> Result<NamedTypeArgs, &'static str> {
        let types = typed.iter().map(|&a| self.adapted_type_arg(a)).collect::<Result<_, _>>()?;
        Ok((typed, types, false))
    }

    /// Type arguments `args`, all named, of polymorphic `f` in the order of its type parameters;
    /// a type variable for each parameter none names.
    pub fn named_type_args(
        &mut self,
        f: TNodeId,
        args: &[NodeId],
        ctx: Ctx,
    ) -> Result<NamedTypeArgs, &'static str> {
        let fun_tpe = self.tpd.tpe(f);
        let info = self.method_info(fun_tpe).unwrap_or(fun_tpe);
        let Type::Poly { params, .. } = self.u.types().get(info).clone() else {
            return Err("type application of a non-polymorphic method");
        };
        let mut named = Vec::with_capacity(args.len());
        for &a in args {
            let Tree::NamedArg { name, arg } = *self.ast.tree(a) else {
                return Err("named and positional type arguments");
            };
            let text = self.text(name.name).to_owned();
            if !params.iter().any(|(n, _)| *n == text) || named.iter().any(|(n, _)| *n == text) {
                return Err("named type argument without its type parameter");
            }
            named.push((text, arg));
        }
        let span = self.tpd.span(f).to_synthetic();
        let (mut trees, mut types, mut fresh) = (Vec::new(), Vec::new(), false);
        for (param, _) in &params {
            let (tree, tpe) = match named.iter().find(|(n, _)| n == param) {
                Some(&(_, arg)) => {
                    let tree = self.typed_type(arg, ctx);
                    (tree, self.adapted_type_arg(tree)?)
                }
                None => {
                    fresh = true;
                    let var = self.constraint.fresh(self.u);
                    (self.type_tree(var, span, true), var)
                }
            };
            trees.push(tree);
            types.push(tpe);
        }
        Ok((trees, types, fresh))
    }
}
