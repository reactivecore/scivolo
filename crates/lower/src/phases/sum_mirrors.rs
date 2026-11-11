// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymUtils.scala
//   compiler/src/dotty/tools/dotc/transform/SyntheticMembers.scala
//! Sum mirrors of the companions of sealed classes other than enums (dotty's `isGenericSum` and
//! `ordinalBody`): `def ordinal(x$0: S): Int = x$0 match { case _: C1 => 0; ... }`.

use super::casts::class_named;
use super::mirror_prefixes::{child_prefix, class_prefix};
use super::mirrors::{has_mirror_method, is_generic_product};
use super::walk::class_defs;
use crate::definitions::{Known, type_ref};
use crate::symbols::has;
use crate::{Lower, Result};
use syntax::ast::Span;
use syntax::ast::tree::MatchKind;
use tasty::tags::{ABSTRACT, ENUM, SEALED, SYNTHETIC};
use tasty::{Flags, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// The children of sealed class `sealed` in unit `root` in the order of their definitions:
/// classes, and objects by their values; of an enum, its cases; of the library, those its TASTy
/// names; of `*:`, `Tuple1` to `Tuple22` (dotty's `adjustForTuple` makes them its children) by
/// the start of their definitions in their own files (`Namer.addChild`).
fn children(l: &mut Lower, root: TNodeId, sealed: SymbolId) -> Vec<SymbolId> {
    if has(l.u, sealed, ENUM) {
        return typer::sealed::enum_cases(l.u, sealed);
    }
    if let Some(types) = l.u.decoded_children(sealed) {
        let referenced = |l: &mut Lower, t: TypeId| l.u.sym_of(t).or_else(|| l.u.resolve_ref(t));
        return types.into_iter().filter_map(|t| referenced(l, t)).collect();
    }
    if l.is_known(sealed, Known::Pair) {
        let mut tuples: Vec<SymbolId> =
            (1..=22).filter_map(|n| class_named(l, &format!("scala.Tuple{n}"))).collect();
        tuples.sort_by_cached_key(|&c| l.u.pickled_span(c).map(|s| s.start));
        return tuples;
    }
    let classes: Vec<(u32, SymbolId)> = class_defs(l.tpd, root)
        .into_iter()
        .filter_map(|td| Some((l.tpd.span(td).start(), l.tpd.sym(td)?)))
        .collect();
    typer::sealed::direct_children(l.u, sealed, &classes)
}

/// Whether sealed class `s` of unit `root` is a sum dotty gives a mirror (`isGenericSum`):
/// abstract, with children, none anonymous, each visible from `s`, an object, a generic product
/// or such a sum.
pub fn is_generic_sum(l: &mut Lower, root: TNodeId, s: SymbolId) -> bool {
    if !has(l.u, s, SEALED) || !(has(l.u, s, ABSTRACT) || l.u.is_trait(s)) {
        return false;
    }
    let children = children(l, root, s);
    !children.is_empty()
        && children.into_iter().all(|c| {
            let anonymous = l.u.symbol(c).name.starts_with("$anon");
            let visible = l.u.symbol(c).owner.is_some_and(|o| typer::sealed::visible_to_parent(l.u, s, o));
            let fits = !l.u.is_class(c) || is_generic_product(l, c) || is_generic_sum(l, root, c);
            !anonymous && visible && fits
        })
}

/// `def ordinal(x$0: P): Int = x$0.ordinal` for enum `enum_class`, `P` the type `param` (dotty's
/// `ordinalBody`); none if `class` has one.
pub fn enum_ordinal(
    l: &mut Lower,
    class: SymbolId,
    (enum_class, param): (SymbolId, TypeId),
    focus: Span,
) -> Result<Option<TNodeId>> {
    let cc = l.case_classes()?;
    let int = l.u.class_ref(tasty::defn::INT);
    let binder = l.u.fresh_binder();
    let params = [(l.u.intern_name("x$0"), param)];
    let info = l.u.intern_method(binder, &params, int, false, false);
    let like = l.u.decls_named(cc.mirror_sum, "ordinal");
    let like = like.into_iter().next().expect("`Mirror.Sum.ordinal`");
    if has_mirror_method(l, class, like, info) {
        return Ok(None);
    }
    let flags = Flags::default().with(SYNTHETIC);
    let java = l.u.linearization(enum_class).into_iter().find(|&b| l.is_known(b, Known::JavaEnum));
    let java_ordinal = java.and_then(|j| l.u.decl(j, "ordinal").ok()?.into_iter().next());
    let (_, def) = l.method_like(class, like, flags, info, focus, |l, args, _| {
        let x = l.copy_ident(args[0]);
        l.call(x, java_ordinal.unwrap_or(cc.enum_ordinal), &[])
    })?;
    l.tpd.get_mut(def).span = Span::NONE;
    Ok(Some(l.with_span(def, focus)))
}

/// `def ordinal(x$0: P): Int` of class `class` for sealed class `sealed` of unit `root`, `P` the
/// type `param`; none if `class` has one.
pub fn ordinal(
    l: &mut Lower,
    root: TNodeId,
    (class, sealed): (SymbolId, SymbolId),
    param: TypeId,
    focus: Span,
) -> Result<Option<TNodeId>> {
    let cc = l.case_classes()?;
    let children = children(l, root, sealed);
    let int = l.u.class_ref(tasty::defn::INT);
    let binder = l.u.fresh_binder();
    let params = [(l.u.intern_name("x$0"), param)];
    let info = l.u.intern_method(binder, &params, int, false, false);
    let like = l.u.decls_named(cc.mirror_sum, "ordinal");
    let like = like.into_iter().next().expect("`Mirror.Sum.ordinal`");
    if has_mirror_method(l, class, like, info) {
        return Ok(None);
    }
    let flags = Flags::default().with(SYNTHETIC);
    let local = l.u.symbol(sealed).owner.is_some_and(|o| l.u.symbol(o).kind.is_term());
    let no_prefix = local.then(|| l.u.intern_type(Type::NoPrefix));
    let pre = class_prefix(l, param, sealed).or(no_prefix);
    let (_, def) = l.method_like(class, like, flags, info, focus, |l, args, _| {
        let mut cases = Vec::new();
        for (i, &c) in children.iter().enumerate() {
            let tpe = match pre.and_then(|pre| child_prefix(l, pre, (sealed, c), class)) {
                Some(pre) => l.u.intern_type(Type::Sym(Some(pre), c)),
                None => type_ref(l.u, c),
            };
            let name = l.term_name("_");
            let wildcard = l.tpd.node(Tree::Ident { name }, Span::NONE, Some(tpe), None);
            let tpt = l.type_tree(tpe, Span::NONE);
            let pat = l.tpd.node(Tree::Typed { expr: wildcard, tpt }, Span::NONE, Some(tpe), None);
            let body = l.int_literal(i as i32);
            let case = l.tpd.node(Tree::CaseDef { pat, guard: None, body }, Span::NONE, Some(int), None);
            cases.push(l.positioned(case));
        }
        let selector = l.copy_ident(args[0]);
        let cases = l.tpd.list(&cases);
        let m = Tree::Match { selector: Some(selector), cases, kind: MatchKind::Plain };
        let m = l.tpd.node(m, Span::NONE, Some(int), None);
        l.positioned(m)
    })?;
    l.tpd.get_mut(def).span = Span::NONE;
    Ok(Some(l.with_span(def, focus)))
}
