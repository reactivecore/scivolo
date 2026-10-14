//! Type patterns: `x @ (p: T)` brought to `(x @ p): T`, the test `scrutinee.$isInstanceOf[T]`
//! and the cast `scrutinee.asInstanceOf[T]` the pattern then matches.

use super::Translator;
use crate::phases::explicit_outer::{outer_accessor, outer_prefix};
use crate::phases::lazy_trees::dup;
use syntax::ast::Span;
use tasty::{Kind, SymbolId, Type, TypeId};
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

    /// Whether `tpe` is the type of one value: a singleton or constant type.
    pub fn is_singleton(&self, tpe: TypeId) -> bool {
        match *self.l.u.types().get(tpe) {
            Type::TermRef(..) | Type::This(_) | Type::Constant(..) | Type::Str(_) => true,
            Type::Sym(_, s) => matches!(self.l.u.symbol(s).kind, Kind::Val | Kind::Param),
            _ => false,
        }
    }

    /// `value.asInstanceOf[target]`.
    pub fn cast(&mut self, value: TNodeId, target: TypeId) -> TNodeId {
        self.type_apply(value, self.l.defs.as_instance_of, target, target)
    }

    /// The condition `scrutinee.$isInstanceOf[T]` of type pattern type `tpt`, for `O.this.C` of a
    /// class `C` with an enclosing instance also `scrutinee.asInstanceOf[C].C$$$outer() eq O.this`
    /// (dotty's `addOuterTest`).
    pub fn type_test(&mut self, scrutinee: TNodeId, tpt: TNodeId) -> TNodeId {
        let target = self.l.tpd.tpe(tpt);
        let boolean = self.l.u.class_ref("scala.Boolean");
        let test = self.type_apply(scrutinee, self.l.defs.type_test, target, boolean);
        let Some(pre) = outer_prefix(self.l, target) else { return test };
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
        let Type::This(c) = *self.l.u.types().get(pre) else { unreachable!("a `this` prefix") };
        let c = self.l.u.class_symbol(c).expect("a class");
        let expected = self.l.this(c, Span::NONE);
        let eq = self.l.call(outer, self.l.defs.eq, &[expected]);
        self.l.call(test, self.l.defs.and, &[eq])
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
