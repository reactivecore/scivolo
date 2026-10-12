//! The members dotty's `SyntheticMembers` gives value classes, each unless the class has one:
//! `hashCode` (`valueHashCodeBody`) and `equals` of the underlying value.

use super::case_members::existing_def;
use crate::Lower;
use crate::Result;
use syntax::ast::Span;
use tasty::tags::{OVERRIDE, SYNTHETIC};
use tasty::{Erased, Flags, SymbolId};
use typer::tpd::TNodeId;

/// The synthetic members of value class `class`, at `span`'s point.
pub fn value_members(l: &mut Lower, class: SymbolId, span: Span) -> Result<Vec<TNodeId>> {
    let Some(value) = l.u.underlying(class).filter(|_| !l.u.is_trait(class)) else { return Ok(Vec::new()) };
    let hash_code = l.defs.case_classes.hash_code;
    let focus = Span::new(span.point(), span.point(), span.point());
    let mut out = Vec::new();
    for sym in [hash_code, l.defs.equality.any_equals] {
        if existing_def(l, class, sym).is_some() {
            continue;
        }
        let flags = Flags::default().with(SYNTHETIC).with(OVERRIDE);
        let info = l.u.info(sym);
        let mut locals = Vec::new();
        let (method, def) =
            l.method_like(class, sym, flags, info, focus, |l, args, _| match sym == hash_code {
                true => hash_code_body(l, class, value),
                false => l.equals_body(class, &[value], args[0], None, &mut locals),
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
fn hash_code_body(l: &mut Lower, class: SymbolId, value: SymbolId) -> TNodeId {
    let this = l.this(class, Span::NONE);
    let value_ref = l.select(this, value, Span::NONE);
    let value_ref = l.positioned(value_ref);
    let info = l.u.info(value);
    if matches!(l.u.erase(info), Erased::Prim(_)) {
        let hash_code = l.defs.case_classes.hash_code;
        return l.call(value_ref, hash_code, &[]);
    }
    let objects_hash_code = l.defs.case_classes.objects_hash_code;
    l.static_call(objects_hash_code, &[value_ref])
}
