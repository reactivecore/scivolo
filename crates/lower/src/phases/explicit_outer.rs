//! dotty's `ExplicitOuter` for classes and objects in classes: one needing its enclosing instance
//! gets a field `$outer`, set from a leading constructor parameter, and an accessor
//! `C$$$outer()`; its references to the instance read the parameter in constructor code, else
//! the field.

use super::outer_traits::outer_class;
use crate::definitions::{symbol, type_ref};
use crate::symbols::{has, is_static};
use crate::{Lower, Result};
use syntax::ast::Span;
use tasty::tags::{FIELDaccessor, FINAL, LAZY, LOCAL, PRIVATE};
use tasty::{Flags, Kind, SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

pub fn transform(l: &mut Lower, classes: &[TNodeId]) -> Result<()> {
    for &td in classes {
        let class = l.tpd.sym(td).expect("a class definition's symbol");
        let mixes_inner = super::outer_traits::implement(l, td, class)?;
        let Some(outer) = outer_class(l, class) else { continue };
        let (own, all) = (terms(l, td, false), terms(l, td, true));
        let outward = |l: &Lower, t: TNodeId| outer_this(l, t, class).is_some();
        let uses = all.iter().any(|&t| outward(l, t)) || own.iter().any(|&t| creates_inner_of(l, t, outer));
        let always = mixes_inner
            || match l.u.is_module_class(class) {
                true => l.u.is_trait(outer),
                false => !has(l.u, class, PRIVATE) && !is_local(l, outer),
            };
        match (always || uses || l.outer_accessors.contains_key(&class), l.u.is_trait(class)) {
            (false, _) => {}
            (true, true) => super::outer_traits::add_outer(l, td, class),
            (true, false) => add_outer(l, td, class, outer),
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
    let nested = l.u.is_class(owner) && !l.u.is_trait(class);
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
        && l.u.is_module(s)
        && !is_static(l.u, s)
    {
        return Some("type tests of objects nested in classes");
    }
    let widened = l.u.widen(target);
    let dealiased = l.u.dealias(widened);
    let inner = l.u.class_symbol(dealiased).is_some_and(|c| outer_class(l, c).is_some());
    let tested = outer_prefix(l, target).is_some();
    (in_match && inner && !tested).then_some("patterns of classes in classes")
}

/// The prefix `O.this` of type `O.this.C` of a class `C` with an enclosing instance, the instance
/// a match tests (dotty's `addOuterTest` for a singleton prefix).
pub fn outer_prefix(l: &mut Lower, target: tasty::TypeId) -> Option<tasty::TypeId> {
    let target = l.u.dealias(target);
    let target = match *l.u.types().get(target) {
        Type::Applied(tycon, _) => tycon,
        _ => target,
    };
    let Type::Sym(Some(pre), c) = *l.u.types().get(target) else { return None };
    let singleton = matches!(l.u.types().get(pre), Type::This(_));
    (singleton && l.u.is_class(c) && enclosing_instance(l, c).is_some()).then_some(pre)
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

/// The classes from the enclosing instance of `class` out to the one `t` is `this` of, if `t`
/// is `this` of a class enclosing `class` through enclosing instances (dotty's `outer.path`).
pub fn outer_this(l: &Lower, t: TNodeId, class: SymbolId) -> Option<Vec<SymbolId>> {
    let target = l.tpd.sym(t).filter(|_| matches!(l.tpd.tree(t), Tree::This { .. }))?;
    let mut chain = Vec::new();
    let mut cur = outer_class(l, class);
    while let Some(c) = cur {
        chain.push(c);
        if c == target {
            return Some(chain);
        }
        cur = outer_class(l, c);
    }
    None
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
                l.select(this, field, span)
            } else {
                l.ident(param, span)
            };
            for pair in chain.windows(2) {
                let accessor = outer_accessor(l, pair[0]);
                let fun = l.select(read, accessor, span);
                let tpe = type_ref(l.u, pair[1]);
                read = l.apply(fun, &[], tpe, span);
            }
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

/// `final def C$$$outer(): Outer = this.$outer` of `class`, at `span`.
fn accessor(l: &mut Lower, class: SymbolId, field: SymbolId, span: Span) -> TNodeId {
    let m = outer_accessor(l, class);
    let result = l.u.info(field);
    let this = l.this(class, span);
    let read = l.select(this, field, span);
    let tpt = l.type_tree(result, span);
    l.method_def(m, &[], tpt, read, span)
}

/// The accessor `C$$$outer()` of the enclosing instance of class `class`, defined once; `class`
/// then gets its field (dotty's `outerAccName`, `ensureOuterAccessors`).
pub fn outer_accessor(l: &mut Lower, class: SymbolId) -> SymbolId {
    if let Some(&m) = l.outer_accessors.get(&class) {
        return m;
    }
    let name = outer_accessor_name(l, class);
    let m = l.u.define(symbol(&name, Kind::Def, class, Flags::default().with(FINAL)));
    let outer = outer_class(l, class).expect("a class with an enclosing instance");
    let result = type_ref(l.u, outer);
    let binder = l.u.fresh_binder();
    let info = l.u.intern_type(Type::Method {
        binder,
        params: Vec::new(),
        result,
        implicit: false,
        contextual: false,
    });
    l.u.set_info(m, info);
    l.outer_accessors.insert(class, m);
    m
}

/// `C$$$outer`, the name of the accessor of the enclosing instance of class `class`.
pub fn outer_accessor_name(l: &Lower, class: SymbolId) -> String {
    format!("{}$$$outer", l.u.binary_name(class).replace('.', "$"))
}
