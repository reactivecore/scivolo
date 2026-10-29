// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymUtils.scala
//   compiler/src/dotty/tools/dotc/transform/SyntheticMembers.scala
//! Sum mirrors of the companions of sealed classes other than enums (dotty's `isGenericSum` and
//! `ordinalBody`): `def ordinal(x$0: S): Int = x$0 match { case _: C1 => 0; ... }`.

use super::mirrors::{class_prefix, is_generic_product};
use super::walk::class_defs;
use crate::definitions::type_ref;
use crate::symbols::has;
use crate::{Lower, Result};
use syntax::ast::Span;
use syntax::ast::tree::MatchKind;
use tasty::tags::{ABSTRACT, ENUM, SEALED, SYNTHETIC};
use tasty::{Flags, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// The children of sealed class `sealed` in unit `root` in the order of their definitions:
/// classes, and objects by their values; of an enum, its cases.
fn children(l: &mut Lower, root: TNodeId, sealed: SymbolId) -> Vec<SymbolId> {
    if has(l.u, sealed, ENUM) {
        return typer::sealed::enum_cases(l.u, sealed);
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

/// `def ordinal(x$0: P): Int` of class `class` for sealed class `sealed` of unit `root`, `P` the
/// type `param`.
pub fn ordinal(
    l: &mut Lower,
    root: TNodeId,
    (class, sealed): (SymbolId, SymbolId),
    param: TypeId,
    focus: Span,
) -> Result<TNodeId> {
    let cc = l.case_classes()?;
    let children = children(l, root, sealed);
    let int = l.u.class_ref("scala.Int");
    let binder = l.u.fresh_binder();
    let params = vec![("x$0".into(), param)];
    let info =
        l.u.intern_type(Type::Method { binder, params, result: int, implicit: false, contextual: false });
    let like = l.u.decl(cc.mirror_sum, "ordinal").unwrap_or_default();
    let like = like.into_iter().next().expect("`Mirror.Sum.ordinal`");
    let flags = Flags::default().with(SYNTHETIC);
    let pre = class_prefix(l, param, sealed);
    let (_, def) = l.method_like(class, like, flags, info, focus, |l, args, _| {
        let mut cases = Vec::new();
        for (i, &c) in children.iter().enumerate() {
            let tpe = match pre.and_then(|pre| child_prefix(l, pre, sealed, c)) {
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
    Ok(l.with_span(def, focus))
}

/// The prefix of child `c` of sealed class `sealed` of prefix `pre`: `pre` up to the common
/// owner, then the objects down to `c` (dotty's `childPrefix`); `None` where it is no such path.
fn child_prefix(l: &mut Lower, pre: TypeId, sealed: SymbolId, c: SymbolId) -> Option<TypeId> {
    let owners = |l: &Lower, s: SymbolId| -> Vec<SymbolId> {
        let mut v: Vec<SymbolId> = std::iter::successors(l.u.symbol(s).owner, |&o| l.u.symbol(o).owner).collect();
        v.reverse();
        v
    };
    let (parents, children) = (owners(l, sealed), owners(l, c));
    let common = parents.iter().zip(&children).take_while(|(a, b)| a == b).count();
    let mut prefix = pre;
    for _ in common..parents.len() {
        let Type::Sym(Some(q), _) = *l.u.types().get(prefix) else { return None };
        prefix = q;
    }
    for &o in &children[common..] {
        let m = l.u.source_module(o).filter(|_| l.u.is_module_class(o))?;
        prefix = l.u.intern_type(Type::Sym(Some(prefix), m));
    }
    Some(prefix)
}
