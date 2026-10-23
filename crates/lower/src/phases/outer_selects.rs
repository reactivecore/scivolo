// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/ElimOuterSelect.scala
//! dotty's `ElimOuterSelect`: the outer selections `(q: C).n_<outer>` of inline expansions made
//! calls of `n` outer accessors from `q`.

use super::explicit_outer::{outer_accessor, terms};
use crate::Lower;
use crate::definitions::type_ref;
use typer::inlining::outer_this::outer_hops;
use typer::tpd::{TNodeId, Tree};

/// Rewrites the outer selections in class definitions `classes`.
pub fn transform(l: &mut Lower, classes: &[TNodeId]) {
    for &td in classes {
        for t in terms(l, td, false) {
            let Tree::Select { qualifier, name } = *l.tpd.tree(t) else { continue };
            let Some(hops) = outer_hops(l.names.text(name.name)) else { continue };
            let span = l.tpd.span(t);
            let mut cls = l.u.class_symbol(l.tpd.tpe(qualifier)).expect("an outer selection from a class");
            let mut read = qualifier;
            for _ in 0..hops {
                let accessor = outer_accessor(l, cls);
                let info = l.u.info(accessor);
                cls = l.u.class_symbol(l.u.result_type(info)).expect("an outer class");
                let fun = l.select(read, accessor, span);
                let tpe = type_ref(l.u, cls);
                read = l.apply(fun, &[], tpe, span);
            }
            l.copy_node(t, read);
        }
    }
}
