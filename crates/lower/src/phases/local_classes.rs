//! Classes defined in blocks (dotty's `LambdaLift` for classes): each becomes a class of its
//! package named `Outer$Name$N`, taking the local values it uses as its first constructor
//! parameters, kept in private fields where its methods read them.

use super::dependencies::{Dependencies, dependencies};
use super::lift_names::{Caller, LocalClass, Pending};
use super::local_free::{
    Free, Local, enclosing_class, free_values, is_local, is_method, is_term, lifted_into, within,
};
use super::local_lift::{lift, pass_values};
use super::local_proxies::{constructor, definition_order, definition_span, make_proxies};
use super::nested_reads::Nested;
use crate::symbols::has;
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashMap;
use syntax::ast::Span;
use tasty::tags::MUTABLE;
use tasty::{Kind, SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

/// Lifts the local classes among `classes` out of the blocks of unit `root`; their names and
/// those of their proxies are left to `lambda_lift`.
pub fn transform(l: &mut Lower, root: TNodeId, classes: &[TNodeId]) -> Result<Pending> {
    let deps = dependencies(l, root);
    let (mut locals, mut traits) = (Vec::new(), Vec::new());
    for &td in classes {
        let class = l.tpd.sym(td).expect("a class definition's symbol");
        let Some(owner) = l.u.symbol(class).owner.filter(|&o| is_term(l, o)) else { continue };
        let outer = enclosing_class(l, owner);
        let mut free = free_values(l, td, class, outer)?;
        free.values = deps.of(constructor(l, td)).to_vec();
        free.in_methods = deps.of(class).to_vec();
        let refused = match () {
            _ if free.values.iter().any(|&v| has(l.u, v, MUTABLE)) => {
                Some("local classes capturing variables")
            }
            _ if !l.u.is_trait(class) => None,
            _ if !free.values.is_empty() => Some("local traits initializing with captured values"),
            _ => None,
        };
        if let Some(what) = refused {
            return unsupported(l.tpd.span(td), what);
        }
        if l.u.is_trait(class) {
            traits.push(td);
        }
        let in_trait = l.u.is_trait(outer) && is_local(l, outer) && uses_outside(l, &deps, class, outer);
        let named_in = if in_trait { outer } else { lifted_into(l, outer) };
        locals.push(Local { td, class, named_in, free });
    }
    let order = definition_order(l, &locals);
    let mut pending = Pending::default();
    let proxies = make_proxies(l, &locals, &mut pending);
    for (c, (fields, _)) in locals.iter().zip(&proxies) {
        pass_values(l, root, c.class, &c.free, fields, &mut pending.passed)?;
    }
    super::local_traits::transform(l, root, &traits, &deps, &mut pending)?;
    pending.deps = Some(deps);
    let own = locals
        .iter()
        .map(|c| (c.class, c.free.values.iter().chain(&c.free.in_methods).copied().collect()))
        .collect();
    let mut nested = Nested { own, named_in: FxHashMap::default() };
    let mut classes = Vec::new();
    for (Local { td, class, named_in, free, .. }, (fields, params)) in locals.into_iter().zip(proxies) {
        let spans: Vec<Span> = free.in_methods.iter().map(|&v| definition_span(l, root, v)).collect();
        if !l.u.is_trait(class) {
            add_proxies(l, td, class, &free, (&fields, &params), (&spans, &mut nested))?;
        }
        let named_in = nested.named_in.get(&class).copied().unwrap_or(named_in);
        if free.called.iter().any(|&(f, _)| l.u.symbol(f).kind == Kind::Def) {
            let fields = free.in_methods.iter().copied().zip(fields).collect();
            let params = free.values.iter().copied().zip(params).collect();
            pending.callers.push(Caller { td, class, fields, params });
        }
        if let Some((block, index)) = lift(l, root, td, class) {
            pending.definitions.push((block, index, free.values.clone()));
        }
        classes.push(LocalClass { at: l.tpd.span(td).start(), class, outer: named_in });
    }
    let mut last = None;
    for i in order {
        let c = &classes[i];
        let at = match last {
            Some(before) if c.at < before => before,
            _ => c.at,
        };
        last = Some(at);
        pending.classes.push(LocalClass { at, class: c.class, outer: c.outer });
    }
    Ok(pending)
}

/// Whether `class` or what is defined in it uses a local value from outside `outer` (dotty's
/// `narrowLogicOwner` to an intermediate local trait).
fn uses_outside(l: &Lower, deps: &Dependencies, class: SymbolId, outer: SymbolId) -> bool {
    let inside = deps.free.iter().filter(|(o, _)| within(l, *o, class));
    inside.flat_map(|(_, vs)| vs).any(|&v| !within(l, v, outer))
}

/// Adds to class definition `td` a leading constructor parameter of `params` for each value of
/// `free` and a private field of `fields`, at the value's definition span of `spans`, for each its
/// methods use; makes the references to them in its methods and nested classes, but to those a
/// nested local class captures itself by `nested`, read the fields and elsewhere the parameters.
fn add_proxies(
    l: &mut Lower,
    td: TNodeId,
    class: SymbolId,
    free: &Free,
    (fields, params): (&[SymbolId], &[SymbolId]),
    (spans, nested): (&[Span], &mut Nested),
) -> Result<()> {
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { constr, parents, self_, body } = *l.tpd.tree(template) else {
        unreachable!("a template")
    };
    let ctor = l.tpd.sym(constr).expect("a constructor's symbol");
    let mut field_defs = Vec::new();
    for ((&v, &field), &at) in free.in_methods.iter().zip(fields).zip(spans) {
        l.fields.insert(field, field);
        l.captured_fields.insert((class, v), field);
        let info = l.u.info(v);
        let tpt = l.type_tree(info, Span::NONE);
        field_defs.push(l.build().val_def(field, tpt, None, at));
        let i = free.values.iter().position(|&w| w == v).expect("a free value");
        l.proxies.push((field, params[i]));
    }
    add_params(l, constr, ctor, params);
    for parent in l.tpd.items(parents).to_vec() {
        redirect(l, parent, &free.values, params, None);
    }
    for stat in l.tpd.items(body).to_vec() {
        match l.tpd.tree(stat) {
            Tree::DefDef { .. } if is_method(l, l.tpd.sym(stat)) => {
                redirect(l, stat, &free.in_methods, fields, Some(class))
            }
            Tree::TypeDef { .. } => {}
            _ => redirect(l, stat, &free.values, params, None),
        }
        super::nested_reads::redirect(l, stat, class, (&free.in_methods, fields), nested)?;
    }
    let stats: Vec<TNodeId> = field_defs.into_iter().chain(l.tpd.items(body).iter().copied()).collect();
    let body = l.tpd.list(&stats);
    l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
    Ok(())
}

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

/// Makes the references in `t`, outside the classes defined in it, to values `free` refer to
/// `to`, fields of `this` of `class` if given.
fn redirect(l: &mut Lower, t: TNodeId, free: &[SymbolId], to: &[SymbolId], class: Option<SymbolId>) {
    for s in super::explicit_outer::terms(l, t, false) {
        if !matches!(l.tpd.tree(s), Tree::Ident { .. }) {
            continue;
        }
        let Some(i) = l.tpd.sym(s).and_then(|v| free.iter().position(|&f| f == v)) else { continue };
        let span = l.tpd.span(s);
        let node = match class {
            Some(c) => {
                let this = l.this(c, span);
                l.select(this, to[i], span)
            }
            None => l.ident(to[i], span),
        };
        l.copy_node(s, node);
    }
}
