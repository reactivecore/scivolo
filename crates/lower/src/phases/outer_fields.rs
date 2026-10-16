//! The enclosing instance a class holds: a leading constructor parameter `$outer`, a field and an
//! accessor `C$$$outer()`; its references to enclosing instances read the parameter in
//! constructor code, else the field. An anonymous local class has the field only where its
//! methods read it and the accessor only where called (dotty's `DropOuterAccessors`).

use super::explicit_outer::{outer_accessor, outer_prefix, outer_this, terms};
use super::local_free::is_local;
use super::local_outer::is_anonymous;
use super::walk::children;
use crate::definitions::{symbol, type_ref};
use crate::symbols::has;
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashMap;
use syntax::ast::Span;
use tasty::tags::{FIELDaccessor, LAZY, LOCAL, PRIVATE};
use tasty::{Flags, Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

/// Gives class definition `td` of `class` its constructor parameter for the instance of `outer`.
pub fn declare(l: &mut Lower, td: TNodeId, class: SymbolId, outer: SymbolId) {
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { constr, .. } = *l.tpd.tree(template) else { unreachable!("a template") };
    let tpe = type_ref(l.u, outer);
    let ctor = l.tpd.sym(constr).expect("a constructor's symbol");
    let param = l.u.define(symbol("$outer", Kind::Param, ctor, Flags::default()));
    l.u.set_info(param, tpe);
    super::leading_params::add_params(l, constr, ctor, &[param]);
    l.outers.insert(class, (None, param));
}

/// Makes the references of class definition `td` of `class` to enclosing instances read its
/// parameter or field, and adds the field and accessor it needs.
pub fn rewrite(l: &mut Lower, td: TNodeId, class: SymbolId) -> Result<()> {
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { constr, parents, self_, body } = *l.tpd.tree(template) else {
        unreachable!("a template")
    };
    let anonymous = is_local(l, class) && is_anonymous(l, class);
    let stats = l.tpd.items(body).to_vec();
    let parent_args = l.tpd.items(parents).to_vec();
    for &stat in stats.iter().chain(&parent_args) {
        if matches!(l.tpd.tree(stat), Tree::TypeDef { .. }) {
            continue;
        }
        let method = match (l.tpd.tree(stat), l.tpd.sym(stat)) {
            (Tree::DefDef { .. }, Some(s)) => !has(l.u, s, FIELDaccessor) || has(l.u, s, LAZY),
            _ => false,
        };
        let all = terms(l, stat, false);
        let mut parents: Option<FxHashMap<TNodeId, TNodeId>> = None;
        for &t in &all {
            let t = outer_prefix(l, t, class).unwrap_or(t);
            let Some(chain) = outer_this(l, t, class) else { continue };
            let parents = parents.get_or_insert_with(|| {
                all.iter().flat_map(|&p| children(l.tpd, p).into_iter().map(move |c| (c, p))).collect()
            });
            let span = parents.get(&t).and_then(|&p| l.after_siblings(p, t)).unwrap_or(l.tpd.span(t));
            let start = if method {
                let this = l.this(class, span);
                let field = field(l, class);
                l.select(this, field, span)
            } else {
                let param = l.outers[&class].1;
                l.ident(param, span)
            };
            let read = through(l, start, &chain, span)?;
            l.copy_node(t, read);
        }
    }
    let with_accessor = !anonymous || l.outer_accessors.contains_key(&class);
    if !anonymous {
        field(l, class);
    }
    let Some(field) = l.outers[&class].0 else { return Ok(()) };
    let name_at = l.tpd.span(td).point();
    let at = Span::at(l.tpd.span(template).end().max(name_at));
    let mut stats = stats;
    stats.push(field_def(l, field, Span::at(name_at)));
    if with_accessor {
        stats.push(accessor(l, class, field, at));
    }
    let body = l.tpd.list(&stats);
    l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
    Ok(())
}

/// `start`, the instance of the first class of `chain`, then the outer accessors to the last.
fn through(l: &mut Lower, start: TNodeId, chain: &[SymbolId], span: Span) -> Result<TNodeId> {
    let mut read = start;
    for pair in chain.windows(2) {
        if is_local(l, pair[0]) && is_anonymous(l, pair[0]) {
            return unsupported(span, "classes in anonymous classes using the enclosing instance");
        }
        let accessor = outer_accessor(l, pair[0]);
        let fun = l.select(read, accessor, span);
        let tpe = type_ref(l.u, pair[1]);
        read = l.apply(fun, &[], tpe, span);
    }
    Ok(read)
}

/// The instance of `target` enclosing `class` of definition `td` read in a method of `class` if
/// `in_method`, else in its constructor, adding the field it reads, or of a trait through its
/// accessor; fails where `class` holds no instance.
pub fn outer_path(
    l: &mut Lower,
    (td, class): (TNodeId, SymbolId),
    target: SymbolId,
    in_method: bool,
    span: Span,
) -> Result<TNodeId> {
    let refused = "local classes calling local methods that use the enclosing instance";
    let accessor = l.outer_accessors.get(&class).copied().filter(|_| l.u.is_trait(class));
    let chain = match accessor {
        Some(a) => {
            let info = l.u.info(a);
            let first = l.u.class_symbol(l.u.result_type(info)).expect("an outer class");
            let rest = super::explicit_outer::outer_chain(l, first, target);
            if first == target { Some(vec![first]) } else { rest.map(|r| [vec![first], r].concat()) }
        }
        None => super::explicit_outer::outer_chain(l, class, target),
    };
    let Some(chain) = chain else {
        return unsupported(span, refused);
    };
    let start = match l.outers.get(&class).copied() {
        Some((had, _)) if in_method => {
            let f = field(l, class);
            if had.is_none() {
                append_field(l, td, f);
            }
            let this = l.this(class, span);
            l.select(this, f, span)
        }
        Some((_, param)) => l.ident(param, span),
        None => {
            let Some(accessor) = accessor else { return unsupported(span, refused) };
            let this = l.this(class, span);
            let fun = l.select(this, accessor, span);
            let tpe = type_ref(l.u, chain[0]);
            l.apply(fun, &[], tpe, span)
        }
    };
    through(l, start, &chain, span)
}

/// Appends the definition of field `f` to class definition `td`.
fn append_field(l: &mut Lower, td: TNodeId, f: SymbolId) {
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { constr, parents, self_, body } = *l.tpd.tree(template) else {
        unreachable!("a template")
    };
    let def = field_def(l, f, Span::at(l.tpd.span(td).point()));
    let stats: Vec<TNodeId> = l.tpd.items(body).iter().copied().chain([def]).collect();
    let body = l.tpd.list(&stats);
    l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
}

/// The field holding the enclosing instance of `class`, defined once.
fn field(l: &mut Lower, class: SymbolId) -> SymbolId {
    let (field, param) = l.outers[&class];
    if let Some(f) = field {
        return f;
    }
    let f = l.u.define(symbol("$outer", Kind::Val, class, Flags::default().with(PRIVATE).with(LOCAL)));
    let tpe = l.u.info(param);
    l.u.set_info(f, tpe);
    l.fields.insert(f, f);
    l.outers.insert(class, (Some(f), param));
    f
}

/// The definition of field `field` without initializer, at `span`.
fn field_def(l: &mut Lower, field: SymbolId, span: Span) -> TNodeId {
    let info = l.u.info(field);
    let tpt = l.type_tree(info, span);
    l.build().val_def(field, tpt, None, span)
}

/// `final def C$$$outer(): Outer = this.$outer` of `class`, at `span`.
fn accessor(l: &mut Lower, class: SymbolId, field: SymbolId, span: Span) -> TNodeId {
    let m = outer_accessor(l, class);
    let result = l.u.info(field);
    let this = l.this(class, span);
    let read = l.select(this, field, span);
    let tpt = l.type_tree(result, span);
    l.method_def(m, &[], tpt, read, span)
}
