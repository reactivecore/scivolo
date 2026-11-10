// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/TreeInfo.scala
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Alternatives narrowed by the shapes of function arguments without parameter types (dotty's
//! `typeShape`, `normArg`, `narrowByShapes`).

use super::applicability::{applicable, rigid_param_types};
use super::{MODIFIED_FUNCTION, Resolution, UNDECIDED};
use crate::typer::Typer;
use syntax::ast::{Ast, NodeId, Tree};
use tasty::{ANY, NOTHING, SymbolId, Type, TypeId};

impl Resolution {
    /// Narrows `alts` to those that apply to the shapes of the arguments of `proto` if one is a
    /// function without parameter types, as `x => x` has the shape `Any => Nothing`, else keeps
    /// them all (dotty's `narrowByShapes`).
    pub fn narrow_by_shapes(
        self,
        t: &mut Typer,
        alts: &[SymbolId],
        proto: usize,
        views: bool,
    ) -> Result<Vec<SymbolId>, &'static str> {
        let (args, ctx) = (t.protos[proto].args.clone(), t.protos[proto].ctx);
        if !args.iter().any(|&a| unknown_param_function(t, a).is_some()) {
            return Ok(alts.to_vec());
        }
        if args.iter().any(|&a| matches!(t.ast.tree(a), Tree::NamedArg { .. })) {
            return Err("overloads with named and function arguments");
        }

        let mut shapes = Vec::new();
        for (i, &a) in args.iter().enumerate() {
            let a = unblocked_all(t.ast, a);
            let tupled = self.tupled_arg(t, a, i, alts)?;
            shapes.push(type_shape(t, a, tupled)?);
        }

        let mut narrowed = Vec::new();
        for &alt in alts {
            if applicable(t, alt, self.prefix, &shapes, views, ctx).ok_or(UNDECIDED)? {
                narrowed.push(alt);
            }
        }
        Ok(narrowed)
    }

    /// Checks whether function argument `arg` at position `i` takes its parameters as one tuple:
    /// no alternative of `alts` takes a function of as many parameters there and one takes a
    /// function of a tuple of that many (dotty's `normArg` with `ptIsCorrectProduct`), as
    /// `(a, b) => a + b` against `((Int, Int)) => Int`.
    fn tupled_arg(
        self,
        t: &mut Typer,
        arg: NodeId,
        i: usize,
        alts: &[SymbolId],
    ) -> Result<bool, &'static str> {
        let Tree::Function { args, .. } = *t.ast.tree(arg) else { return Ok(false) };
        let params = t.ast.items(args).to_vec();
        if params.len() < 2 {
            return Ok(false);
        }

        let mut unary = Vec::new();
        for &alt in alts {
            let ps = formal_at(t, alt, i, self.prefix).map_or_else(Vec::new, |f| t.function_params(f));
            if ps.len() == params.len() {
                return Ok(false);
            }
            if let [p] = ps[..] {
                unary.push((alt, p));
            }
        }

        for (alt, p) in unary {
            let p = t.zonk(p);
            if matches!(*t.u.types().get(p), Type::Sym(_, s) if t.u.symbol(s).owner == Some(alt)) {
                return Err("tupled function argument against a type parameter");
            }
            if params.iter().any(|&q| !t.is_untyped_param(q)) {
                return Err("tupled function argument with parameter types");
            }
            let p = t.fully_defined(p)?;
            let tpe = t.u.dealias(p);
            if let Some((tycon, n)) = t.u.applied(tpe).map(|(c, elems)| (c, elems.len()))
                && n == params.len()
                && t.u.class_symbol(tycon).is_some_and(|c| t.u.full_name(c) == format!("scala.Tuple{n}"))
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

/// Finds the function value or case lambda that argument `arg` is if a parameter of it has no
/// type (dotty's `functionWithUnknownParamType`): `x => x + 1` or `{ case 1 => "one" }`, also
/// named or in braces.
pub fn unknown_param_function(t: &Typer, arg: NodeId) -> Option<NodeId> {
    match *t.ast.tree(arg) {
        Tree::Function { args, .. } => {
            t.ast.items(args).iter().any(|&p| t.is_untyped_param(p)).then_some(arg)
        }
        Tree::Match { selector: None, .. } => Some(arg),
        Tree::NamedArg { arg, .. } => unknown_param_function(t, arg),
        _ => unblocked(t.ast, arg).and_then(|e| unknown_param_function(t, e)),
    }
}

/// Finds the expression of block `arg` of no statements.
fn unblocked(ast: &Ast, arg: NodeId) -> Option<NodeId> {
    match *ast.tree(arg) {
        Tree::Block { stats, expr: Some(e) } if ast.items(stats).is_empty() => Some(e),
        _ => None,
    }
}

/// Strips `arg` of enclosing blocks of no statements (dotty's `normArg` of `{ expr }`).
pub fn unblocked_all(ast: &Ast, mut arg: NodeId) -> NodeId {
    while let Some(e) = unblocked(ast, arg) {
        arg = e;
    }
    arg
}

/// Gives the shape of argument `arg` as a type, a function of several parameters taking one
/// tuple if `tupled` (dotty's `typeShape`).
fn type_shape(t: &mut Typer, arg: NodeId, tupled: bool) -> Result<TypeId, &'static str> {
    match *t.ast.tree(arg) {
        Tree::Function { mods: Some(_), .. } => Err(MODIFIED_FUNCTION),
        Tree::Function { args, body, .. } => {
            let n = if tupled { 1 } else { t.ast.items(args).len() };
            let mut parts = vec![ANY; n];
            parts.push(type_shape(t, body, false)?);
            t.function_of(parts)
        }
        Tree::Match { selector: None, .. } => t.partial_function_of(ANY, NOTHING),
        _ => Ok(NOTHING),
    }
}

/// Finds the parameter type of the first clause of alternative `alt` at position `i`, its type
/// parameters rigid; none for a repeated parameter (dotty's `firstParamTypes`).
pub fn formal_at(t: &mut Typer, alt: SymbolId, i: usize, prefix: Option<TypeId>) -> Option<TypeId> {
    let (formals, repeated) = rigid_param_types(t, alt, prefix)?;
    if repeated && i + 1 >= formals.len() {
        return None;
    }
    formals.get(i).copied()
}
