// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/TreeInfo.scala
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Alternatives narrowed by the shapes of function arguments without parameter types (dotty's
//! `typeShape`, `normArg`, `narrowByShapes`).

use super::Typer;
use syntax::ast::{NodeId, Tree};
use tasty::{ANY, NOTHING, SymbolId, Type, TypeId};

const UNDECIDED: &str = "overload resolution with implicit conversions";

impl Typer<'_> {
    /// The function value or case lambda that argument `arg` is if a parameter of it has no type
    /// (dotty's `functionWithUnknownParamType`).
    pub fn unknown_param_function(&self, arg: NodeId) -> Option<NodeId> {
        match *self.ast.tree(arg) {
            Tree::Function { args, .. } => {
                self.ast.items(args).iter().any(|&p| self.is_untyped_param(p)).then_some(arg)
            }
            Tree::Match { selector: None, .. } => Some(arg),
            Tree::NamedArg { arg, .. } => self.unknown_param_function(arg),
            _ => self.unblocked(arg).and_then(|e| self.unknown_param_function(e)),
        }
    }

    /// The expression of block `arg` of no statements.
    fn unblocked(&self, arg: NodeId) -> Option<NodeId> {
        match *self.ast.tree(arg) {
            Tree::Block { stats, expr: Some(e) } if self.ast.items(stats).is_empty() => Some(e),
            _ => None,
        }
    }

    /// `arg` without enclosing blocks of no statements (dotty's `normArg` of `{ expr }`).
    pub fn unblocked_all(&self, mut arg: NodeId) -> NodeId {
        while let Some(e) = self.unblocked(arg) {
            arg = e;
        }
        arg
    }

    /// The alternatives of `alts`, members of `prefix`, that apply to the shapes of the arguments
    /// of `proto` if one is a function without parameter types, else `alts` (dotty's
    /// `narrowByShapes`).
    pub fn narrow_by_shapes(
        &mut self,
        alts: &[SymbolId],
        prefix: Option<TypeId>,
        proto: usize,
        views: bool,
    ) -> Result<Vec<SymbolId>, &'static str> {
        let (args, ctx) = (self.protos[proto].args.clone(), self.protos[proto].ctx);
        if !args.iter().any(|&a| self.unknown_param_function(a).is_some()) {
            return Ok(alts.to_vec());
        }
        if args.iter().any(|&a| matches!(self.ast.tree(a), Tree::NamedArg { .. })) {
            return Err("overloads with named and function arguments");
        }
        let mut shapes = Vec::new();
        for (i, &a) in args.iter().enumerate() {
            let a = self.unblocked_all(a);
            let tupled = self.tupled_arg(a, i, alts, prefix)?;
            shapes.push(self.type_shape(a, tupled)?);
        }
        let mut narrowed = Vec::new();
        for &alt in alts {
            if self.applicable(alt, prefix, &shapes, views, ctx).ok_or(UNDECIDED)? {
                narrowed.push(alt);
            }
        }
        Ok(narrowed)
    }

    /// The shape of argument `arg` as a type, a function of several parameters taking one tuple
    /// if `tupled` (dotty's `typeShape`).
    fn type_shape(&mut self, arg: NodeId, tupled: bool) -> Result<TypeId, &'static str> {
        match *self.ast.tree(arg) {
            Tree::Function { mods: Some(_), .. } => Err("function argument with modifiers of an overload"),
            Tree::Function { args, body, .. } => {
                let n = if tupled { 1 } else { self.ast.items(args).len() };
                let mut parts = vec![ANY; n];
                parts.push(self.type_shape(body, false)?);
                self.function_of(parts)
            }
            Tree::Match { selector: None, .. } => self.partial_function_of(ANY, NOTHING),
            _ => Ok(NOTHING),
        }
    }

    /// Whether function argument `arg` at position `i` takes its parameters as one tuple: no
    /// alternative of `alts` takes a function of as many parameters there and one takes a
    /// function of a tuple of that many (dotty's `normArg` with `ptIsCorrectProduct`).
    fn tupled_arg(
        &mut self,
        arg: NodeId,
        i: usize,
        alts: &[SymbolId],
        prefix: Option<TypeId>,
    ) -> Result<bool, &'static str> {
        let Tree::Function { args, .. } = *self.ast.tree(arg) else { return Ok(false) };
        let params = self.ast.items(args).to_vec();
        if params.len() < 2 {
            return Ok(false);
        }
        let mut unary = Vec::new();
        for &alt in alts {
            let ps = self.formal_at(alt, i, prefix).map_or_else(Vec::new, |f| self.function_params(f));
            if ps.len() == params.len() {
                return Ok(false);
            }
            if let [p] = ps[..] {
                unary.push((alt, p));
            }
        }
        for (alt, p) in unary {
            let p = self.zonk(p);
            if matches!(*self.u.types().get(p), Type::Sym(_, s) if self.u.symbol(s).owner == Some(alt)) {
                return Err("tupled function argument against a type parameter");
            }
            if params.iter().any(|&q| !self.is_untyped_param(q)) {
                return Err("tupled function argument with parameter types");
            }
            let p = self.fully_defined(p)?;
            let t = self.u.dealias(p);
            if let Some((tycon, n)) = self.u.applied(t).map(|(c, elems)| (c, elems.len()))
                && n == params.len()
                && self
                    .u
                    .class_symbol(tycon)
                    .is_some_and(|c| self.u.full_name(c) == format!("scala.Tuple{n}"))
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// The parameter type of the first clause of alternative `alt` at position `i`, its type
    /// parameters rigid; none for a repeated parameter (dotty's `firstParamTypes`).
    pub fn formal_at(&mut self, alt: SymbolId, i: usize, prefix: Option<TypeId>) -> Option<TypeId> {
        let (formals, repeated) = self.rigid_param_types(alt, prefix)?;
        if repeated && i + 1 >= formals.len() {
            return None;
        }
        formals.get(i).copied()
    }
}
