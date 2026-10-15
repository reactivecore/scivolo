//! Type patterns: `x @ (p: T)` brought to `(x @ p): T`, the test `scrutinee.$isInstanceOf[T]`
//! and the cast `scrutinee.asInstanceOf[T]` the pattern then matches.

use super::Translator;
use crate::phases::explicit_outer::outer_accessor;
use crate::phases::lazy_trees::dup;
use crate::phases::outer_tests::{OuterTest, outer_test};
use crate::phases::type_tests::singletons::{is_instance, singleton};
use syntax::ast::Span;
use tasty::{SymbolId, TypeId};
use typer::tpd::{TNodeId, Tree};

impl Translator<'_, '_> {
    /// `pat` with `x @ (p: T)` made `(x @ p): T`, repeatedly (dotty's `swapBind`).
    pub fn swap_bind(&mut self, pat: TNodeId) -> TNodeId {
        let Tree::Bind { name, body } = *self.l.tpd.tree(pat) else { return pat };
        let swapped = self.swap_bind(body);
        let Tree::Typed { expr, tpt } = *self.l.tpd.tree(swapped) else { return pat };
        let node = self.l.tpd.get(pat);
        let (span, tpe, sym) = (node.span, node.tpe, node.sym);
        let bind = self.l.tpd.node(Tree::Bind { name, body: expr }, span, tpe, sym);
        let typed_tpe = self.l.tpd.tpe(tpt);
        self.node(Tree::Typed { expr: bind, tpt }, typed_tpe, None)
    }

    /// `value.asInstanceOf[target]`.
    pub fn cast(&mut self, value: TNodeId, target: TypeId) -> TNodeId {
        self.type_apply(value, self.l.defs.as_instance_of, target, target)
    }

    /// The condition `scrutinee.$isInstanceOf[T]` of type pattern type `tpt`, for `p.C` of a class
    /// `C` with an enclosing instance also `scrutinee.asInstanceOf[C].C$$$outer() eq p` for a
    /// singleton `p`, `….$isInstanceOf[P]` for a class type `P` (dotty's `addOuterTest`).
    pub fn type_test(&mut self, scrutinee: TNodeId, tpt: TNodeId) -> TNodeId {
        let target = self.l.tpd.tpe(tpt);
        let dealiased = self.l.u.dealias(target);
        let written = self.written_value(tpt);
        if let Some(test) = is_instance(self.l, scrutinee, dealiased, written) {
            return test;
        }
        let boolean = self.l.u.class_ref("scala.Boolean");
        let test = self.type_apply(scrutinee, self.l.defs.type_test, target, boolean);
        let outer_test = outer_test(self.l, target);
        let (Some(OuterTest::Is(pre)) | Some(OuterTest::Has(pre))) = outer_test else { return test };
        let class = self.l.u.class_symbol(target).expect("a class type");
        let accessor = outer_accessor(self.l, class);
        let value = dup(self.l, scrutinee);
        let value = self.cast(value, target);
        let fun = self.l.select(value, accessor, Span::NONE);
        let fun = self.l.positioned(fun);
        let info = self.l.u.info(accessor);
        let outer_type = self.l.u.result_type(info);
        let outer = self.l.apply(fun, &[], outer_type, Span::NONE);
        let outer = self.l.positioned(outer);
        let outer_test = match outer_test {
            Some(OuterTest::Has(_)) => self.type_apply(outer, self.l.defs.type_test, pre, boolean),
            _ => {
                let expected = singleton(self.l, pre).expect("a singleton prefix");
                self.l.call(outer, self.l.defs.eq, &[expected])
            }
        };
        self.l.call(test, self.l.defs.and, &[outer_test])
    }

    /// A copy without spans of the path singleton type tree `tpt` refers to, as the phases before
    /// left it.
    fn written_value(&mut self, tpt: TNodeId) -> Option<TNodeId> {
        let Tree::SingletonTypeTree { reference } = *self.l.tpd.tree(tpt) else { return None };
        let value = dup(self.l, reference);
        let mut part = value;
        loop {
            self.l.tpd.get_mut(part).span = Span::NONE;
            let Tree::Select { qualifier, .. } = *self.l.tpd.tree(part) else { break };
            part = qualifier;
        }
        Some(value)
    }

    /// `qual.m[target]` of type `tpe`.
    fn type_apply(&mut self, qual: TNodeId, m: SymbolId, target: TypeId, tpe: TypeId) -> TNodeId {
        let fun = self.l.select(qual, m, Span::NONE);
        let fun = self.l.positioned(fun);
        let tpt = self.l.type_tree(target, Span::NONE);
        let args = self.l.tpd.list(&[tpt]);
        let t = self.l.tpd.node(Tree::TypeApply { fun, args }, Span::NONE, Some(tpe), Some(m));
        self.l.positioned(t)
    }
}
