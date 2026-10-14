//! dotty's `NonLocalReturns`: a return from a method other than the one it is in throws a
//! `NonLocalReturnControl` with the method's key, and the method catches it:
//! `{ val key = new Object(); try body catch { case ex: NonLocalReturnControl =>
//! if ex.key().eq(key) then ex.value().asInstanceOf[T] else throw ex } }`.

use super::walk::{subtrees, walk_class};
use crate::definitions::{symbol, type_ref};
use crate::symbols::has;
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashMap;
use syntax::ast::Span;
use tasty::tags::{CASE, LAZY, SYNTHETIC};
use tasty::{Flags, Kind, SymbolId};
use typer::tpd::{TNodeId, Tree, Walk};

/// Expands the non-local returns in class definition `td` and the methods they return from.
pub fn transform(l: &mut Lower, td: TNodeId) -> Result<()> {
    let mut keys: FxHashMap<SymbolId, SymbolId> = FxHashMap::default();
    let mut result = Ok(());
    walk_class(l, td, &mut |l, t, owner| {
        if let Tree::Return { expr, from } = *l.tpd.tree(t)
            && let Some(method) = l.tpd.sym(from).filter(|&m| l.u.symbol(m).kind == Kind::Def)
            && (Some(method) != enclosing_method(l, owner) || has(l.u, owner, LAZY))
        {
            let key = *keys.entry(method).or_insert_with(|| new_key(l, method));
            match nlrc(l) {
                Ok(class) => throw(l, t, expr, key, class),
                Err(e) => {
                    result = Err(e);
                    return Walk::Stop;
                }
            }
        }
        Walk::Children
    });
    result?;
    if keys.is_empty() {
        return Ok(());
    }
    let class = nlrc(l)?;
    for t in subtrees(l.tpd, td) {
        if let Tree::DefDef { rhs: Some(rhs), name, paramss, tpt } = *l.tpd.tree(t)
            && let Some(&key) = l.tpd.sym(t).and_then(|m| keys.get(&m))
        {
            let method = l.tpd.sym(t).expect("a method definition's symbol");
            let rhs = try_(l, rhs, key, method, class);
            l.tpd.get_mut(t).tree = Tree::DefDef { name, paramss, tpt, rhs: Some(rhs) };
            l.recopy(t);
        }
    }
    Ok(())
}

/// The method `owner` is in or is.
fn enclosing_method(l: &Lower, mut owner: SymbolId) -> Option<SymbolId> {
    loop {
        if l.u.symbol(owner).kind == Kind::Def {
            return Some(owner);
        }
        owner = l.u.symbol(owner).owner?;
    }
}

/// `scala.runtime.NonLocalReturnControl`.
fn nlrc(l: &mut Lower) -> Result<SymbolId> {
    let runtime = l.u.package("scala.runtime").expect("package scala.runtime");
    let found = l.u.decl(runtime, "NonLocalReturnControl").unwrap_or_default();
    match found.into_iter().find(|&c| l.u.symbol(c).kind == Kind::Class) {
        Some(c) => Ok(c),
        None => unsupported(Span::NONE, "scala.runtime.NonLocalReturnControl missing from the library"),
    }
}

/// The key `nonLocalReturnKeyN` of the non-local returns from `method`.
fn new_key(l: &mut Lower, method: SymbolId) -> SymbolId {
    let name = l.fresh.next(l.names, "nonLocalReturnKey");
    let name = l.names.text(name).to_owned();
    let key = l.u.define(symbol(&name, Kind::Val, method, Flags::default().with(SYNTHETIC)));
    let object = type_ref(l.u, l.defs.object);
    l.u.set_info(key, object);
    key
}

/// The member `name` of class `class`.
fn member(l: &mut Lower, class: SymbolId, name: &str) -> SymbolId {
    l.u.decl(class, name).unwrap_or_default().first().copied().expect("a NonLocalReturnControl member")
}

