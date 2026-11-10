// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/PatternMatcher.scala
//! Type patterns: `x @ (p: T)` brought to `(x @ p): T`, the test `scrutinee.$isInstanceOf$[T]`
//! and the cast `scrutinee.asInstanceOf[T]` the pattern then matches; named tuples cast to their
//! values.

use super::Translator;
use crate::phases::explicit_outer::outer_accessor;
use crate::phases::lazy_trees::dup;
use crate::phases::outer_tests::{OuterTest, outer_test};
use crate::phases::type_tests::singletons::{is_instance, singleton};
use crate::symbols::has;
use syntax::ast::Span;
use tasty::tags::{CASE, INLINE, MUTABLE, STABLE};
use tasty::{Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

impl Translator<'_, '_> {
    /// `Any` for a value `tree` of a pattern-bound type, else its own type (dotty's
    /// `avoidPatBoundType`).
    pub fn avoid_pat_bound_type(&mut self, tree: TNodeId) -> Option<TypeId> {
        let tpe = self.l.tpd.tpe(tree);
        let widened = self.l.u.widen(tpe);
        let s = self.l.u.sym_of(widened)?;
        (self.l.u.symbol(s).kind == Kind::Type && has(self.l.u, s, CASE)).then_some(tasty::ANY)
    }

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

    /// Type `t` of a binder, by-name types dropped and widened once while it refers to a variable,
    /// a method that is not stable or a by-name or inline parameter (dotty's `sanitize` through
    /// `widenIfUnstable`).
    pub fn sanitized(&mut self, mut t: TypeId) -> TypeId {
        loop {
            let s = match *self.l.u.ty(t) {
                Type::ByName(r) => {
                    t = r;
                    continue;
                }
                Type::Sym(_, s) => s,
                _ => return t,
            };
            let (kind, info) = (self.l.u.symbol(s).kind, self.l.u.info(s));
            let unstable = match kind {
                Kind::Val => has(self.l.u, s, MUTABLE),
                Kind::Def => !has(self.l.u, s, STABLE),
                Kind::Param => has(self.l.u, s, INLINE) || self.l.u.by_name(info).is_some(),
                _ => false,
            };
            match self.l.u.widen_once(t).filter(|_| unstable) {
                Some(u) => t = u,
                None => return t,
            }
        }
    }

    /// `value.asInstanceOf[target]`.
    pub fn cast(&mut self, value: TNodeId, target: TypeId) -> TNodeId {
        self.type_apply(value, self.l.defs.as_instance_of, target, target)
    }

    /// `value.asInstanceOf[V]` if `value` is a named tuple `NamedTuple[N, V]`, else `value`
    /// (dotty's `dropNamedTuple`).
    pub fn drop_named_tuple(&mut self, value: TNodeId) -> TNodeId {
        let tpe = self.l.tpd.tpe(value);
        match self.named_tuple_values(tpe) {
            Some(values) => self.cast(value, values),
            None => value,
        }
    }

    /// The values tuple `V` of `t` widened if that is a named tuple `NamedTuple[N, V]`.
    pub fn named_tuple_values(&mut self, t: TypeId) -> Option<TypeId> {
        let widened = self.l.u.widen(t);
        let dealiased = self.l.u.dealias(widened);
        let Some((tycon, &[_, values])) = self.l.u.applied(dealiased) else { return None };
        let nt = self.l.u.referenced(tycon)?;
        (self.l.u.full_name(nt) == "scala.NamedTuple$.NamedTuple").then_some(values)
    }

    /// The condition `scrutinee.$isInstanceOf$[T]` of type pattern type `tpt`, for `p.C` of a
    /// class `C` with an enclosing instance also `scrutinee.asInstanceOf[C].C$$$outer() eq p` for
    /// a singleton `p`, `….$isInstanceOf$[P]` for a class type `P` (dotty's `addOuterTest`).
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
