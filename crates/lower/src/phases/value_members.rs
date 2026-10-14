//! The members dotty's `SyntheticMembers` gives value classes, each unless the class has one:
//! `hashCode` (`valueHashCodeBody`) and `equals` of the underlying value.

use super::case_members::existing_def;
use crate::definitions::CaseClasses;
use crate::symbols::has;
use crate::{Lower, Result};
use syntax::ast::Span;
use tasty::tags::{CASE, OVERRIDE, SYNTHETIC};
use tasty::{Erased, Flags, SymbolId};
use typer::tpd::TNodeId;

/// The synthetic members of value class `class`, at `span`'s point.
pub fn value_members(l: &mut Lower, class: SymbolId, span: Span) -> Result<Vec<TNodeId>> {
    let plain = !l.u.is_trait(class) && !has(l.u, class, CASE);
    let Some(value) = l.u.underlying(class).filter(|_| plain) else { return Ok(Vec::new()) };
    let cc = l.case_classes()?;
    let can_equal = existing_def(l, class, cc.can_equal);
    let focus = Span::new(span.point(), span.point(), span.point());
    let mut out = Vec::new();
    for sym in [cc.hash_code, cc.any_equals] {
        if existing_def(l, class, sym).is_some() {
            continue;
        }
        let flags = Flags::default().with(SYNTHETIC).with(OVERRIDE);
        let info = l.u.info(sym);
        let mut locals = Vec::new();
        let (method, def) =
            l.method_like(class, sym, flags, info, focus, |l, args, _| match sym == cc.hash_code {
                true => hash_code_body(l, class, value, cc),
                false => l.equals_body(class, &[value], args[0], can_equal, &mut locals, cc),
            })?;
        for local in locals {
            l.u.set_owner(local, method);
        }
        l.tpd.get_mut(def).span = Span::NONE;
        out.push(l.with_span(def, focus));
    }
    Ok(out)
}

/// `value.hashCode()` for a primitive `value` of `class`, else `java.util.Objects.hashCode(value)`.
pub fn hash_code_body(l: &mut Lower, class: SymbolId, value: SymbolId, cc: CaseClasses) -> TNodeId {
    let this = l.this(class, Span::NONE);
    let value_ref = l.select(this, value, Span::NONE);
    let value_ref = l.positioned(value_ref);
    let info = l.u.info(value);
    if matches!(l.u.erase(info), Erased::Prim(_)) {
        return l.call(value_ref, cc.hash_code, &[]);
    }
    l.static_call(cc.objects_hash_code, &[value_ref])
}