/// `qual.f` of value `name` of `class`, typed as seen from `qual`.
fn field(l: &mut Lower, qual: TNodeId, class: SymbolId, name: &str) -> TNodeId {
    let f = member(l, class, name);
    let prefix = l.tpd.tpe(qual);
    let tpe = l.u.member_info(prefix, f);
    let sel = l.select(qual, f, Span::NONE);
    l.tpd.get_mut(sel).tpe = Some(tpe);
    l.positioned(sel)
}

/// Makes return `t` of `expr` `throw new NonLocalReturnControl(key, expr)`, at the return's span.
fn throw(l: &mut Lower, t: TNodeId, expr: TNodeId, key: SymbolId, class: SymbolId) {
    let span = l.tpd.span(t);
    let new = l.new_(class, Span::NONE);
    let ctor = member(l, class, "<init>");
    let key = l.ident(key, Span::NONE);
    let created = l.call(new, ctor, &[key, expr]);
    let tpe = type_ref(l.u, class);
    l.tpd.get_mut(created).tpe = Some(tpe);
    let thrown = l.throw_(created);
    l.copy_node(t, thrown);
    l.tpd.get_mut(t).span = span;
}

/// `body` of `method` in a block defining `key` and catching the returns with it.
fn try_(l: &mut Lower, body: TNodeId, key: SymbolId, method: SymbolId, class: SymbolId) -> TNodeId {
    let object = l.defs.object;
    let new = l.new_(object, Span::NONE);
    let ctor = member(l, object, "<init>");
    let created = l.call(new, ctor, &[]);
    let object_type = type_ref(l.u, object);
    l.tpd.get_mut(created).tpe = Some(object_type);
    let key_tpt = l.type_tree(object_type, Span::NONE);
    let key_def = l.build().val_def(key, key_tpt, Some(created), Span::NONE);
    let key_def = l.positioned(key_def);
    let nlrc = type_ref(l.u, class);
    let ex = l.u.define(symbol("ex", Kind::Val, method, Flags::default().with(CASE)));
    l.u.set_info(ex, nlrc);
    let wildcard = l.wildcard(nlrc);
    let tpt = l.type_tree(nlrc, Span::NONE);
    let typed = l.tpd.node(Tree::Typed { expr: wildcard, tpt }, Span::NONE, Some(nlrc), None);
    let typed = l.positioned(typed);
    let ex_name = l.term_name("ex");
    let pat = l.tpd.node(Tree::Bind { name: ex_name, body: typed }, Span::NONE, Some(nlrc), Some(ex));
    let pat = l.positioned(pat);
    let ex_ref = l.ident(ex, Span::NONE);
    let key_of = field(l, ex_ref, class, "key");
    let key_ref = l.ident(key, Span::NONE);
    let eq = l.defs.eq;
    let same = l.call(key_of, eq, &[key_ref]);
    let ex_ref = l.ident(ex, Span::NONE);
    let value = field(l, ex_ref, class, "value");
    let info = l.u.info(method);
    let result = l.u.result_type(info);
    let value = if l.u.erase(result) == tasty::Erased::Unit {
        let unit = l.unit_literal(Span::NONE);
        let block = l.block(&[value], unit, Span::NONE);
        l.positioned(block)
    } else {
        l.ensure_conforms(value, result)
    };
    let ex_ref = l.ident(ex, Span::NONE);
    let rethrow = l.throw_(ex_ref);
    let cond = l.tpd.node(
        Tree::If { cond: same, thenp: value, elsep: rethrow, inline: false },
        Span::NONE,
        Some(result),
        None,
    );
    let cond = l.positioned(cond);
    let case = l.tpd.node(Tree::CaseDef { pat, guard: None, body: cond }, Span::NONE, Some(result), None);
    let case = l.positioned(case);
    let cases = l.tpd.list(&[case]);
    let tried = l.tpd.node(Tree::Try { expr: body, cases, finalizer: None }, Span::NONE, Some(result), None);
    let tried = l.positioned(tried);
    let block = l.block(&[key_def], tried, Span::NONE);
    l.positioned(block)
}
