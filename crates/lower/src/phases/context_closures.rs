// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/Bridges.scala
//   compiler/src/dotty/tools/dotc/transform/Erasure.scala
//! The closures a bridge of a method flattened by `ContextFunctionResults` returns, one per
//! context clause it takes (dotty's `Bridges.etaExpand` with `Erasure.adaptClosure`).

use super::adapted::{function_adaptation, specialized_interface};
use super::lift_names::fresh;
use crate::definitions::{symbol, type_ref};
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::tags::{ARTIFACT, FINAL, PRIVATE, SYNTHETIC};
use tasty::{Erased, Flags, Kind, MAX_IMPLEMENTED_ARITY, SymbolId, TypeId};
use typer::tpd::{TNodeId, Tree};

/// The parameters of a method, each with its name.
pub type Params = Vec<(String, TypeId)>;

/// A closure's method: its parameters, the erased types of the closure's if they come as one
/// array, how many are the closure's, the specialized interface the closure implements if any,
/// else its `$adapted` method if it needs one, with its parameter names and adaptations.
pub struct Anon {
    pub sym: SymbolId,
    pub params: Params,
    pub bunched: Option<Vec<Erased>>,
    pub arity: usize,
    pub interface: Option<SymbolId>,
    pub adapted: Option<(SymbolId, Vec<String>, (bool, bool))>,
}

/// The closures' methods of `class` for `contexts`, the context clauses of `m` its bridge lacks,
/// each nested in the one before, capturing `captured` of names made from `originals`, the last
/// of erased result `result`.
pub fn plan(
    l: &mut Lower,
    (class, m): (SymbolId, SymbolId),
    contexts: &[Params],
    (mut captured, mut originals): (Params, Vec<String>),
    result: Erased,
    span: Span,
) -> Result<Vec<Anon>> {
    let adapted_flags = Flags::default().with(PRIVATE).with(SYNTHETIC).with(ARTIFACT);
    let flags = Flags::default().with(PRIVATE);
    let flags = if l.u.is_trait(class) { flags } else { flags.with(FINAL) };
    let mut anons: Vec<Anon> = Vec::new();
    for (k, context) in contexts.iter().enumerate() {
        let outer = anons.last().map_or(m, |a| a.sym);
        let prefix = l.u.symbol(outer).name.clone();
        let name = fresh(l, &format!("{prefix}$$anonfun"));
        let sym = l.u.define(symbol(&name, Kind::Def, class, flags));
        if !anons.is_empty() {
            for ((name, _), original) in captured.iter_mut().zip(&originals) {
                *name = fresh(l, original);
            }
        }
        let context: Params = context.iter().filter(|&&(_, p)| !l.u.is_erased_param(p)).cloned().collect();
        let erased: Vec<Erased> = context.iter().map(|&(_, t)| l.u.erase_value(t)).collect();
        let mut params = captured.clone();
        captured.extend(context.iter().cloned());
        originals.extend(context.iter().map(|(name, _)| name.clone()));
        let bunched = context.len() > MAX_IMPLEMENTED_ARITY;
        match bunched {
            true => params.push(super::bunched::param(l)),
            false => params = captured.clone(),
        }
        let result = match contexts.get(k + 1) {
            Some(next) => {
                let arity = next.iter().filter(|&&(_, p)| !l.u.is_erased_param(p)).count();
                let function = function_type(l, arity);
                l.u.erase_value(function)
            }
            None => result.clone(),
        };
        let needed = if bunched { (false, false) } else { function_adaptation(&erased, &result) };
        let Ok(interface) = specialized_interface(l, needed, &erased, &result).transpose() else {
            return unsupported(span, "context functions of this function type");
        };
        let adapted = (interface.is_none() && needed != (false, false)).then(|| {
            let name = fresh(l, &format!("{prefix}$$anonfun$adapted"));
            let adapted = l.u.define(symbol(&name, Kind::Def, class, adapted_flags));
            let first = captured.len() - context.len();
            let names = captured.iter().zip(&originals).enumerate();
            let names = names.map(|(i, ((name, _), o))| if i < first { fresh(l, o) } else { name.clone() });
            (adapted, names.collect(), needed)
        });
        let bunched = bunched.then_some(erased);
        anons.push(Anon { sym, params, bunched, arity: context.len(), interface, adapted });
    }
    Ok(anons)
}

/// Arguments `args` of the parameters of `anon`, its closure's array unpacked.
pub fn unbunched(l: &mut Lower, apply: SymbolId, anon: &Anon, args: &[TNodeId], span: Span) -> Vec<TNodeId> {
    let Some(types) = &anon.bunched else { return args.to_vec() };
    let (all_args, captured) = args.split_last().expect("the array parameter");
    let all_args = l.tpd.sym(*all_args).expect("a parameter reference");
    let elements = super::bunched::elements(l, apply, all_args, types, span);
    captured.iter().copied().chain(elements).collect()
}

/// `closure(anon)` of the function type of `anon`'s arity capturing `args`, `Function1` of one
/// taking an array; in a block, in two if of its `$adapted` method (dotty's `etaExpand` and
/// `adaptClosure`).
pub fn closure(l: &mut Lower, anon: &Anon, args: &[TNodeId], span: Span) -> TNodeId {
    let function = function_type(l, if anon.bunched.is_some() { 1 } else { anon.arity });
    let env = l.tpd.list(args);
    let meth = l.ident(anon.adapted.as_ref().map_or(anon.sym, |a| a.0), span);
    let tpt = anon.interface.map(|i| {
        let interface = type_ref(l.u, i);
        l.type_tree(interface, span)
    });
    let mut expr = l.tpd.node(Tree::Closure { env, meth, tpt }, span, Some(function), None);
    for _ in 0..1 + usize::from(anon.adapted.is_some()) {
        let stats = l.tpd.list(&[]);
        expr = l.tpd.node(Tree::Block { stats, expr }, span, Some(function), None);
    }
    expr
}

/// `FunctionN` of `arity` parameters, `FunctionXXL` beyond the library's arities.
pub fn function_type(l: &mut Lower, arity: usize) -> TypeId {
    match arity > MAX_IMPLEMENTED_ARITY {
        true => l.u.class_ref("scala.runtime.FunctionXXL"),
        false => l.u.class_ref(&format!("scala.Function{arity}")),
    }
}
