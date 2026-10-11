//! dotty's `ExplicitOuter` for classes and objects in classes: one needing its enclosing instance
//! gets a field `$outer`, set from a leading constructor parameter, and an accessor
//! `C$$$outer()`; its references to the instance read the parameter in constructor code, else
//! the field.

use crate::definitions::{symbol, type_ref};
use crate::symbols::{has, is_class, is_module, is_module_class, is_static, is_trait};
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::tags::{FIELDaccessor, FINAL, LAZY, LOCAL, PRIVATE};
use tasty::{Flags, Kind, SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

pub fn transform(l: &mut Lower, classes: &[TNodeId]) -> Result<()> {
    for &td in classes {
        let class = l.tpd.sym(td).expect("a class definition's symbol");
        let Some(outer) = enclosing_instance(l, class) else { continue };
        let (own, all) = (terms(l, td, false), terms(l, td, true));
        if let Some(&t) = all.iter().find(|&&t| is_this_of(l, t, outer) && !own.contains(&t)) {
            return unsupported(l.tpd.span(t), "the enclosing instance two classes out");
        }
        let uses = own.iter().any(|&t| is_this_of(l, t, outer) || creates_inner_of(l, t, outer));
        let always = match is_module_class(l.u, class) {
            true => is_trait(l.u, outer),
            false => !has(l.u, class, PRIVATE) && !is_local(l, outer),
        };
        if always || uses {
            add_outer(l, td, class, outer);
        }
    }
    for &td in classes {
        super::outer_args::pass_outer(l, td)?;
    }
    Ok(())
}

/// The class whose instance a class or object `class` nested in it can use (dotty's
/// `needsOuterIfReferenced`), unless static or a trait.
pub fn enclosing_instance(l: &Lower, class: SymbolId) -> Option<SymbolId> {
    let owner = l.u.symbol(class).owner?;
    let nested = is_class(l.u, owner) && !is_trait(l.u, class);
    (nested && !is_static(l.u, class)).then_some(owner)
}

/// Whether `class` is defined in a method or value, directly or through other classes.
fn is_local(l: &Lower, class: SymbolId) -> bool {
    let mut owner = l.u.symbol(class).owner;
    while let Some(o) = owner {
        if l.u.symbol(o).kind.is_term() {
            return true;
        }
        owner = l.u.symbol(o).owner;
    }
    false
}

/// Whether `t` is `new C` of a class `C` whose enclosing instance is one of `outer` (dotty's
/// `referencesOuter` of a `New`).
fn creates_inner_of(l: &mut Lower, t: TNodeId, outer: SymbolId) -> bool {
    let Tree::New { .. } = l.tpd.tree(t) else { return false };
    let tpe = l.tpd.tpe(t);
    l.u.class_symbol(tpe).is_some_and(|c| enclosing_instance(l, c) == Some(outer))
}

/// Why a type test against `target` cannot be lowered: an object of a class, or in a match a
/// class that may test its enclosing instance (dotty's `addOuterTest`).
pub fn unsupported_test(l: &mut Lower, target: tasty::TypeId, in_match: bool) -> Option<&'static str> {
    if let Type::Sym(_, s) = *l.u.types().get(target)
        && is_module(l.u, s)
        && !is_static(l.u, s)
    {
        return Some("type tests of objects nested in classes");
    }
    let widened = l.u.widen(target);
    let dealiased = l.u.dealias(widened);
    let inner = l.u.class_symbol(dealiased).is_some_and(|c| enclosing_instance(l, c).is_some());
    (in_match && inner).then_some("patterns of classes in classes")
}

/// `t` and the trees in it but those in type trees, and in class definitions within `t` unless
/// `nested`.
pub fn terms(l: &Lower, t: TNodeId, nested: bool) -> Vec<TNodeId> {
    let mut out = Vec::new();
    let mut todo = vec![t];
    while let Some(s) = todo.pop() {
        out.push(s);
        let skip = match l.tpd.tree(s) {
            Tree::SingletonTypeTree { .. } | Tree::AppliedTypeTree { .. } | Tree::TypeTree { .. } => true,
            Tree::TypeDef { .. } => s != t && !nested,
            _ => false,
        };
        if !skip {
            todo.extend(super::walk::children(l.tpd, s));
        }
    }
    out
}

fn is_this_of(l: &Lower, t: TNodeId, class: SymbolId) -> bool {
    matches!(l.tpd.tree(t), Tree::This { .. }) && l.tpd.sym(t) == Some(class)
}

/// Gives object `class` of definition `td` its field, constructor parameter and accessor for
/// the instance of `outer`, and makes its references to that instance read them.
fn add_outer(l: &mut Lower, td: TNodeId, class: SymbolId, outer: SymbolId) {
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { constr, parents, self_, body } = *l.tpd.tree(template) else {
        unreachable!("a template")
    };
    let tpe = type_ref(l.u, outer);
    let field = l.u.define(symbol("$outer", Kind::Val, class, Flags::default().with(PRIVATE).with(LOCAL)));
    l.u.set_info(field, tpe);
    l.fields.insert(field, field);
    let ctor = l.tpd.sym(constr).expect("a constructor's symbol");
    let param = l.u.define(symbol("$outer", Kind::Param, ctor, Flags::default()));
    l.u.set_info(param, tpe);
    super::local_classes::add_params(l, constr, ctor, &[param]);
    l.outers.insert(class, (field, param));
    let stats = l.tpd.items(body).to_vec();
    let parent_args = l.tpd.items(parents).to_vec();
    for &stat in stats.iter().chain(&parent_args) {
        let method = match (l.tpd.tree(stat), l.tpd.sym(stat)) {
            (Tree::DefDef { .. }, Some(s)) => !has(l.u, s, FIELDaccessor) || has(l.u, s, LAZY),
            _ => false,
        };
        for t in terms(l, stat, false) {
            if !is_this_of(l, t, outer) {
                continue;
            }
            let span = l.tpd.span(t);
            let read = if method {
                let this = l.this(class, span);
                l.select(this, field, span)
            } else {
                l.ident(param, span)
            };
            l.copy_node(t, read);
        }
    }
    let name_at = l.tpd.span(td).point();
    let at = Span::at(l.tpd.span(template).end().max(name_at));
    let field_def = field_def(l, field, Span::at(name_at));
    let accessor = accessor(l, class, field, at);
    let stats: Vec<TNodeId> = stats.into_iter().chain([field_def, accessor]).collect();
    let body = l.tpd.list(&stats);
    l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
}

/// The definition of field `field` without initializer, at `span`.
fn field_def(l: &mut Lower, field: SymbolId, span: Span) -> TNodeId {
    let info = l.u.info(field);
    let tpt = l.type_tree(info, span);
    l.build().val_def(field, tpt, None, span)
}

/// `final def C$$$outer(): Outer = this.$outer` of `class`, at `span` (dotty's
/// `outerAccName`: the class's name expanded, then `$$$outer`).
fn accessor(l: &mut Lower, class: SymbolId, field: SymbolId, span: Span) -> TNodeId {
    let name = format!("{}$$$outer", l.u.binary_name(class).replace('.', "$"));
    let m = l.u.define(symbol(&name, Kind::Def, class, Flags::default().with(FINAL)));
    let result = l.u.info(field);
    let binder = l.u.fresh_binder();
    let info = l.u.intern_type(Type::Method {
        binder,
        params: Vec::new(),
        result,
        implicit: false,
        contextual: false,
    });
    l.u.set_info(m, info);
    let this = l.this(class, span);
    let read = l.select(this, field, span);
    let tpt = l.type_tree(result, span);
    l.method_def(m, &[], tpt, read, span)
}
