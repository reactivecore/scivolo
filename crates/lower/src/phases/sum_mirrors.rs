//! Sum mirrors of the companions of sealed classes other than enums (dotty's `isGenericSum` and
//! `ordinalBody`): `def ordinal(x$0: S): Int = x$0 match { case _: C1 => 0; ... }`.

use super::local_free::within;
use super::mirrors::is_generic_product;
use super::walk::class_defs;
use crate::definitions::type_ref;
use crate::symbols::has;
use crate::{Lower, Result};
use syntax::ast::Span;
use syntax::ast::tree::MatchKind;
use tasty::tags::{ABSTRACT, CASE, ENUM, SEALED, SYNTHETIC};
use tasty::{Flags, Kind, SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

/// The children of sealed class `sealed` in unit `root` in the order of their definitions:
/// classes, and objects by their values; of an enum, its cases.
fn children(l: &mut Lower, root: TNodeId, sealed: SymbolId) -> Vec<SymbolId> {
    if has(l.u, sealed, ENUM) {
        let companion = l.u.companion_module(sealed).and_then(|m| l.u.module_class(m));
        let decls = companion.map(|c| l.u.symbol(c).decls.clone()).unwrap_or_default();
        return decls
            .into_iter()
            .filter(|&d| {
                let kind = l.u.symbol(d).kind;
                let class_or_value =
                    kind == Kind::Class && !l.u.is_module_class(d) || kind == Kind::Val && !l.u.is_module(d);
                class_or_value && has(l.u, d, CASE) && has(l.u, d, ENUM)
            })
            .collect();
    }
    let mut out = Vec::new();
    for td in class_defs(l.tpd, root) {
        let Some(c) = l.tpd.sym(td) else { continue };
        if !l.u.parents(c).into_iter().any(|p| l.u.class_symbol(p) == Some(sealed)) {
            continue;
        }
        let module = l.u.is_module_class(c).then(|| l.u.source_module(c)).flatten();
        out.push(module.unwrap_or(c));
    }
    out
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
            let visible = l.u.symbol(c).owner.is_some_and(|o| visible_to_parent(l, s, o));
            let fits = !l.u.is_class(c) || is_generic_product(l, c) || is_generic_sum(l, root, c);
            !anonymous && visible && fits
        })
}

/// Whether `owner`, where a child of `s` is defined, encloses `s` or is an object visible so.
fn visible_to_parent(l: &Lower, s: SymbolId, owner: SymbolId) -> bool {
    within(l, s, owner)
        || l.u.is_module_class(owner) && l.u.symbol(owner).owner.is_some_and(|o| visible_to_parent(l, s, o))
}

/// `def ordinal(x$0: S): Int` of object class `class` for its companion `sealed` of unit `root`.
pub fn ordinal(
    l: &mut Lower,
    root: TNodeId,
    class: SymbolId,
    sealed: SymbolId,
    focus: Span,
) -> Result<TNodeId> {
    let cc = l.case_classes()?;
    let children = children(l, root, sealed);
    let param = type_ref(l.u, sealed);
    let int = l.u.class_ref("scala.Int");
    let binder = l.u.fresh_binder();
    let params = vec![("x$0".into(), param)];
    let info =
        l.u.intern_type(Type::Method { binder, params, result: int, implicit: false, contextual: false });
    let like = l.u.decl(cc.mirror_sum, "ordinal").unwrap_or_default();
    let like = like.into_iter().next().expect("`Mirror.Sum.ordinal`");
    let flags = Flags::default().with(SYNTHETIC);
    let (_, def) = l.method_like(class, like, flags, info, focus, |l, args, _| {
        let mut cases = Vec::new();
        for (i, &c) in children.iter().enumerate() {
            let tpe = type_ref(l.u, c);
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
