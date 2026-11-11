// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/Bridges.scala
//! Bridges of methods flattened by `ContextFunctionResults` overriding methods flattened by
//! fewer context clauses: the bridge returns closures taking the rest (dotty's `etaExpand`).

use super::adapt::adapt;
use super::adapted::erased_adapted_def;
use super::context_closures::{closure, function_type, plan, unbunched};
use super::lift_names::fresh_name;
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashMap;
use syntax::ast::Span;
use tasty::tags::{CASEaccessor, FIELDaccessor, LAZY, OBJECT, PARAMsetter};
use tasty::{Kind, NameId, SymbolId, Type, TypeId};
use typer::tpd::TNodeId;

/// `def m(params): F = closure(m$$anonfun$N)` of the type of `other`, flattened by fewer context
/// clauses than `m`, and the closures' methods, one per clause `other` lacks, each nested in the
/// one before: `def m$$anonfun$N(params, contextParams) = this.m(params, contextParams)` for the
/// last, in `class` at `span`; the methods innermost first, then their `$adapted` methods.
pub fn bridge(
    l: &mut Lower,
    class: SymbolId,
    m: SymbolId,
    other: SymbolId,
    span: Span,
) -> Result<(TNodeId, Vec<TNodeId>)> {
    let (_, depth) = l.context_results[&m];
    let flattened = l.context_results.get(&other).map_or(0, |&(_, d)| d);
    let (m_info, other_info) = (l.u.info(m), l.u.info(other));
    let own_clauses = clauses(l, m_info);
    let own = own_clauses.len() - depth + flattened;
    if depth <= flattened || clauses(l, other_info).len() != own {
        return unsupported(span, "bridges of methods with context function results flattened otherwise");
    }
    let m_result = l.u.result_type(m_info);
    let other_params: Vec<(NameId, TypeId)> = clauses(l, other_info).concat();
    let mut captured: Vec<(NameId, TypeId)> = own_clauses[..own].concat();
    let originals: Vec<NameId> = captured.iter().map(|&(name, _)| name).collect();
    for ((name, tpe), &(_, other)) in captured.iter_mut().zip(&other_params) {
        *name = fresh_name(l, *name);
        *tpe = other;
    }
    let (erased, others) = (l.u.erased_method(m).0, l.u.erased_method(other).0);
    let value_params = l.u.erased_method_value(m).0;
    let erased_result = l.u.erase_value(m_result);
    let anons = plan(l, (class, m), &own_clauses[own..], (captured, originals), erased_result, span)?;
    let apply = super::bunched::array_apply(l)?;
    let (mut defs, mut adapteds) = (Vec::new(), Vec::new());
    let mut result = m_result;
    for i in (0..anons.len()).rev() {
        let anon = anons[i].sym;
        let binder = l.u.fresh_binder();
        let info = l.u.intern_method(binder, &anons[i].params, result, false, false);
        l.u.set_info(anon, info);
        let inner = anons.get(i + 1);
        defs.push(l.forwarding_def(anon, span, |l, args, result| {
            let args = unbunched(l, apply, &anons[i], args, span);
            if let Some(a) = inner {
                return closure(l, a, &args, span);
            }
            let args: Vec<TNodeId> = args
                .iter()
                .enumerate()
                .map(|(i, &a)| match (erased.get(i), others.get(i), value_params.get(i)) {
                    (Some(p), Some(o), Some(v)) if p != o => adapt(l, a, v),
                    _ => a,
                })
                .collect();
            let this = l.this(class, span);
            let fun = l.select(this, m, span);
            l.apply(fun, &args, result, span)
        }));
        if let Some((adapted, names, needed)) = anons[i].adapted.clone() {
            let methods = (class, anon, adapted);
            adapteds.push(erased_adapted_def(l, methods, (anons[i].arity, needed), names, span));
            l.bridges.insert(adapted);
        }
        result = function_type(l, anons[i].arity);
    }
    let mut flags = l.u.symbol(m).flags;
    for tag in [FIELDaccessor, PARAMsetter, CASEaccessor, LAZY, OBJECT] {
        flags = flags.without(tag);
    }
    let first = &anons[0];
    let (sym, def) =
        l.method_like(class, other, flags, other_info, span, |l, args, _| closure(l, first, args, span))?;
    l.bridges.insert(sym);
    for (i, a) in anons.iter().enumerate() {
        let from = if i == 0 { sym } else { anons[i - 1].sym };
        l.lifted_from.insert(a.sym, from);
        if let Some((adapted, ..)) = a.adapted {
            l.lifted_from.insert(adapted, from);
        }
    }
    defs.extend(adapteds);
    Ok((def, defs))
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
            related.extend(l.u.decls_named(base, &name));
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
    let found = l.u.decls_named(pkg, class);
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
pub fn clauses(l: &Lower, mut t: TypeId) -> Vec<Vec<(NameId, TypeId)>> {
    let mut out = Vec::new();
    loop {
        match *l.u.ty(t) {
            Type::Poly { result, .. } => t = result,
            Type::Method { params, result, .. } => {
                out.push(l.u.params_of(params).to_vec());
                t = result;
            }
            _ => return out,
        }
    }
}
