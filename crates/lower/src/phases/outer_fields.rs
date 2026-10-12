//! The enclosing instance a class holds: a leading constructor parameter `$outer`, a field and an
//! accessor `C$$$outer()`; its references to enclosing instances read the parameter in
//! constructor code, else the field. An anonymous local class has the field only where its
//! methods read it and the accessor only where called (dotty's `DropOuterAccessors`).

use super::explicit_outer::{outer_accessor, outer_this, terms};
use super::local_free::is_local;
use super::local_outer::is_anonymous;
use crate::definitions::{symbol, type_ref};
use crate::symbols::has;
use crate::{Lower, Result, unsupported};
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
    super::local_classes::add_params(l, constr, ctor, &[param]);
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
        for t in terms(l, stat, false) {
            let Some(chain) = outer_this(l, t, class) else { continue };
            let span = l.tpd.span(t);
            let mut read = if method {
                let this = l.this(class, span);
                let field = field(l, class);
                l.select(this, field, span)
            } else {
                let param = l.outers[&class].1;
                l.ident(param, span)
            };
            for pair in chain.windows(2) {
                if is_local(l, pair[0]) && is_anonymous(l, pair[0]) {
                    return unsupported(span, "classes in anonymous classes using the enclosing instance");
                }
                let accessor = outer_accessor(l, pair[0]);
                let fun = l.select(read, accessor, span);
                let tpe = type_ref(l.u, pair[1]);
                read = l.apply(fun, &[], tpe, span);
            }
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
