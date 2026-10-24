// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/Bridges.scala
//! Bridges of methods flattened by `ContextFunctionResults` overriding methods flattened by
//! fewer context clauses: the bridge returns closures taking the rest (dotty's `etaExpand`).

use super::lift_names::fresh;
use crate::definitions::symbol;
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashMap;
use syntax::ast::Span;
use tasty::tags::{ARTIFACT, CASEaccessor, FIELDaccessor, LAZY, OBJECT, PARAMsetter, PRIVATE, SYNTHETIC};
use tasty::{Flags, Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// The parameters of a method, each with its name.
type Params = Vec<(String, TypeId)>;

/// `def m(params): F = closure(m$$anonfun$N)` of the type of `other`, flattened by fewer context
/// clauses than `m`, and the closures' methods, one per clause `other` lacks, each nested in the
/// one before: `def m$$anonfun$N(params, contextParams) = this.m(params, contextParams)` for the
/// last, in `class` at `span`.
pub fn bridge(
    l: &mut Lower,
    class: SymbolId,
    m: SymbolId,
    other: SymbolId,
    span: Span,
) -> Result<Vec<TNodeId>> {
    let (_, depth) = l.context_results[&m];
    let flattened = l.context_results.get(&other).map_or(0, |&(_, d)| d);
    let (m_info, other_info) = (l.u.info(m), l.u.info(other));
    let own_clauses = clauses(l, m_info);
    let own = own_clauses.len() - depth + flattened;
    if depth <= flattened || clauses(l, other_info).len() != own {
        return unsupported(span, "bridges of methods with context function results flattened otherwise");
    }
    let m_result = l.u.result_type(m_info);
    let mut captured: Vec<(String, TypeId)> = own_clauses[..own].concat();
    let own_params = captured.len();
    for (name, _) in &mut captured {
        *name = fresh(l, name);
    }
    let flags = Flags::default().with(PRIVATE).with(SYNTHETIC).with(ARTIFACT);
    let mut anons: Vec<(SymbolId, Params, usize)> = Vec::new();
    for context in &own_clauses[own..] {
        let outer = anons.last().map_or(m, |&(a, ..)| a);
        let anon_name = fresh(l, &format!("{}$$anonfun", l.u.symbol(outer).name));
        let anon = l.u.define(symbol(&anon_name, Kind::Def, class, flags));
        if !anons.is_empty() {
            for (name, _) in &mut captured[own_params..] {
                *name = fresh(l, name);
            }
        }
        captured.extend(context.iter().cloned());
        anons.push((anon, captured.clone(), context.len()));
    }
    let mut defs = Vec::new();
    let mut result = m_result;
    for (i, (anon, params, _)) in anons.iter().enumerate().rev() {
        let binder = l.u.fresh_binder();
        let params = params.clone();
        let info =
            l.u.intern_type(Type::Method { binder, params, result, implicit: false, contextual: false });
        l.u.set_info(*anon, info);
        let inner = anons.get(i + 1).map(|&(a, _, arity)| (a, arity));
        defs.push(l.forwarding_def(*anon, span, |l, args, result| match inner {
            Some((a, arity)) => closure(l, a, arity, args, span),
            None => {
                let this = l.this(class, span);
                let fun = l.select(this, m, span);
                l.apply(fun, args, result, span)
            }
        }));
        result = function_type(l, anons[i].2);
    }
    let mut flags = l.u.symbol(m).flags;
    for tag in [FIELDaccessor, PARAMsetter, CASEaccessor, LAZY, OBJECT] {
        flags = flags.without(tag);
    }
    let (first, arity) = (anons[0].0, anons[0].2);
    let (sym, def) =
        l.method_like(class, m, flags, other_info, span, |l, args, _| closure(l, first, arity, args, span))?;
    l.bridges.insert(sym);
    Ok([def].into_iter().chain(defs).collect())
}

/// `closure(anon)` of a function of `arity` parameters capturing `args`.
fn closure(l: &mut Lower, anon: SymbolId, arity: usize, args: &[TNodeId], span: Span) -> TNodeId {
    let function = function_type(l, arity);
    let env = l.tpd.list(args);
    let meth = l.ident(anon, span);
    l.tpd.node(Tree::Closure { env, meth, tpt: None }, span, Some(function), None)
}

fn function_type(l: &mut Lower, arity: usize) -> TypeId {
    l.u.class_ref(&format!("scala.Function{arity}"))
}

/// Whether every member of the name of method `m` that `m` may override, or that may override
/// `m`, is a method of the unit flattened by as many closures, one of the unit flattened by fewer
/// that the more flattened one overrides, or an abstract one of the unit's a base class declares,
/// bridged: those of the base classes of its class and of the unit's classes deriving from it,
/// and of `Object` and `Any`; `counts` holds the flattened methods of the unit, `defs` all.
pub fn overrides_alike(
    l: &mut Lower,
    m: SymbolId,
    defs: &[SymbolId],
    counts: &FxHashMap<SymbolId, (TNodeId, usize)>,
) -> bool {
    let Some(owner) = l.u.symbol(m).owner.filter(|&o| l.u.symbol(o).kind == Kind::Class) else {
        return true;
    };
    let count = counts.get(&m).map_or(0, |&(_, c)| c);
    let name = l.u.symbol(m).name.clone();
    let mut classes = vec![owner];
    for &d in defs {
        let o = l.u.symbol(d).owner.filter(|&o| l.u.symbol(o).kind == Kind::Class);
        if let Some(o) = o.filter(|&o| !classes.contains(&o) && l.u.derives_from(o, owner)) {
            classes.push(o);
        }
    }
    let mut related: Vec<SymbolId> = Vec::new();
    for class in classes {
        for base in l.u.linearization(class) {
            related.extend(l.u.decl(base, &name).unwrap_or_default());
        }
    }
    let roots = [("java.lang", "Object"), ("scala", "Any")];
    let alike = |l: &mut Lower, d: SymbolId| {
        let c = counts.get(&d).map(|&(_, c)| c);
        if c == Some(count) || c.is_none() && bridged(l, owner, d) {
            return true;
        }
        let (c, o) = (c.unwrap_or(0), l.u.symbol(d).owner.expect("a member's class"));
        let below = c < count && l.u.derives_from(owner, o);
        defs.contains(&d) && o != owner && (below || c > count && l.u.derives_from(o, owner))
    };
    related.into_iter().all(|d| d == m || alike(l, d))
        && roots.into_iter().all(|(p, c)| !root_declares(l, p, c, &name))
}

/// Whether class `class` of package `package` declares a member `name`, or cannot be found.
fn root_declares(l: &mut Lower, package: &str, class: &str, name: &str) -> bool {
    let Some(pkg) = l.u.package(package) else { return true };
    let found = l.u.decl(pkg, class).unwrap_or_default();
    let classes: Vec<SymbolId> = found.into_iter().filter(|&c| l.u.symbol(c).kind == Kind::Class).collect();
    classes.is_empty() || classes.into_iter().any(|c| l.u.decl(c, name).map_or(true, |d| !d.is_empty()))
}

/// Whether a method of class `owner` flattened by `ContextFunctionResults` gets a bridge
/// overriding `d` (were it not flattened): an abstract method of the unit `owner` inherits.
pub fn bridged(l: &mut Lower, owner: SymbolId, d: SymbolId) -> bool {
    let deferred = l.u.is_deferred(d) && l.u.symbol(d).tree.is_none();
    deferred && l.u.symbol(d).owner.is_some_and(|o| o != owner && l.u.derives_from(owner, o))
}

/// The parameter clauses of method type `t`, each parameter with its name.
pub fn clauses(l: &Lower, mut t: TypeId) -> Vec<Vec<(String, TypeId)>> {
    let mut out = Vec::new();
    loop {
        match l.u.types().get(t) {
            Type::Poly { result, .. } => t = *result,
            Type::Method { params, result, .. } => {
                out.push(params.clone());
                t = *result;
            }
            _ => return out,
        }
    }
}
