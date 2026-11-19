// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/LazyVals.scala
//   compiler/src/dotty/tools/dotc/transform/MoveStatics.scala
//! The `VarHandle`s of lazy values on the JVM (dotty's `transformMemberDefThreadSafeNew`): a static
//! field per lazy value, set first in the class's `<clinit>` as `MoveStatics` sets `@static` fields.

use super::Lazy;
use crate::definitions::{LazyVals, applied, symbol, type_ref};
use crate::phases::lazy_init::Cas;
use crate::phases::lazy_trees::{assign, block, call, object};
use crate::phases::static_values::{clinit_def, clinit_symbol};
use crate::{Lower, Result};
use names::{name_kinds, nme};
use syntax::ast::Span;
use tasty::tags::{PRIVATE, STATIC, SYNTHETIC};
use tasty::{Flags, Kind, SymbolId, TypeId};
use typer::tpd::{TNodeId, Tree};

/// The number of arguments of a lazy value's `compareAndSet`: the object and the two values.
const CAS_ARITY: usize = 3;

/// A lazy value's static `VarHandle` field.
#[derive(Clone, Copy)]
pub struct VarHandle {
    pub symbol: SymbolId,
    /// The field's definition, without an initializer.
    pub def: TNodeId,
    /// `x$lzyN$lzyHandle = MethodHandles.lookup().findVarHandle(classOf[C], "x$lzyN",
    /// classOf[Object])`.
    pub init: TNodeId,
}

/// Defines the `VarHandle` field `x$lzyN$lzyHandle` of lazy value field `field` of `class`.
pub fn define(l: &mut Lower, class: SymbolId, field: SymbolId, defs: LazyVals) -> Result<VarHandle> {
    let jdk = l.var_handles()?;
    let field_name = l.u.symbol(field).name.clone();
    let name = name_kinds::lazy_var_handle(&field_name);
    let flags = Flags::default().with(PRIVATE).with(SYNTHETIC).with(STATIC);
    let handle = l.u.define(symbol(&name, Kind::Val, class, flags));
    let tpe = type_ref(l.u, defs.var_handle);
    l.u.set_info(handle, tpe);
    l.fields.insert(handle, handle);
    let tpt = l.type_tree(tpe, Span::NONE);
    let def = l.build().val_def(handle, tpt, None, Span::NONE);

    let module = l.ident(jdk.method_handles, Span::NONE);
    let lookup = call(l, module, jdk.lookup, &[]);
    let this_class = type_ref(l.u, class);
    let this_class = class_literal(l, this_class);
    let name = l.string_literal(&tasty::encode(&field_name));
    let object = object(l);
    let object = class_literal(l, object);
    let found = call(l, lookup, jdk.find_var_handle, &[this_class, name, object]);
    let init = assign(l, handle, found);
    Ok(VarHandle { symbol: handle, def, init })
}

/// Gives the definitions and initializers of `handles`, first in the body of `template`, the end
/// of its constructor and parents as their span, as copying the template without the self
/// `FirstTransform` drops does (dotty's `Positioned.envelope`).
pub fn position(l: &mut Lower, template: TNodeId, handles: &[VarHandle]) {
    let Tree::Template { constr, parents, .. } = *l.tpd.tree(template) else { return };
    let heads: Vec<TNodeId> = [constr].into_iter().chain(l.tpd.items(parents).iter().copied()).collect();
    let span = heads.into_iter().fold(Span::NONE, |s, t| s.union(l.tpd.span(t)));
    if !span.exists() || span.is_in_other_source() {
        return;
    }
    let at = span.end_pos().to_synthetic();
    for h in handles {
        l.positioned_from(h.def, at);
        l.positioned_from(h.init, at);
    }
}

/// `classOf[tpe]`.
fn class_literal(l: &mut Lower, tpe: TypeId) -> TNodeId {
    let class = l.u.class_ref(tasty::defn::CLASS);
    let class = l.u.class_symbol(class).expect("java.lang.Class");
    let class = applied(l.u, class, vec![tpe]);
    let literal = l.tpd.node(Tree::ClassLiteral { tpe }, Span::NONE, Some(class), None);
    l.positioned(literal)
}

/// Selects `handle.compareAndSet` at type `(Object, Object, Object): Boolean` (dotty's
/// `retypeSignaturePolymorphicFn`), to call on `this` of `lazy`'s class.
pub fn compare_and_set(l: &mut Lower, lazy: &Lazy, handle: SymbolId) -> Result<Cas> {
    let method = l.var_handles()?.compare_and_set;
    let object = object(l);
    let params: Vec<_> = (0..CAS_ARITY).map(|i| (l.u.intern_name(&format!("x${i}")), object)).collect();
    let boolean = l.u.class_ref(tasty::defn::BOOLEAN);
    let binder = l.u.fresh_binder();
    let info = l.u.intern_method(binder, &params, boolean, false, false);
    let instance = l.u.signature_polymorphic_instance(method, info);
    let qual = l.ident(handle, Span::NONE);
    let fun = l.select(qual, instance, Span::NONE);
    let fun = l.positioned(fun);
    let target = l.this(lazy.class, Span::NONE);
    Ok(Cas { fun, target })
}

/// Runs `inits` first in the `<clinit>` among `stats` of `class`, added if there is none.
pub fn initialize_first(l: &mut Lower, class: SymbolId, inits: &[TNodeId], stats: &mut Vec<TNodeId>) {
    if inits.is_empty() {
        return;
    }
    let is_clinit = |l: &Lower, s: TNodeId| {
        matches!(l.tpd.tree(s), Tree::DefDef { .. })
            && l.tpd.sym(s).is_some_and(|m| l.u.symbol(m).name == nme::STATIC_CONSTRUCTOR)
    };
    let Some(clinit) = stats.iter().copied().find(|&s| is_clinit(l, s)) else {
        let sym = clinit_symbol(l, class);
        stats.push(clinit_def(l, sym, inits, Span::NONE));
        return;
    };
    let Tree::DefDef { name, paramss, tpt, rhs: Some(rhs) } = *l.tpd.tree(clinit) else {
        unreachable!("a static initializer's body")
    };
    let rhs = block(l, inits, rhs);
    l.tpd.get_mut(clinit).tree = Tree::DefDef { name, paramss, tpt, rhs: Some(rhs) };
}
