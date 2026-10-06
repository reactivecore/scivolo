//! Typed trees made by the phases (dotty's `tpd` constructors).

use crate::Lower;
use crate::definitions::type_ref;
use crate::symbols::{is_static_module, source_module};
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

    /// A reference to field `f` of its class: through the object if the class is a static
    /// object's, else through `this`.
    pub fn field_ref(&mut self, f: SymbolId, span: Span) -> TNodeId {
        let class = self.u.symbol(f).owner.expect("a field's class");
        let module = is_static_module(self.u, class).then(|| source_module(self.u, class)).flatten();
        let qual = match module {
            Some(m) => self.ident(m, span),
            None => self.this(class, span),
        };
        self.select(qual, f, span)
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

    /// `C.super[T]`, of class `c` and its trait `t`.
    pub fn super_mix(&mut self, c: SymbolId, t: SymbolId, span: Span) -> TNodeId {
        let qual = self.this(c, span);
        let name = self.u.symbol(t).name.clone();
        let mix = TName { name: self.names.intern(&name), is_type: true };
        let tpe = type_ref(self.u, t);
        self.tpd.node(Tree::Super { qual, mix: Some(mix) }, span, Some(tpe), None)
    }

    pub fn unit_literal(&mut self, span: Span) -> TNodeId {
        let tpe = self.u.intern_type(Type::Constant(tasty::tags::UNITconst, 0));
        self.tpd.node(Tree::Literal { value: Constant::Unit }, span, Some(tpe), None)
    }

    pub fn null_literal(&mut self, span: Span) -> TNodeId {
        let tpe = self.u.intern_type(Type::Constant(tasty::tags::NULLconst, 0));
        self.tpd.node(Tree::Literal { value: Constant::Null }, span, Some(tpe), None)
    }

    /// The definition of parameter `p`.
    pub fn param_def(&mut self, p: SymbolId, span: Span) -> TNodeId {
        let name = self.u.symbol(p).name.clone();
        let name = self.term_name(&name);
        let info = self.u.info(p);
        let tpt = self.type_tree(info, span);
        self.tpd.node(Tree::ValDef { name, tpt, rhs: None }, span, Some(info), Some(p))
    }

    /// The definition of method `sym` with one clause of `params`.
    pub fn method_def(
        &mut self,
        sym: SymbolId,
        params: &[TNodeId],
        tpt: TNodeId,
        rhs: TNodeId,
        span: Span,
    ) -> TNodeId {
        let name = self.u.symbol(sym).name.clone();
        let name = self.term_name(&name);
        let clause = self.tpd.list(params);
        let paramss = self.tpd.clauses(&[clause]);
        let info = self.u.info(sym);
        self.tpd.node(Tree::DefDef { name, paramss, tpt, rhs: Some(rhs) }, span, Some(info), Some(sym))
    }

    /// Makes node `to` a copy of node `from`.
    pub fn copy_node(&mut self, to: TNodeId, from: TNodeId) {
        let n = self.tpd.get(from);
        let (tree, span, tpe, sym) = (n.tree.clone(), n.span, n.tpe, n.sym);
        let node = self.tpd.get_mut(to);
        (node.tree, node.span, node.tpe, node.sym) = (tree, span, tpe, sym);
    }

    pub fn block(&mut self, stats: &[TNodeId], expr: TNodeId, span: Span) -> TNodeId {
        let tpe = self.tpd.tpe(expr);
        let stats = self.tpd.list(stats);
        self.tpd.node(Tree::Block { stats, expr }, span, Some(tpe), None)
    }
}
