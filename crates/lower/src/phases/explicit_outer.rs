//! dotty's `ExplicitOuter` for objects in classes: an object using its enclosing instance gets
//! a field `$outer`, set from a leading constructor parameter, and an accessor `C$$$outer()`;
//! its references to the instance read the parameter in constructor code, else the field, and
//! `new C` passes the instance.

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
            return unsupported(l.tpd.span(t), "the enclosing instance in classes nested in objects");
        }
        if let Some(&t) = all.iter().find(|&&t| is_private_member(l, t, outer)) {
            return unsupported(l.tpd.span(t), "private members of an enclosing class");
        }
        if is_trait(l.u, outer) || own.iter().any(|&t| is_this_of(l, t, outer)) {
            add_outer(l, td, class, outer);
        }
    }
    for &td in classes {
        pass_outer(l, td)?;
    }
    Ok(())
}

/// The class whose instance an object `class` nested in it can use, unless static.
fn enclosing_instance(l: &Lower, class: SymbolId) -> Option<SymbolId> {
    let owner = l.u.symbol(class).owner?;
    let nested = is_module_class(l.u, class) && is_class(l.u, owner);
    (nested && !is_static(l.u, class)).then_some(owner)
}

/// `t` and the trees in it but those in type trees, and in class definitions within `t` unless
/// `nested`.
fn terms(l: &Lower, t: TNodeId, nested: bool) -> Vec<TNodeId> {
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

/// Whether `t` refers to a private member of `class` other than an object, which dotty's
/// `ExpandPrivate` renames when a nested class uses it.
fn is_private_member(l: &Lower, t: TNodeId, class: SymbolId) -> bool {
    let reference = matches!(l.tpd.tree(t), Tree::Ident { .. } | Tree::Select { .. });
    l.tpd.sym(t).is_some_and(|s| {
        reference
            && l.u.symbol(s).owner == Some(class)
            && has(l.u, s, PRIVATE)
            && l.u.symbol(s).kind.is_term()
            && !is_module(l.u, s)
    })
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
    let name = l.term_name(&l.u.symbol(field).name.clone());
    let info = l.u.info(field);
    let tpt = l.type_tree(info, span);
    l.tpd.node(Tree::ValDef { name, tpt, rhs: None }, span, None, Some(field))
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

/// Passes the enclosing instance to each `new C` in class definition `td` of an object `C` that
/// takes it.
fn pass_outer(l: &mut Lower, td: TNodeId) -> Result<()> {
    for t in terms(l, td, false) {
        let Tree::Apply { fun, args } = *l.tpd.tree(t) else { continue };
        let Tree::Select { qualifier: new, .. } = *l.tpd.tree(fun) else { continue };
        if !matches!(l.tpd.tree(new), Tree::New { .. }) {
            continue;
        }
        let Some(created) = l.u.class_symbol(l.tpd.tpe(new)).filter(|c| l.outers.contains_key(c)) else {
            continue;
        };
        let outer = l.u.symbol(created).owner.expect("an object's class");
        let enclosing = l.tpd.sym(td);
        if enclosing != Some(outer) {
            return unsupported(l.tpd.span(t), "objects of a class created outside it");
        }
        let instance = l.this(outer, Span::NONE);
        let all: Vec<TNodeId> = std::iter::once(instance).chain(l.tpd.items(args).iter().copied()).collect();
        let args = l.tpd.list(&all);
        l.tpd.get_mut(t).tree = Tree::Apply { fun, args };
    }
    Ok(())
}
