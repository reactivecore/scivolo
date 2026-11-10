// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/AccessProxies.scala
//! The parameter, result and type argument trees of inline accessors.

use super::super::Typer;
use crate::tpd::{TNodeId, Tpd, Tree as T};
use syntax::ast::flags::{self as f, FlagSet};
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::{Binder, NameId, Type, TypeId, Universe};

/// A parameter clause of an accessed method: its binder, parameters and their flags.
pub struct Clause {
    pub binder: Option<Binder>,
    pub params: Vec<(NameId, TypeId)>,
    pub flags: FlagSet,
}

/// Makes the parameter and result type trees of typed accessor definition `def` and the type
/// arguments of its forwarding call type trees of their types (dotty's `tpd.DefDef`).
pub fn with_type_trees(tpd: &mut Tpd, def: TNodeId) {
    let T::DefDef { paramss, tpt, rhs, .. } = *tpd.tree(def) else { return };
    let mut tpts = vec![tpt];
    for &clause in tpd.clause_items(paramss) {
        for &p in tpd.items(clause) {
            match *tpd.tree(p) {
                T::ValDef { tpt, .. } => tpts.push(tpt),
                T::TypeDef { rhs, .. } => tpts.push(rhs),
                _ => {}
            }
        }
    }
    let mut call = rhs;
    while let Some(c) = call {
        call = match *tpd.tree(c) {
            T::Apply { fun, .. } | T::Assign { lhs: fun, .. } => Some(fun),
            T::TypeApply { fun, args } => {
                tpts.extend_from_slice(tpd.items(args));
                Some(fun)
            }
            _ => None,
        };
    }
    for c in tpts {
        let node = tpd.get_mut(c);
        (node.tree, node.sym) = (T::TypeTree { inferred: false }, None);
    }
}

/// Makes the type tree of `tpe` in an accessor whose parameter clauses so far are `clauses`:
/// `tpe` spliced, with its references to their parameters by name; `None`, reported at `span`,
/// for a type it cannot rebuild around them, as a refinement over a parameter.
pub fn accessor_tpt(t: &mut Typer<'_>, tpe: TypeId, clauses: &[Clause], span: Span) -> Option<NodeId> {
    if !refers_to_params(t.u, tpe, clauses) {
        return Some(t.splice_type(tpe));
    }
    let tree = match t.u.ty(tpe).clone() {
        Type::Param(..) => {
            let (name, reference) = param_ref(t, tpe, clauses)?;
            return Some(match name.is_type {
                true => reference,
                false => t.ast.node(Tree::SingletonTypeTree { reference }),
            });
        }
        Type::Applied(tycon, args) => {
            let tpt = accessor_tpt(t, tycon, clauses, span)?;
            let args = args.iter().map(|&a| accessor_tpt(t, a, clauses, span)).collect::<Option<Vec<_>>>()?;
            let args = t.ast.list(&args);
            Tree::AppliedTypeTree { tpt, args }
        }
        Type::Sym(Some(p), s) if t.u.is_type_symbol(s) => {
            let qualifier = path(t, p, clauses, span)?;
            let name = TName { name: t.name(&t.u.symbol(s).name.clone()), is_type: true };
            Tree::Select { qualifier, name }
        }
        Type::TypeRef(p, n) => {
            let qualifier = path(t, p, clauses, span)?;
            let n = t.u.name(n).to_owned();
            Tree::Select { qualifier, name: TName { name: t.name(&n), is_type: true } }
        }
        Type::Sym(Some(_), _) | Type::TermRef(..) => {
            let reference = path(t, tpe, clauses, span)?;
            Tree::SingletonTypeTree { reference }
        }
        Type::Bounds(lo, hi) => {
            let lo = Some(accessor_tpt(t, lo, clauses, span)?);
            let hi = Some(accessor_tpt(t, hi, clauses, span)?);
            Tree::TypeBoundsTree { lo, hi, alias: None }
        }
        Type::ByName(r) => Tree::ByNameTypeTree { result: accessor_tpt(t, r, clauses, span)? },
        _ => return unsupported(t, span),
    };
    Some(t.ast.node(tree))
}

/// The path tree of term reference `p` in an accessor with parameter clauses `clauses`.
fn path(t: &mut Typer<'_>, p: TypeId, clauses: &[Clause], span: Span) -> Option<NodeId> {
    let (qualifier, name) = match t.u.ty(p).clone() {
        Type::Param(..) => return param_ref(t, p, clauses).map(|(_, r)| r),
        Type::Sym(Some(q), s) if !t.u.is_type_symbol(s) => (q, t.u.symbol(s).name.clone()),
        Type::TermRef(q, n) => (q, t.u.name(n).to_owned()),
        _ => return unsupported(t, span),
    };
    let qualifier = path(t, qualifier, clauses, span)?;
    let name = TName { name: t.name(&name), is_type: false };
    Some(t.ast.node(Tree::Select { qualifier, name }))
}

/// The name of parameter reference `p` to one of `clauses` and an identifier of it.
fn param_ref(t: &mut Typer<'_>, p: TypeId, clauses: &[Clause]) -> Option<(TName, NodeId)> {
    let (b, i) = t.u.param_ref(p)?;
    let clause = clauses.iter().find(|c| c.binder == Some(b))?;
    let is_type = clause.flags.is(f::TYPE_PARAM);
    let name = t.u.name(clause.params[i as usize].0).to_owned();
    let name = TName { name: t.name(&name), is_type };
    Some((name, t.ast.node(Tree::Ident { name })))
}

fn unsupported(t: &mut Typer<'_>, span: Span) -> Option<NodeId> {
    t.unsupported_at(span, "inline accessor of a method with a type built of its parameters");
    None
}

/// Whether `tpe` refers to a parameter of `clauses`.
fn refers_to_params(u: &mut Universe, tpe: TypeId, clauses: &[Clause]) -> bool {
    let mut refers = false;
    u.map_type(tpe, &mut |u, part| {
        refers |= u.param_ref(part).is_some_and(|(b, _)| clauses.iter().any(|c| c.binder == Some(b)));
        None
    });
    refers
}
