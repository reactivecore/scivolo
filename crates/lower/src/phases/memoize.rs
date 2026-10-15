//! dotty's `Memoize`: each getter of a class gets a private field that its right-hand side
//! (or, for a class parameter, the constructor) initializes and that the getter returns; a
//! setter stores its argument in the field; a constant `final` value needs no field. The class's
//! own accessors come first, then those `Mixin` added, in its order.

use super::accessor_names::setter_getter_name;
use super::getters::{is_const_final, is_param_accessor, is_setter};
use super::literals::is_wildcard;
use crate::definitions::symbol;
use crate::symbols::has;
use crate::{Lower, Result};
use rustc_hash::{FxHashMap, FxHashSet};
use syntax::ast::Span;
use tasty::tags::{FIELDaccessor, LAZY, LOCAL, MUTABLE, PRIVATE};
use tasty::{Erased, Flags, Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

pub fn transform(l: &mut Lower, td: TNodeId) -> Result<()> {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { constr, parents, self_, body } = *l.tpd.tree(template) else { return Ok(()) };
    let items = l.tpd.items(body).to_vec();
    let mut fields: FxHashMap<TNodeId, TNodeId> = FxHashMap::default();
    let mut memoized = FxHashSet::default();
    let mut fence = false;
    for mixed in [false, true] {
        for &stat in &items {
            let param_setter = |l: &Lower, s| is_param_accessor(l, s) && is_setter(l, s);
            let accessor = l
                .tpd
                .sym(stat)
                .filter(|&s| has(l.u, s, FIELDaccessor) && (!l.u.is_deferred(s) || param_setter(l, s)));
            let Some(accessor) = accessor.filter(|s| l.mixed_in.contains(s) == mixed) else { continue };
            if is_setter(l, accessor) {
                l.u.set_deferred(accessor, false);
                setter(l, class, stat, accessor, &memoized, &mut fence);
            } else if !is_const_final(l, accessor) && !has(l.u, accessor, LAZY) {
                fields.extend(getter(l, class, stat, accessor).map(|f| (stat, f)));
                memoized.insert(l.u.symbol(accessor).name.clone());
            }
        }
    }
    let mut stats = Vec::new();
    for stat in items {
        stats.extend(fields.get(&stat).copied());
        stats.push(stat);
    }
    if fence {
        let spans = super::walk::children(l.tpd, template).into_iter().map(|c| l.tpd.span(c));
        let end = spans.filter(|s| s.exists()).reduce(Span::union).unwrap_or(l.tpd.span(template));
        let release_fence = l.equality()?.release_fence;
        let call = l.static_call(release_fence, &[]);
        l.tpd.get_mut(call).span = end.end_pos();
        stats.push(call);
    }
    let body = l.tpd.list(&stats);
    l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
    Ok(())
}

/// Makes getter definition `stat` of `getter` return its field, or the value of a type needing
/// none; the field's definition, none for a mixed-in or uninitialized getter of such a type.
fn getter(l: &mut Lower, class: SymbolId, stat: TNodeId, getter: SymbolId) -> Option<TNodeId> {
    let Tree::DefDef { name, paramss, tpt, rhs } = *l.tpd.tree(stat) else { unreachable!("a getter") };
    let mixed_in = l.mixed_in.contains(&getter);
    let bottom = bottom(l, getter);
    let span = l.tpd.span(stat);
    let init = rhs.filter(|&r| !is_wildcard(l, r));
    let field = (bottom.is_none() || !mixed_in && init.is_some()).then(|| {
        let name_text = super::accessor_names::run_name(l, getter).to_owned();
        let mut flags = Flags::default().with(PRIVATE).with(LOCAL);
        if has(l.u, getter, MUTABLE) {
            flags = flags.with(MUTABLE);
        }
        let field = l.u.define(symbol(&name_text, Kind::Val, class, flags));
        l.fields.insert(field, getter);
        let info = l.u.info(getter);
        l.u.set_info(field, info);
        let field_span = if mixed_in { Span::NONE } else { span };
        let field_tpt = l.type_tree(info, field_span);
        let def = Tree::ValDef { name, tpt: field_tpt, rhs: init };
        (field, l.tpd.node(def, field_span, None, Some(field)))
    });
    let tpt_span = l.tpd.span(tpt);
    let inferred = matches!(l.tpd.tree(tpt), Tree::TypeTree { inferred: true });
    let recorded = l.getter_rhs.get(&getter).copied();
    let read_span = match (recorded, rhs.map(|r| l.tpd.span(r))) {
        _ if inferred && tpt_span.exists() => tpt_span.end_pos(),
        (Some(r), _) => r,
        (None, Some(r)) if r.exists() => r.with_point(r.start()),
        _ if span.exists() => span.with_point(span.start()),
        _ => span,
    };
    let read = match (bottom, field) {
        (Some(Erased::Unit), _) => l.unit_literal(read_span),
        (Some(Erased::Null), _) => l.null_literal(read_span),
        (Some(_), _) => {
            let null = l.null_literal(read_span);
            l.throw_(null)
        }
        (None, Some((field, _))) => l.field_ref(field, read_span),
        (None, None) => unreachable!("a field"),
    };
    l.tpd.get_mut(stat).tree = Tree::DefDef { name, paramss, tpt, rhs: Some(read) };
    field.map(|(_, def)| def)
}

/// The erased class of getter `getter`'s value if it is `Unit`, `Null` or `Nothing`, which need
/// no field (dotty's `isErasableBottomField`).
pub fn bottom(l: &mut Lower, getter: SymbolId) -> Option<Erased> {
    let info = l.u.info(getter);
    let erased = l.u.erase(info);
    matches!(erased, Erased::Unit | Erased::Null | Erased::Nothing).then_some(erased)
}

/// Makes setter definition `stat` of `setter` store its argument in the field of its getter, if
/// that getter is among `memoized` and not a class parameter's; else its body stays `()`. A
/// `val`'s field becomes mutable, and `fence` true (dotty's `classesThatNeedReleaseFence`).
fn setter(
    l: &mut Lower,
    class: SymbolId,
    stat: TNodeId,
    setter: SymbolId,
    memoized: &FxHashSet<String>,
    fence: &mut bool,
) {
    let Tree::DefDef { name, paramss, tpt, rhs } = *l.tpd.tree(stat) else { unreachable!("a setter") };
    let unit_body = |l: &mut Lower| rhs.unwrap_or_else(|| l.unit_literal(l.tpd.span(stat)));
    let getter_name = setter_getter_name(&l.u.symbol(setter).name).to_owned();
    let getters = l.u.decl(class, &getter_name).unwrap_or_default();
    let getter = getters.into_iter().find(|&g| !is_setter(l, g) && !l.bridges.contains(&g));
    let param_val = getter.is_some_and(|g| is_param_accessor(l, g) && !has(l.u, g, MUTABLE));
    let Some(getter) = getter.filter(|_| memoized.contains(&getter_name) && !param_val) else {
        let rhs = unit_body(l);
        l.tpd.get_mut(stat).tree = Tree::DefDef { name, paramss, tpt, rhs: Some(rhs) };
        return;
    };
    let field = l.fields.iter().find_map(|(&f, &g)| (g == getter && f != g).then_some(f));
    if !has(l.u, getter, MUTABLE) {
        if let Some(field) = field {
            let flags = l.u.symbol(field).flags.with(MUTABLE);
            l.u.set_flags(field, flags);
        }
        *fence = true;
    }
    let rhs = match (bottom(l, getter), field) {
        (Some(_), _) if rhs.is_none() => {
            let s = l.tpd.span(stat);
            l.unit_literal(if s.exists() { s.with_point(s.start()) } else { s })
        }
        (Some(_), _) | (None, None) => unit_body(l),
        (None, Some(field)) => {
            let param_tree = l.tpd.items(l.tpd.clause_items(paramss)[0])[0];
            let span = rhs.map_or(l.tpd.span(param_tree), |r| l.tpd.span(r));
            let param = l.tpd.sym(param_tree).expect("a setter's parameter");
            let lhs = l.field_ref(field, span);
            let value = l.ident(param, span);
            let unit = l.u.intern_type(tasty::Type::Constant(tasty::tags::UNITconst, 0));
            l.tpd.node(Tree::Assign { lhs, rhs: value }, span, Some(unit), None)
        }
    };
    l.tpd.get_mut(stat).tree = Tree::DefDef { name, paramss, tpt, rhs: Some(rhs) };
}
