// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/tpd.scala
//! The types of blocks and selections re-derived as the body of an inline method of a class is
//! copied (dotty's `TypedTreeCopier`).

use super::super::Typer;
use super::super::gadt_cast::is_stable;
use super::MakeInlineable;
use crate::tpd::{TNodeId, Tree as T};
use tasty::{Type, TypeId};

impl MakeInlineable {
    /// Re-derives the type of `tree`, a block or a selection whose expression or qualifier `c`
    /// had type `old` and `tree` type `before`: a block takes its expression's type unless both
    /// kept their types, as `{ 23.asInstanceOf[23 & Int & T] }` whose type the GADT bounds
    /// simplified to `23`; a selection is taken on its qualifier's new type, widened if
    /// unstable. A block whose expression refers to its definitions keeps its type.
    pub fn retype(
        &self,
        t: &mut Typer<'_>,
        tree: TNodeId,
        c: TNodeId,
        (before, old): (Option<TypeId>, Option<TypeId>),
    ) {
        let owner = t.u.symbol(self.meth).owner;
        if !owner.is_some_and(|o| t.u.is_class(o)) {
            return;
        }
        let Some(new) = t.tpd.get(c).tpe else { return };
        match *t.tpd.tree(tree) {
            T::Block { stats, .. } if old != Some(new) || before != old => {
                let locals: Vec<_> = t.tpd.items(stats).iter().filter_map(|&s| t.tpd.sym(s)).collect();
                if old == before || !t.refers_to(new, &locals) {
                    t.tpd.get_mut(tree).tpe = Some(new);
                }
            }
            T::Select { .. } if old != Some(new) => {
                let Some((Some(_), m)) = before.and_then(|b| t.u.sym_ref(b)) else { return };
                let annot = t.u.class_ref("scala.annotation.unchecked.uncheckedStable");
                let prefix = if is_stable(t.u, new, annot) { new } else { t.u.widen(new) };
                t.tpd.get_mut(tree).tpe = Some(t.u.intern_type(Type::Sym(Some(prefix), m)));
            }
            _ => {}
        }
    }
}
