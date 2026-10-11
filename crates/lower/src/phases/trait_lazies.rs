//! The lazy values and objects a class gets from its traits (dotty's `traitInits`, 4.2.1 and
//! 4.2.2), each made a lazy value of the class, its members named as `LazyVals` names them on
//! leaving the class.

use super::accessor_names::accessor_name;
use super::lazy_vals::{define_members, fresh, name_members};
use super::mixin::{has_body, mixins};
use super::trait_fields::{forwarder, implemented_by};
use crate::symbols::has;
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::SymbolId;
use tasty::tags::{FIELDaccessor, LAZY};
use typer::tpd::{TNodeId, Tree};

/// Whether class `class` gets a lazy value for member `getter` of a trait of the unit.
pub fn is_mixed_in_lazy(l: &mut Lower, class: SymbolId, getter: SymbolId) -> bool {
    if l.u.symbol(getter).tree.is_some() {
        return false;
    }
    l.u.is_module(getter) || has(l.u, getter, LAZY) && has_body(l, getter) && implemented_by(l, class, getter)
}

/// Names the fields and `$lzyINIT` methods of the lazy values class `class` gets from its traits.
pub fn name_mixed_in(l: &mut Lower, class: SymbolId) {
    for mixin in mixins(l, class) {
        for getter in l.u.decls(mixin).unwrap_or_default() {
            if is_mixed_in_lazy(l, class, getter) {
                let name = accessor_name(l, getter);
                let field = fresh(l, &format!("{name}$lzy"));
                let init = fresh(l, &format!("{name}$lzyINIT"));
                l.mixed_in_lazy_names.insert((class, getter), (field, init));
            }
        }
    }
}

/// The lazy object `x = new T$x$(this)` of class `class` for object `getter` of a trait.
pub fn module_forwarder(l: &mut Lower, class: SymbolId, getter: SymbolId, span: Span) -> Result<TNodeId> {
    let module_class = l.u.module_class(getter);
    let ctor = module_class.and_then(|c| l.u.decl(c, "<init>").unwrap_or_default().into_iter().next());
    let (Some(module_class), Some(ctor)) = (module_class, ctor) else {
        return unsupported(span, "objects in traits without their class");
    };
    let new = l.new_(module_class, Span::NONE);
    let this = l.this(class, Span::NONE);
    let rhs = l.call(new, ctor, &[this]);
    l.tpd.get_mut(rhs).tpe = l.tpd.get(new).tpe;
    let def = forwarder(l, class, getter, Some(rhs));
    let sym = l.tpd.sym(def).expect("a forwarder's symbol");
    let flags = l.u.symbol(sym).flags.with(LAZY).with(FIELDaccessor);
    l.u.set_flags(sym, flags);
    define(l, class, getter, sym, rhs);
    Ok(def)
}

/// The lazy value `lazy val x = super[M].x` of class `class` for lazy value `getter` of trait
/// `mixin`, the call at `at`.
pub fn lazy_forwarder(
    l: &mut Lower,
    class: SymbolId,
    mixin: SymbolId,
    getter: SymbolId,
    at: Span,
) -> TNodeId {
    let sup = l.super_mix(class, mixin, at);
    let fun = l.select(sup, getter, at);
    let info = l.u.info(getter);
    let result = l.u.result_type(info);
    let rhs = l.apply(fun, &[], result, at);
    let def = forwarder(l, class, getter, Some(rhs));
    if let Tree::DefDef { tpt, .. } = *l.tpd.tree(def) {
        l.tpd.get_mut(tpt).span = at;
    }
    let sym = l.tpd.sym(def).expect("a forwarder's symbol");
    define(l, class, getter, sym, rhs);
    def
}

/// Defines the members of lazy value `sym` of `class` for `getter`, with the names reserved
/// for them.
fn define(l: &mut Lower, class: SymbolId, getter: SymbolId, sym: SymbolId, rhs: TNodeId) {
    define_members(l, class, sym, rhs);
    let Some((field, init)) = l.mixed_in_lazy_names.remove(&(class, getter)) else {
        name_members(l, sym);
        return;
    };
    let members = l.lazy_members[&sym];
    l.u.rename(members.field, &field);
    l.u.rename(members.init, &init);
}
