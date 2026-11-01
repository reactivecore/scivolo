// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymUtils.scala
//   compiler/src/dotty/tools/dotc/transform/SyntheticMembers.scala
//! The mirror support dotty's `SyntheticMembers` gives objects (`addMirrorSupport`): the
//! companion of a case class extends `Mirror.Product` with `fromProduct`, a case object extends
//! `Mirror.Singleton`.

use super::anon_mirrors::anon_mirror_support;
use super::case_members::existing_def;
use super::enum_values::is_enum_value;
use super::from_product::from_product;
use super::sum_mirrors::{enum_ordinal, is_generic_sum};
use super::walk::subtrees;
use crate::definitions::{symbol, type_ref};
use crate::symbols::has;
use crate::{Lower, Result};
use syntax::ast::{Span, TName};
use tasty::tags::{CASE, ENUM, SEALED, SYNTHETIC};
use tasty::{Flags, Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// The parent object class `class` gets and its members, at `span`'s point.
pub fn mirror_support(
    l: &mut Lower,
    root: TNodeId,
    class: SymbolId,
    span: Span,
) -> Result<Option<(SymbolId, Vec<TNodeId>)>> {
    if !l.before_inlining
        && let Ok(cc) = l.case_classes()
        && [cc.mirror_product, cc.mirror_singleton, cc.mirror_sum].iter().any(|&m| l.u.derives_from(class, m))
    {
        return Ok(None);
    }
    if let Some(kind) = l.tpd.anon_mirror(class) {
        return anon_mirror_support(l, root, class, kind, span).map(Some);
    }
    if is_enum_value(l, class) {
        return Ok(Some((l.case_classes()?.mirror_singleton, Vec::new())));
    }
    if !l.u.is_module_class(class) {
        return Ok(None);
    }
    if has(l.u, class, CASE) {
        return Ok(Some((l.case_classes()?.mirror_singleton, Vec::new())));
    }
    let Some(linked) = linked_class(l, root, class) else { return Ok(None) };
    let focus = Span::new(span.point(), span.point(), span.point());
    let is_enum = has(l.u, linked, SEALED) && has(l.u, linked, ENUM);
    let product = !is_enum && is_generic_product(l, linked);
    if !is_enum && !product && !(has(l.u, linked, SEALED) && is_generic_sum(l, root, linked)) {
        return Ok(None);
    }
    let mono = mono_type(l, class, linked, focus);
    let cc = l.case_classes()?;
    let (parent, method) = if is_enum {
        (cc.mirror_sum, enum_ordinal(l, class, linked, focus)?)
    } else if product {
        (cc.mirror_product, from_product(l, class, linked, None, focus)?)
    } else {
        let param = type_ref(l.u, linked);
        (cc.mirror_sum, super::sum_mirrors::ordinal(l, root, (class, linked), param, focus)?)
    };
    Ok(Some((parent, mono.into_iter().chain(method).collect())))
}

/// Whether `class` has a concrete method `like`'s name of type `info` (dotty's `existingDef` in
/// `addMethod`).
pub fn has_mirror_method(l: &mut Lower, class: SymbolId, like: SymbolId, info: TypeId) -> bool {
    let name = l.u.symbol(like).name.clone();
    let sym = l.u.define(symbol(&name, Kind::Def, class, Flags::default().with(SYNTHETIC)));
    l.u.set_info(sym, info);
    let existing = existing_def(l, class, sym).is_some();
    l.u.unenter(sym);
    existing
}

/// Whether `c` is a case class dotty gives a product mirror (`isGenericProduct`).
pub fn is_generic_product(l: &mut Lower, c: SymbolId) -> bool {
    let value_class = l.u.underlying(c).is_some();
    typer::sealed::is_product_shape(l.u, c, value_class)
}

/// `type MirroredMonoType = C` in object class `class` for its companion class `linked`, at
/// `focus`, unless `class` has such an alias.
fn mono_type(l: &mut Lower, class: SymbolId, linked: SymbolId, focus: Span) -> Option<TNodeId> {
    if l.u.member(class, "MirroredMonoType").into_iter().any(|m| l.u.alias_of(m).is_some()) {
        return None;
    }
    let alias = symbol("MirroredMonoType", Kind::Type, class, Flags::default().with(SYNTHETIC));
    let alias = l.u.define(alias);
    let linked = type_ref(l.u, linked);
    let info = l.u.intern_type(Type::Alias(linked));
    l.u.set_info(alias, info);
    let rhs = l.type_tree(info, focus);
    let name = TName { name: l.names.intern("MirroredMonoType"), is_type: true };
    Some(l.tpd.node(Tree::TypeDef { name, rhs }, focus, None, Some(alias)))
}

/// The companion class of object class `class` of unit `root`; of a local one, the class of its
/// name among the statements that define it (dotty's `companionNamed`).
fn linked_class(l: &mut Lower, root: TNodeId, class: SymbolId) -> Option<SymbolId> {
    let owner = l.u.symbol(class).owner?;
    if !l.u.symbol(owner).kind.is_term() {
        return l.u.companion_class(class);
    }
    let name = tasty::module_name(&l.u.symbol(class).name)?.to_owned();
    let defined_in = |l: &Lower, stats| l.tpd.items(stats).iter().any(|&s| l.tpd.sym(s) == Some(class));
    let stats = subtrees(l.tpd, root).into_iter().find_map(|t| match *l.tpd.tree(t) {
        Tree::Block { stats, .. } if defined_in(l, stats) => Some(l.tpd.items(stats).to_vec()),
        _ => None,
    })?;
    let linked = |s: SymbolId| l.u.symbol(s).name == name && l.u.is_class(s) && !l.u.is_module_class(s);
    stats.into_iter().filter_map(|s| l.tpd.sym(s)).find(|&s| linked(s))
}
