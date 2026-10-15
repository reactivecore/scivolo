//! Parameters put first in a method or constructor: the captured values of lifted local classes
//! and trait methods, enclosing instances.

use crate::Lower;
use tasty::{SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

/// Puts `params` first in the first parameter clause of constructor `constr` of `ctor`.
pub fn add_params(l: &mut Lower, constr: TNodeId, ctor: SymbolId, params: &[SymbolId]) {
    let span = l.tpd.span(constr);
    let param_defs: Vec<TNodeId> = params.iter().map(|&p| l.param_def(p, span)).collect();
    let Tree::DefDef { name, paramss, tpt, rhs } = *l.tpd.tree(constr) else { unreachable!("a constructor") };
    let mut clauses = l.tpd.clause_items(paramss).to_vec();
    match clauses.first_mut() {
        Some(first) => {
            let items: Vec<TNodeId> = param_defs.iter().chain(l.tpd.items(*first)).copied().collect();
            *first = l.tpd.list(&items);
        }
        None => clauses.push(l.tpd.list(&param_defs)),
    }
    let paramss = l.tpd.clauses(&clauses);
    l.tpd.get_mut(constr).tree = Tree::DefDef { name, paramss, tpt, rhs };
    let info = l.u.info(ctor);
    let info = with_leading_params(l, info, params);
    l.u.set_info(ctor, info);
}

/// Method type `info`, also under type parameters, with `params` first in its first clause; of a
/// method without parameter lists, a method type of `params`.
fn with_leading_params(l: &mut Lower, info: tasty::TypeId, params: &[SymbolId]) -> tasty::TypeId {
    match l.u.types().get(info).clone() {
        Type::Poly { binder, params: tparams, result } => {
            let result = with_leading_params(l, result, params);
            l.u.intern_type(Type::Poly { binder, params: tparams, result })
        }
        Type::Method { binder, params: old, result, implicit, contextual } => {
            let new = params.iter().map(|&p| (l.u.symbol(p).name.clone(), l.u.info(p)));
            let params = new.chain(old).collect();
            l.u.intern_type(Type::Method { binder, params, result, implicit, contextual })
        }
        other => {
            let result = if let Type::ByName(r) = other { r } else { info };
            let params = params.iter().map(|&p| (l.u.symbol(p).name.clone(), l.u.info(p))).collect();
            let binder = l.u.fresh_binder();
            l.u.intern_type(Type::Method { binder, params, result, implicit: false, contextual: false })
        }
    }
}
