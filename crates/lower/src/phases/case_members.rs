//! The members dotty's `SyntheticMembers` gives case classes and case objects, each unless the
//! class has one: `hashCode`, `equals` (not for objects), `toString`, `canEqual`,
//! `productArity`, `productPrefix`, `productElement`, `productElementName`.

use super::value_members::hash_code_body as value_hash_code_body;
use crate::definitions::type_ref;
use crate::symbols::has;
use crate::{Lower, Result};
use syntax::ast::Span;
use tasty::tags::{CASE, CASEaccessor, OVERRIDE, SYNTHETIC};
use tasty::{Flags, Kind, SymbolId};
use typer::tpd::{MatchKind, TNodeId, Tree};

/// The synthetic members of case class or object `class`, at `span`'s point.
pub fn case_members(l: &mut Lower, class: SymbolId, span: Span) -> Result<Vec<TNodeId>> {
    if !has(l.u, class, CASE) {
        return Ok(Vec::new());
    }
    let cc = l.defs.case_classes;
    let module = l.u.is_module_class(class);
    let members = [
        cc.hash_code,
        l.defs.equality.any_equals,
        cc.to_string,
        cc.can_equal,
        cc.product_arity,
        cc.product_prefix,
        cc.product_element,
        cc.product_element_name,
    ];
    let focus = Span::new(span.point(), span.point(), span.point());
    let mut out = Vec::new();
    let can_equal = existing_def(l, class, cc.can_equal);
    for sym in members {
        if module && sym == l.defs.equality.any_equals || existing_def(l, class, sym).is_some() {
            continue;
        }
        let flags = Flags::default().with(SYNTHETIC).with(OVERRIDE);
        let info = l.u.info(sym);
        let mut locals = Vec::new();
        let (method, def) = l.method_like(class, sym, flags, info, focus, |l, args, _| {
            body(l, class, sym, args, can_equal, &mut locals)
        })?;
        for local in locals {
            l.u.set_owner(local, method);
        }
        l.tpd.get_mut(def).span = Span::NONE;
        out.push(l.with_span(def, focus));
    }
    Ok(out)
}

/// The body of synthetic member `sym` of `class` with parameters `args`, `can_equal` the
/// `canEqual` the class has; the locals it defines go to `locals`.
fn body(
    l: &mut Lower,
    class: SymbolId,
    sym: SymbolId,
    args: &[TNodeId],
    can_equal: Option<SymbolId>,
    locals: &mut Vec<SymbolId>,
) -> TNodeId {
    let cc = l.defs.case_classes;
    let accessors = case_accessors(l, class);
    let own_name = own_name(l, class);
    match sym {
        _ if sym == cc.hash_code => match l.u.underlying(class) {
            Some(value) => value_hash_code_body(l, class, value),
            None => l.hash_code_body(class, &accessors, &own_name, locals),
        },
        _ if sym == l.defs.equality.any_equals => {
            l.equals_body(class, &accessors, args[0], can_equal, locals)
        }
        _ if sym == cc.to_string && l.u.is_module_class(class) => l.string_literal(&own_name),
        _ if sym == cc.to_string => {
            let run_time = l.ident(cc.scala_run_time, Span::NONE);
            let this = l.this(class, Span::NONE);
            l.call(run_time, cc.runtime_to_string, &[this])
        }
        _ if sym == cc.can_equal => {
            let tpe = type_ref(l.u, class);
            l.is_instance(args[0], tpe)
        }
        _ if sym == cc.product_arity => l.int_literal(accessors.len() as i32),
        _ if sym == cc.product_prefix => l.string_literal(&own_name),
        _ if sym == cc.product_element => {
            let elems: Vec<TNodeId> = (1..=accessors.len())
                .map(|i| {
                    let this = l.this(class, Span::NONE);
                    let sel = l.u.decl(class, &format!("_{i}")).unwrap_or_default();
                    let sel = sel.into_iter().find(|&s| l.u.symbol(s).kind.is_term()).expect("a selector");
                    let info = l.u.info(sel);
                    let result = l.u.result_type(info);
                    let select = l.select(this, sel, Span::NONE);
                    l.tpd.get_mut(select).tpe = Some(result);
                    select
                })
                .collect();
            index_match(l, args[0], elems)
        }
        _ => {
            let names: Vec<String> = accessors.iter().map(|&a| l.u.symbol(a).name.clone()).collect();
            let names: Vec<TNodeId> = names.iter().map(|n| l.string_literal(n)).collect();
            index_match(l, args[0], names)
        }
    }
}

/// `index match { case 0 => elems(0) ... case _ => throw new IndexOutOfBoundsException(index) }`.
fn index_match(l: &mut Lower, index: TNodeId, elems: Vec<TNodeId>) -> TNodeId {
    let mut cases = Vec::new();
    for (i, body) in elems.into_iter().enumerate() {
        let pat = l.int_literal(i as i32);
        cases.push(l.tpd.node(Tree::CaseDef { pat, guard: None, body }, Span::NONE, None, None));
    }
    let cc = l.defs.case_classes;
    let new = l.new_(cc.ioobe, Span::NONE);
    let index_ref = l.copy_ident(index);
    let error = l.call(new, cc.ioobe_ctor, &[index_ref]);
    l.tpd.get_mut(error).tpe = l.tpd.get(new).tpe;
    let throw = l.throw_(error);
    let int = l.u.class_ref("scala.Int");
    let pat = l.wildcard(int);
    cases.push(l.tpd.node(Tree::CaseDef { pat, guard: None, body: throw }, Span::NONE, None, None));
    let cases = l.tpd.list(&cases);
    let result = l.cases_lub(cases);
    let m = l.tpd.node(
        Tree::Match { selector: index, cases, kind: MatchKind::Plain },
        Span::NONE,
        Some(result),
        None,
    );
    l.positioned(m)
}

/// The values the parameters of case class `class` define, in order (dotty's `caseAccessors`).
pub fn case_accessors(l: &mut Lower, class: SymbolId) -> Vec<SymbolId> {
    let decls = l.u.decls(class).unwrap_or_default();
    let value = |l: &Lower, d| matches!(l.u.symbol(d).kind, Kind::Val | Kind::Param);
    decls.into_iter().filter(|&d| value(l, d) && has(l.u, d, CASEaccessor)).collect()
}

/// The name of `class` without the `$` of an object's class.
fn own_name(l: &Lower, class: SymbolId) -> String {
    let name = &l.u.symbol(class).name;
    tasty::module_name(name).unwrap_or(name).to_owned()
}

/// The implementation of `sym` other than `sym` that `class` has or inherits (dotty's
/// `existingDef`): the first concrete method or value of its name and erased parameters in its
/// linearization.
pub fn existing_def(l: &mut Lower, class: SymbolId, sym: SymbolId) -> Option<SymbolId> {
    let name = l.u.symbol(sym).name.clone();
    let params = l.u.erased_method(sym).0;
    for base in l.u.linearization(class) {
        for other in l.u.decl(base, &name).unwrap_or_default() {
            let member = matches!(l.u.symbol(other).kind, Kind::Def | Kind::Val);
            if !member || l.u.erased_method(other).0 != params {
                continue;
            }
            if other == sym {
                return None;
            }
            if super::mixin::has_body(l, other) {
                return Some(other);
            }
        }
    }
    None
}
