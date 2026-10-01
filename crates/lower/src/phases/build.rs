//! Typed trees made by the phases (dotty's `tpd` constructors).

use crate::Lower;
use crate::definitions::type_ref;
use syntax::ast::constant::Constant;
use syntax::ast::{Span, TName};
use tasty::{SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

impl Lower<'_> {
    pub fn term_name(&mut self, s: &str) -> TName {
        TName { name: self.names.intern(s), is_type: false }
    }

    pub fn type_tree(&mut self, tpe: TypeId, span: Span) -> TNodeId {
        self.tpd.node(Tree::TypeTree { inferred: false }, span, Some(tpe), None)
    }

    /// A reference to `s` by its name.
    pub fn ident(&mut self, s: SymbolId, span: Span) -> TNodeId {
        let name = self.u.symbol(s).name.clone();
        let name = self.term_name(&name);
        let tpe = type_ref(self.u, s);
        self.tpd.node(Tree::Ident { name }, span, Some(tpe), Some(s))
    }

    /// `new C`, of class `c`.
    pub fn new_(&mut self, c: SymbolId, span: Span) -> TNodeId {
        let tpe = type_ref(self.u, c);
        let tpt = self.type_tree(tpe, span);
        self.tpd.node(Tree::New { tpt }, span, Some(tpe), None)
    }

    /// `qual.m`, of member `m`.
    pub fn select(&mut self, qual: TNodeId, m: SymbolId, span: Span) -> TNodeId {
        let name = self.u.symbol(m).name.clone();
        let name = self.term_name(&name);
        let prefix = self.tpd.tpe(qual);
        let tpe = self.u.intern_type(Type::Sym(Some(prefix), m));
        self.tpd.node(Tree::Select { qualifier: qual, name }, span, Some(tpe), Some(m))
    }

    pub fn apply(&mut self, fun: TNodeId, args: &[TNodeId], tpe: TypeId, span: Span) -> TNodeId {
        let args = self.tpd.list(args);
        self.tpd.node(Tree::Apply { fun, args }, span, Some(tpe), None)
    }

    /// `C.this`, of class `c`.
    pub fn this(&mut self, c: SymbolId, span: Span) -> TNodeId {
        let class = type_ref(self.u, c);
        let tpe = self.u.intern_type(Type::This(class));
        let name = TName { name: self.names.intern(""), is_type: true };
        let qual = self.tpd.node(Tree::Ident { name }, span, Some(tpe), None);
        self.tpd.node(Tree::This { qual }, span, Some(tpe), Some(c))
    }

    /// `C.super`, of class `c`, with the type of its superclass `parent`.
    pub fn super_(&mut self, c: SymbolId, parent: TypeId, span: Span) -> TNodeId {
        let qual = self.this(c, span);
        self.tpd.node(Tree::Super { qual, mix: None }, span, Some(parent), None)
    }

    pub fn unit_literal(&mut self, span: Span) -> TNodeId {
        let tpe = self.u.intern_type(Type::Constant(tasty::tags::UNITconst, 0));
        self.tpd.node(Tree::Literal { value: Constant::Unit }, span, Some(tpe), None)
    }

    pub fn block(&mut self, stats: &[TNodeId], expr: TNodeId, span: Span) -> TNodeId {
        let tpe = self.tpd.tpe(expr);
        let stats = self.tpd.list(stats);
        self.tpd.node(Tree::Block { stats, expr }, span, Some(tpe), None)
    }
}
