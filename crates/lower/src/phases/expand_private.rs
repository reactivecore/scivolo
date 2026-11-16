// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//   compiler/src/dotty/tools/dotc/transform/ExpandPrivate.scala
//! dotty's `ExpandPrivate`: private members used from another class, a nested one or a
//! companion, and the private values of value classes get expanded names and become public, as
//! do `@publicInBinary` private constructors.

use crate::Lower;
use crate::symbols::has;
use rustc_hash::FxHashSet;
use tasty::tags::PRIVATE;
use typer::tpd::{TNodeId, Tree};

pub fn transform(l: &mut Lower, classes: &[TNodeId]) {
    for &td in classes {
        let class = l.tpd.sym(td).expect("a class definition's symbol");
        if let Some(value) = l.u.underlying(class).filter(|&v| has(l.u, v, PRIVATE)) {
            super::accessor_names::ensure_not_private(l, value);
        }
        let decls = l.u.symbol(class).decls.clone();
        for c in decls {
            let private = l.u.is_constructor(c) && has(l.u, c, PRIVATE);
            if private && l.u.is_annotated(c, "scala.annotation.publicInBinary") {
                super::accessor_names::ensure_not_private(l, c);
            }
        }
        let terms = super::explicit_outer::terms(l, td, false);
        let field_qualifiers = terms.iter().filter_map(|&t| field_qualifier(l, t)).collect::<FxHashSet<_>>();
        for t in terms {
            if !matches!(l.tpd.tree(t), Tree::Ident { .. } | Tree::Select { .. })
                || field_qualifiers.contains(&t)
            {
                continue;
            }
            let Some(s) = l.tpd.sym(t) else { continue };
            let Some(owner) = l.u.symbol(s).owner else { continue };
            if owner != class && l.u.is_class(owner) && has(l.u, s, PRIVATE) && l.u.symbol(s).kind.is_term() {
                super::accessor_names::ensure_not_private(l, s);
            }
        }
    }
}

/// The object `Lower::field_ref` puts before field selection `t` of that static object, where
/// dotty refers to the field alone.
fn field_qualifier(l: &mut Lower, t: TNodeId) -> Option<TNodeId> {
    let Tree::Select { qualifier, .. } = *l.tpd.tree(t) else { return None };
    let module = l.tpd.sym(qualifier).filter(|_| matches!(l.tpd.tree(qualifier), Tree::Ident { .. }))?;
    let field_class = l.tpd.sym(t).and_then(|f| l.u.symbol(f).owner);
    let synthesized = l.tpd.span(qualifier) == l.tpd.span(t) && l.u.is_module(module);
    (synthesized && l.u.module_class(module) == field_class && field_class.is_some()).then_some(qualifier)
}
