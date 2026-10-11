//! The closures of lifted lambdas: the functional interface a closure implements (a specialized
//! `JFunctionN`, a `JProcedureN`, else its function type), its `$adapted` method where a parameter
//! is primitive or the result `Unit` (dotty's `Erasure.adaptClosure`), and the block becoming
//! `closure(captured, method)`.

use super::adapt::from_start;
use super::adapted::specializable;
use super::lambda_lift::{Function, fresh};
use super::lifted::{Method, define};
use crate::definitions::{procedure, specialized, type_ref};
use crate::{Lower, Result, unsupported};
use tasty::{Erased, SymbolId, TypeId};
use typer::tpd::{TNodeId, Tree};

/// How a lambda's closure is made: the interface it implements if not its function type, its
/// `$adapted` method if any.
pub struct Plan {
    pub interface: Option<SymbolId>,
    pub adapted: Option<Method>,
}

/// The plan for the closure of lambda `f` with parameters `params` and result `result`, its
/// lifted name prefix `prefix`; the `$adapted` method's proxies to name last go to `deferred`.
pub fn plan(
    l: &mut Lower,
    class: SymbolId,
    f: &Function,
    prefix: &str,
    params: &[(SymbolId, TypeId)],
    result: TypeId,
    deferred: &mut Vec<SymbolId>,
) -> Result<Plan> {
    let closure = f.closure.expect("a lambda's closure");
    let erased: Vec<Erased> = params
        .iter()
        .map(|&(_, t)| match l.u.erase(t) {
            Erased::Unit => Erased::Class("scala.runtime.BoxedUnit".into()),
            e => e,
        })
        .collect();
    let erased_result = l.u.erase(result);
    let unit = erased_result == Erased::Unit;
    let primitive = erased.iter().any(|e| matches!(e, Erased::Prim(_) | Erased::Unit));
    let function = matches!(l.tpd.tree(closure), Tree::Closure { tpt: None, .. });
    let specialized_ = function && (unit || primitive) && specializable(&erased, &erased_result);
    let interface = match () {
        _ if specialized_ => Some(specialized(l.u, &erased, &erased_result)),
        _ if function && unit && !primitive => Some(procedure(l.u, params.len())),
        _ => None,
    };
    let Ok(interface) = interface.transpose() else {
        return unsupported(l.tpd.span(f.block), "lambdas of this function type");
    };
    let adapted = (!specialized_ && (primitive || unit && !function)).then(|| {
        let object = type_ref(l.u, l.defs.object);
        let objects: Vec<(SymbolId, TypeId)> = params.iter().map(|&(p, _)| (p, object)).collect();
        let result = if unit { object } else { result };
        let name = fresh(l, &format!("{prefix}$anonfun$adapted"));
        let adapted = define(l, class, &name, &f.captured, &objects, result, f.this);
        l.bridges.insert(adapted.sym);
        deferred.extend(adapted.proxies.iter().copied());
        adapted
    });
    Ok(Plan { interface, adapted })
}

/// Makes the block of lambda `f` the closure of `method`, or of its `$adapted` method.
pub fn rewrite(l: &mut Lower, f: &Function, method: &Method, plan: &Plan) {
    let closure = f.closure.expect("a lambda's closure");
    let span = l.tpd.span(closure);
    let env: Vec<TNodeId> = f.captured.iter().map(|&s| l.ident(s, span)).collect();
    let target = plan.adapted.as_ref().unwrap_or(method).sym;
    let meth = l.ident(target, span);
    let Tree::Closure { tpt, .. } = *l.tpd.tree(closure) else { unreachable!("a closure") };
    let tpt = match plan.interface {
        Some(interface) => {
            let interface = type_ref(l.u, interface);
            Some(l.type_tree(interface, span))
        }
        None => tpt,
    };
    let env = l.tpd.list(&env);
    l.tpd.get_mut(closure).tree = Tree::Closure { env, meth, tpt };
    let stats = l.tpd.list(&[]);
    let expr = match plan.adapted {
        Some(_) => {
            let node = l.tpd.get_mut(closure);
            let span = node.span;
            node.span = from_start(span);
            l.block(&[], closure, span)
        }
        None => closure,
    };
    l.tpd.get_mut(f.block).tree = Tree::Block { stats, expr };
}
