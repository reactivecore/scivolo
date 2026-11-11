// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/Erasure.scala
//! The closures of lifted lambdas: the functional interface a closure implements (a specialized
//! `JFunctionN`, a `JProcedureN`, else its function type), its `$adapted` method where parameters
//! or result erase differently from its abstract method's (dotty's `Erasure.adaptClosure`), and
//! the block becoming `closure(captured, method)`.

use super::adapt::from_start;
use super::adapted::{adapted_types, specialized_interface};
use super::function_classes::{erased_function_class, function_class_arity};
use super::lambda_lift::Function;
use super::lift_names::fresh;
use super::lifted::{Method, define};
use super::narrowed::{enclosing_instance, home};
use super::poly_functions::poly_function_class;
use super::sam::{abstract_methods, adaptation_needed};
use crate::definitions::type_ref;
use crate::{Lower, Result, unsupported};
use names::name_kinds;
use tasty::{Erased, MAX_IMPLEMENTED_ARITY, SymbolId, TypeId};
use typer::tpd::{TNodeId, Tree};

/// How a lambda's closure is made: the interface it implements if not its function type, its
/// `$adapted` method if any.
pub struct Plan {
    pub interface: Option<SymbolId>,
    pub adapted: Option<Method>,
}

/// The plan for the closure of lambda `f` with parameters `params` and result `result`, its
/// lifted name prefix `prefix`.
pub fn plan(
    l: &mut Lower,
    class: SymbolId,
    f: &Function,
    prefix: &str,
    params: &[(SymbolId, TypeId)],
    result: TypeId,
) -> Result<Plan> {
    let closure = f.closure.expect("a lambda's closure");
    let span = l.tpd.span(f.block);
    let Tree::Closure { tpt, .. } = *l.tpd.tree(closure) else { unreachable!("a closure") };
    let tpe = tpt.map_or(l.tpd.tpe(closure), |t| l.tpd.tpe(t));
    let (unrefined, _) = l.u.without_refinements(tpe);
    let declared = l.u.class_symbol(unrefined);
    let xxl = declared
        .and_then(|c| function_class_arity(l, c))
        .is_some_and(|(n, _)| n as usize > MAX_IMPLEMENTED_ARITY);
    let function = tpt.is_none() && !xxl;
    let interface =
        poly_function_class(l, tpe).or_else(|| declared.and_then(|c| erased_function_class(l, c)));
    let erased_context = interface.filter(|&c| Some(c) != declared);
    let Some(sam) = interface.and_then(|c| abstract_methods(l, c).first().copied()) else {
        return unsupported(span, "closures of a type without an abstract method");
    };
    let (sam_params, sam_result) = l.u.erased_method_value(sam);
    let erased: Vec<Erased> = params.iter().map(|&(_, t)| l.u.erase_value(t).boxing_unit()).collect();
    let erased_result = l.u.erase_value(result);
    let needed = adaptation_needed(&erased, &erased_result, &sam_params, &sam_result);
    let specialized_interface =
        function.then(|| specialized_interface(l, needed, &erased, &erased_result)).flatten();
    let Ok(specialized_interface) = specialized_interface.transpose() else {
        return unsupported(span, "lambdas of this function type");
    };
    let adapted = (specialized_interface.is_none() && needed != (false, false)).then(|| {
        let (types, result) = adapted_types(l, sam, needed, params, result);
        let name = fresh(l, &name_kinds::adapted_closure(&name_kinds::anon_fun(prefix)));
        let adapted = define(l, home(l, class, f), &name, &f.captured, &types, result, f.instance());
        l.bridges.insert(adapted.sym);
        if let Some(owner) = l.u.symbol(f.sym).owner {
            l.lifted_from.insert(adapted.sym, owner);
        }
        adapted
    });
    Ok(Plan { interface: specialized_interface.or(erased_context), adapted })
}

/// Makes the block of lambda `f` of definition `td` of `class` the closure of `method`, or of its
/// `$adapted` method, selected on the enclosing instance it is narrowed into.
pub fn rewrite(
    l: &mut Lower,
    (td, class): (TNodeId, SymbolId),
    f: &Function,
    method: &Method,
    plan: &Plan,
) -> Result<()> {
    let closure = f.closure.expect("a lambda's closure");
    let span = l.tpd.span(closure);
    let env: Vec<TNodeId> = f.captured.iter().map(|&s| l.ident(s, from_start(span))).collect();
    let target = plan.adapted.as_ref().unwrap_or(method).sym;
    let local = |c: SymbolId| f.parent.is_none() && c != class && !l.u.is_static_module(c);
    let meth = match f.into.filter(|&c| enclosing_instance(l, class, c) || local(c)) {
        Some(c) => {
            let receiver = super::outer_fields::outer_path(l, (td, class), c, !f.ctor, from_start(span))?;
            l.select(receiver, target, span)
        }
        None => l.ident(target, span),
    };
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
    Ok(())
}
