// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/PostTyper.scala
//! The inferred function type of a definition whose right-hand side is a closure, its parameter
//! types with capture sets marked `@caps.internal.declared` (dotty's `explicifyTpt` cases 2 and 3,
//! `makeFormalsDeclared`).

use super::inferred_captures::caps_internal_class;
use super::retaining::emptied;
use super::{Refusal, TreePickler};
use crate::tpd::{TNodeId, Tree};
use crate::typer::capture_types::has_captures;
use crate::typer::seen_from::member_of;
use names::nme;
use tasty::defn::DECLARED;
use tasty::{SymbolId, Type, TypeId};

impl TreePickler<'_> {
    /// Records the type of inferred type tree `tpt` of definition `sym`, not overriding, with
    /// closure `rhs` in a capture-checked file: `(A^) @declared -> Int` of `val f = (x: A^) => 1`,
    /// also for the closures in the results of curried ones (`(x: A^) => (y: A^{x}) => 1`), but
    /// for other anonymous functions.
    pub fn declare_closure_formals(
        &mut self,
        sym: SymbolId,
        tpt: TNodeId,
        rhs: Option<TNodeId>,
    ) -> Result<(), Refusal> {
        let inferred = matches!(self.tpd.tree(tpt), Tree::TypeTree { inferred: true });
        if !self.capture_checked() || !inferred || self.kept_captures.contains_key(&tpt) {
            return Ok(());
        }
        let (Some(rhs), Some(tpe)) = (rhs, self.tpd.get(tpt).tpe) else { return Ok(()) };
        if self.closure_method(rhs).is_none() {
            return Ok(());
        }
        if self.u.symbol(sym).name == nme::ANON_FUN && !self.curried_closures.remove(&sym) {
            return Ok(());
        }
        let declared = self.formals_declared(tpe, rhs)?;
        if declared != tpe {
            let cleaned = emptied(self.u, declared);
            self.kept_captures.insert(tpt, cleaned);
            self.tpt_spans.insert(tpt, self.def_span);
        }
        Ok(())
    }

    /// The method of closure `t`, a block of its definition and the closure (dotty's `closureDef`).
    fn closure_method(&self, t: TNodeId) -> Option<TNodeId> {
        let Tree::Block { stats, expr } = *self.tpd.tree(t) else { return None };
        match (self.tpd.items(stats), self.tpd.tree(expr)) {
            (&[], _) => self.closure_method(expr),
            (&[stat], Tree::Closure { .. }) => {
                matches!(self.tpd.tree(stat), Tree::DefDef { .. }).then_some(stat)
            }
            _ => None,
        }
    }

    /// Function type `t` of closure `rhs` with its parameter types marked, those of the closures
    /// its body ends in too (dotty's `makeFormalsDeclared`).
    fn formals_declared(&mut self, t: TypeId, rhs: TNodeId) -> Result<TypeId, Refusal> {
        let Some(method) = self.closure_method(rhs) else { return Ok(t) };
        if let Some(sym) = self.tpd.sym(method) {
            self.curried_closures.insert(sym);
        }
        let Tree::DefDef { rhs: Some(body), .. } = *self.tpd.tree(method) else { return Ok(t) };
        if let Some((tycon, args)) = self.function_args(t) {
            return self.declared_function(tycon, &args, (rhs, body));
        }
        match *self.u.ty(t) {
            Type::Refined(parent, name, info) => {
                let parent = match (self.function_args(parent), self.u.ty(info)) {
                    (Some((tycon, args)), Type::Method { .. }) => {
                        self.declared_function(tycon, &args, (rhs, body))?
                    }
                    _ => parent,
                };
                let info = self.formals_declared(info, rhs)?;
                Ok(self.u.intern_type(Type::Refined(parent, name, info)))
            }
            Type::Poly { binder, params, result } => {
                let params = self.u.params_of(params).to_vec();
                let declared: Vec<_> =
                    params.into_iter().map(|(n, b)| (n, self.bounds_declared(b, rhs))).collect();
                let result = self.formals_declared(result, rhs)?;
                Ok(self.u.intern_poly(binder, &declared, result))
            }
            Type::Method { binder, params, result, implicit, contextual } => {
                let params = self.u.params_of(params).to_vec();
                let declared: Vec<_> =
                    params.into_iter().map(|(n, f)| (n, self.formal_declared(f, rhs))).collect();
                let result = self.formals_declared(result, body)?;
                Ok(self.u.intern_method(binder, &declared, result, implicit, contextual))
            }
            _ => Ok(t),
        }
    }

    /// Type parameter bounds `b` of closure `rhs`, each bound with a capture set marked.
    fn bounds_declared(&mut self, b: TypeId, rhs: TNodeId) -> TypeId {
        let Type::Bounds(lo, hi) = *self.u.ty(b) else { return b };
        let (lo, hi) = (self.formal_declared(lo, rhs), self.formal_declared(hi, rhs));
        self.u.intern_type(Type::Bounds(lo, hi))
    }

    /// `tycon[args]` of closure `rhs` of body `body`, its parameters marked and its result, the
    /// last of `args`, as `formals_declared` gives it.
    fn declared_function(
        &mut self,
        tycon: TypeId,
        args: &[TypeId],
        (rhs, body): (TNodeId, TNodeId),
    ) -> Result<TypeId, Refusal> {
        let Some((&result, formals)) = args.split_last() else { return Ok(tycon) };
        let mut declared: Vec<TypeId> = formals.iter().map(|&f| self.formal_declared(f, rhs)).collect();
        declared.push(self.formals_declared(result, body)?);
        Ok(self.u.intern_applied(tycon, &declared))
    }

    /// The constructor and arguments of `t` if it is a function type of package `scala`.
    fn function_args(&mut self, t: TypeId) -> Option<(TypeId, Vec<TypeId>)> {
        let (tycon, args) = self.u.applied(t).map(|(c, a)| (c, a.to_vec()))?;
        let function =
            |n: &str| n.trim_start_matches("Impure").trim_start_matches("Context").starts_with("Function");
        let sym = self.u.sym_of(tycon)?;
        member_of(self.u, sym, "scala", function).then_some((tycon, args))
    }

    /// `formal @declared`, the annotation at closure `rhs`, if `formal` has a capture set and no
    /// such mark (dotty's `makeFormalDeclared`).
    fn formal_declared(&mut self, formal: TypeId, rhs: TNodeId) -> TypeId {
        let marked = self.u.annotated(formal).is_some_and(|(_, a)| self.u.is_class_named(a, DECLARED));
        if marked || !has_captures(self.u, formal) {
            return formal;
        }
        let Some(class) = caps_internal_class(self.u, "declared") else { return formal };
        let declared = self.u.intern_type(Type::Annotated(formal, class));
        self.declared_spans.insert(declared, self.tpd.span(rhs));
        declared
    }
}
