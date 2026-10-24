// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/SyntheticMembers.scala
//! The mirror support of the anonymous classes the typer makes for mirrors (dotty's
//! `addMirrorSupport` on an `ExtendsSumOrProductMirror` template): `Mirror.Product` with
//! `fromProduct`, or `Mirror.Sum` with `ordinal`, of the class its `MirroredMonoType` names.

use super::case_members::case_accessors;
use super::lift_names::fresh;
use super::mirrors::{from_product, ordinal};
use crate::definitions::type_ref;
use crate::symbols::has;
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::tags::{ENUM, INLINEPROXY};
use tasty::{SymbolId, Type};
use typer::tpd::{AnonMirror, TNodeId};

/// The parent anonymous mirror class `class` of kind `kind` gets and its members; the names of
/// an inline argument's `fromProduct` taken a second time, as dotty's PostTyper synthesizes it in
/// the inline call too.
pub fn anon_mirror_support(
    l: &mut Lower,
    root: TNodeId,
    class: SymbolId,
    kind: AnonMirror,
    span: Span,
) -> Result<(SymbolId, Vec<TNodeId>)> {
    let mono = l.u.decl(class, "MirroredMonoType").unwrap_or_default().first().copied();
    let linked = mono.and_then(|m| l.u.alias_of(m)).and_then(|a| {
        let a = l.u.dealias(a);
        let a = l.u.normalized(a);
        l.u.class_symbol(a)
    });
    let (Some(mono), Some(linked)) = (mono, linked) else {
        return unsupported(span, "anonymous mirror of a type that is no class");
    };
    let focus = Span::new(span.point(), span.point(), span.point());
    let cc = l.case_classes()?;
    let this = type_ref(l.u, class);
    let this = l.u.intern_type(Type::This(this));
    let mono = l.u.intern_type(Type::Sym(Some(this), mono));
    Ok(match kind {
        AnonMirror::Product => {
            if l.u.symbol(class).owner.is_some_and(|o| has(l.u, o, INLINEPROXY)) {
                for accessor in case_accessors(l, linked) {
                    let name = l.u.symbol(accessor).name.clone();
                    fresh(l, &name);
                }
            }
            (cc.mirror_product, vec![from_product(l, class, linked, Some(mono), focus)?])
        }
        AnonMirror::Sum if has(l.u, linked, ENUM) => (cc.mirror_sum, vec![ordinal(l, class, linked, focus)?]),
        AnonMirror::Sum => {
            let param = mono;
            let ordinal = super::sum_mirrors::ordinal(l, root, (class, linked), param, focus)?;
            (cc.mirror_sum, vec![ordinal])
        }
    })
}
