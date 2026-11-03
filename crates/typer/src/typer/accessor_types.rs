// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/AccessProxies.scala
//! The parameter, result and type argument trees of inline accessors.

use super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::flags::{self as f, FlagSet};
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::{Binder, Type, TypeId};

/// A parameter clause of an accessed method: its binder, parameters and their flags.
pub struct Clause {
    pub binder: Option<Binder>,
    pub params: Vec<(String, TypeId)>,
    pub flags: FlagSet,
}

impl Typer<'_> {
    /// Makes the parameter and result type trees of typed accessor definition `def` and the type
    /// arguments of its forwarding call type trees of their types (dotty's `tpd.DefDef`).
    pub fn with_type_trees(&mut self, def: TNodeId) {
        let T::DefDef { paramss, tpt, rhs, .. } = *self.tpd.tree(def) else { return };
        let mut tpts = vec![tpt];
        for &clause in self.tpd.clause_items(paramss) {
            for &p in self.tpd.items(clause) {
                match *self.tpd.tree(p) {
                    T::ValDef { tpt, .. } => tpts.push(tpt),
                    T::TypeDef { rhs, .. } => tpts.push(rhs),
                    _ => {}
                }
            }
        }
        let mut call = rhs;
        while let Some(t) = call {
            call = match *self.tpd.tree(t) {
                T::Apply { fun, .. } | T::Assign { lhs: fun, .. } => Some(fun),
                T::TypeApply { fun, args } => {
                    tpts.extend_from_slice(self.tpd.items(args));
                    Some(fun)
                }
                _ => None,
            };
        }
        for t in tpts {
            let node = self.tpd.get_mut(t);
            (node.tree, node.sym) = (T::TypeTree { inferred: false }, None);
        }
    }

    /// The type tree of `t` in an accessor whose parameter clauses so far are `clauses`: `t`
    /// spliced, with its references to their parameters by name; `None`, reported at `span`, for
    /// a type it cannot rebuild around them.
    pub fn accessor_tpt(&mut self, t: TypeId, clauses: &[Clause], span: Span) -> Option<NodeId> {
        if !self.refers_to_params(t, clauses) {
            return Some(self.splice_type(t));
        }
        let tree = match self.u.types().get(t).clone() {
            Type::Param(..) => {
                let (name, reference) = self.accessor_param_ref(t, clauses)?;
                return Some(match name.is_type {
                    true => reference,
                    false => self.ast.node(Tree::SingletonTypeTree { reference }),
                });
            }
            Type::Applied(tycon, args) => {
                let tpt = self.accessor_tpt(tycon, clauses, span)?;
                let args =
                    args.iter().map(|&a| self.accessor_tpt(a, clauses, span)).collect::<Option<Vec<_>>>()?;
                let args = self.ast.list(&args);
                Tree::AppliedTypeTree { tpt, args }
            }
            Type::Sym(Some(p), s) if self.u.is_type_symbol(s) => {
                let qualifier = self.accessor_path(p, clauses, span)?;
                let name = TName { name: self.name(&self.u.symbol(s).name.clone()), is_type: true };
                Tree::Select { qualifier, name }
            }
            Type::TypeRef(p, n) => {
                let qualifier = self.accessor_path(p, clauses, span)?;
                Tree::Select { qualifier, name: TName { name: self.name(&n), is_type: true } }
            }
            Type::Sym(Some(_), _) | Type::TermRef(..) => {
                let reference = self.accessor_path(t, clauses, span)?;
                Tree::SingletonTypeTree { reference }
            }
            Type::Bounds(lo, hi) => {
                let lo = Some(self.accessor_tpt(lo, clauses, span)?);
                let hi = Some(self.accessor_tpt(hi, clauses, span)?);
                Tree::TypeBoundsTree { lo, hi, alias: None }
            }
            Type::ByName(r) => Tree::ByNameTypeTree { result: self.accessor_tpt(r, clauses, span)? },
            _ => return self.accessor_unsupported(span),
        };
        Some(self.ast.node(tree))
    }

    /// The path tree of term reference `t` in an accessor with parameter clauses `clauses`.
    fn accessor_path(&mut self, t: TypeId, clauses: &[Clause], span: Span) -> Option<NodeId> {
        let (qualifier, name) = match self.u.types().get(t).clone() {
            Type::Param(..) => return self.accessor_param_ref(t, clauses).map(|(_, r)| r),
            Type::Sym(Some(p), s) if !self.u.is_type_symbol(s) => (p, self.u.symbol(s).name.clone()),
            Type::TermRef(p, n) => (p, n),
            _ => return self.accessor_unsupported(span),
        };
        let qualifier = self.accessor_path(qualifier, clauses, span)?;
        let name = TName { name: self.name(&name), is_type: false };
        Some(self.ast.node(Tree::Select { qualifier, name }))
    }

    /// The name of parameter reference `t` to one of `clauses` and an identifier of it.
    fn accessor_param_ref(&mut self, t: TypeId, clauses: &[Clause]) -> Option<(TName, NodeId)> {
        let Type::Param(b, i) = *self.u.types().get(t) else { return None };
        let clause = clauses.iter().find(|c| c.binder == Some(b))?;
        let is_type = clause.flags.is(f::TYPE_PARAM);
        let name = TName { name: self.name(&clause.params[i as usize].0), is_type };
        Some((name, self.ast.node(Tree::Ident { name })))
    }

    fn accessor_unsupported(&mut self, span: Span) -> Option<NodeId> {
        self.unsupported_at(span, "inline accessor of a method with a type built of its parameters");
        None
    }

    /// Whether `t` refers to a parameter of `clauses`.
    fn refers_to_params(&mut self, t: TypeId, clauses: &[Clause]) -> bool {
        let mut refers = false;
        self.u.map_type(t, &mut |u, part| {
            refers |= matches!(*u.types().get(part), Type::Param(b, _) if clauses.iter().any(|c| c.binder == Some(b)));
            None
        });
        refers
    }
}
