//! dotty's `ResolveSuper`: a class implements the super accessors of the traits it mixes in,
//! each calling the member it accesses in the first base class after the trait that has one.

use super::mixin::{has_body, mixins};
use crate::symbols::has;
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::SymbolId;
use tasty::tags::PRIVATE;
use typer::tpd::{TNodeId, Tree};

/// Adds to class definition `td` the implementations of the super accessors of the traits its
/// class mixes in, first in its body.
pub fn transform(l: &mut Lower, td: TNodeId) -> Result<()> {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    if l.u.is_trait(class) {
        return Ok(());
    }
    let span = l.tpd.span(td);
    let at = if span.exists() { Span::at(span.point()) } else { span };
    let mut defs = Vec::new();
    for mixin in mixins(l, class) {
        for acc in l.u.decls(mixin).unwrap_or_default() {
            if !l.super_accessors.contains_key(&acc) {
                if l.u.symbol(mixin).tree.is_some() && l.u.symbol(acc).name.contains("$$super$") {
                    return unsupported(span, "super accessors of library traits");
                }
                continue;
            }
            defs.push(implementation(l, class, acc, at)?);
        }
    }
    if defs.is_empty() {
        return Ok(());
    }
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { constr, parents, self_, body } = *l.tpd.tree(template) else { return Ok(()) };
    defs.extend_from_slice(l.tpd.items(body));
    let body = l.tpd.list(&defs);
    l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
    Ok(())
}

/// `def T$$super$m(params) = C.super[S].m(params)` in `class` for super accessor `acc`, at `span`.
fn implementation(l: &mut Lower, class: SymbolId, acc: SymbolId, span: Span) -> Result<TNodeId> {
    let target = rebind_super(l, class, acc, span)?;
    let owner = l.u.symbol(target).owner.expect("a member's owner");
    let flags = l.u.symbol(acc).flags;
    let this = l.u.this_type(class);
    let info = l.u.member_info(this, acc);
    let (_, def) = l.method_like(class, acc, flags, info, span, |l, args, result| {
        let sup = l.super_mix(class, owner, span);
        let fun = l.select(sup, target, span);
        l.apply(fun, args, result, span)
    })?;
    Ok(def)
}

/// The member super accessor `acc` calls in `base`: the one of the named mixin, else the first
/// concrete one of the accessed member's name in the base classes of `base` after the accessor's
/// trait whose type seen from `base` matches the accessor's, bridges left out.
fn rebind_super(l: &mut Lower, base: SymbolId, acc: SymbolId, span: Span) -> Result<SymbolId> {
    let (member, mixed) = l.super_accessors[&acc];
    if mixed {
        return Ok(member);
    }
    let owner = l.u.symbol(acc).owner;
    let name = l.u.symbol(member).name.clone();
    let site = l.u.this_type(base);
    let bases = l.u.linearization(base);
    for &b in bases.iter().skip_while(|&&b| Some(b) != owner).skip(1) {
        for other in l.u.decl(b, &name).unwrap_or_default() {
            let candidate = !has(l.u, other, PRIVATE) && !l.bridges.contains(&other) && has_body(l, other);
            if candidate && l.u.matches(site, other, acc) {
                return Ok(other);
            }
        }
    }
    unsupported(span, "a super accessor without a concrete member")
}
