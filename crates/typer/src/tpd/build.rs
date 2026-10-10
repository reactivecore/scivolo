//! Typed trees made from symbols and types (dotty's `tpd` constructors).

use super::{List, TNodeId, Tpd, Tree as T};
use crate::Typer;
use crate::defn::type_ref;
use syntax::ast::constant::Constant;
use syntax::ast::{Span, TName};
use syntax::names::Names;
use tasty::tags::*;
use tasty::{SymbolId, Type, TypeId, Universe};

pub struct Build<'a> {
    pub tpd: &'a mut Tpd,
    pub u: &'a mut Universe,
    pub names: &'a mut Names,
}

impl Typer<'_> {
    pub fn build(&mut self) -> Build<'_> {
        Build { tpd: &mut self.tpd, u: self.u, names: self.names }
    }
}

impl Build<'_> {
    /// The term name of `s`.
    pub fn term_name(&mut self, s: SymbolId) -> TName {
        TName { name: self.names.intern(&self.u.symbol(s).name), is_type: false }
    }

    pub fn constant_type(&mut self, c: &Constant) -> TypeId {
        let t = match *c {
            Constant::Unit => Type::Constant(UNITconst, 0),
            Constant::Boolean(b) => Type::Constant(if b { TRUEconst } else { FALSEconst }, 0),
            Constant::Char(c) => Type::Constant(CHARconst, c as i64),
            Constant::Int(i) => Type::Constant(INTconst, i as i64),
            Constant::Long(l) => Type::Constant(LONGconst, l),
            Constant::Float(f) => Type::Constant(FLOATconst, f.to_bits() as i64),
            Constant::Double(d) => Type::Constant(DOUBLEconst, d.to_bits() as i64),
            Constant::String(ref s) => Type::Str(String::from_utf16_lossy(s)),
            Constant::Null => Type::Constant(NULLconst, 0),
        };
        self.u.intern_type(t)
    }

    /// `value`, of its constant type.
    pub fn literal(&mut self, value: Constant, span: Span) -> TNodeId {
        let tpe = self.constant_type(&value);
        self.tpd.node(T::Literal { value }, span, Some(tpe), None)
    }

    /// An identifier of `s` by its name, of type `tpe` (dotty's `Ident(tp)`).
    pub fn ident(&mut self, s: SymbolId, tpe: TypeId, span: Span) -> TNodeId {
        let name = self.term_name(s);
        self.tpd.node(T::Ident { name }, span, Some(tpe), Some(s))
    }

    /// An identifier of `s`, of its type from where its owner's members are visible (dotty's
    /// `ref(sym)`).
    pub fn ref_(&mut self, s: SymbolId, span: Span) -> TNodeId {
        let tpe = type_ref(self.u, s);
        self.ident(s, tpe, span)
    }

    /// `qual.m`, of member `m` seen from the type of `qual`.
    pub fn select(&mut self, qual: TNodeId, m: SymbolId, span: Span) -> TNodeId {
        let prefix = self.tpd.tpe(qual);
        let tpe = self.u.intern_type(Type::Sym(Some(prefix), m));
        let name = self.term_name(m);
        self.tpd.node(T::Select { qualifier: qual, name }, span, Some(tpe), Some(m))
    }

    /// `C.this` of class type `class` at the start of `span`, its qualifier the untyped name of
    /// the class (dotty's `This(cls)`).
    pub fn this(&mut self, class: TypeId, span: Span) -> TNodeId {
        let cls = self.u.class_symbol(class);
        let text = cls.map(|c| self.u.symbol(c).name.clone()).unwrap_or_default();
        let qual = TName { name: self.names.intern(&text), is_type: true };
        let qual = self.tpd.node(T::Ident { name: qual }, span.start_pos(), None, None);
        let tpe = self.u.intern_type(Type::This(class));
        self.tpd.node(T::This { qual }, span.start_pos(), Some(tpe), cls)
    }

    pub fn apply(&mut self, fun: TNodeId, args: &[TNodeId], tpe: TypeId, span: Span) -> TNodeId {
        let args = self.tpd.list(args);
        self.tpd.node(T::Apply { fun, args }, span, Some(tpe), None)
    }

    pub fn type_apply(&mut self, fun: TNodeId, args: &[TNodeId], tpe: TypeId, span: Span) -> TNodeId {
        let args = self.tpd.list(args);
        self.tpd.node(T::TypeApply { fun, args }, span, Some(tpe), None)
    }

    /// `{ stats; expr }`, of the type of `expr`.
    pub fn block(&mut self, stats: List, expr: TNodeId, span: Span) -> TNodeId {
        let tpe = self.tpd.tpe(expr);
        self.tpd.node(T::Block { stats, expr }, span, Some(tpe), None)
    }

    /// `return expr` from `method`, identified at the end of `span`.
    pub fn return_(&mut self, expr: TNodeId, method: SymbolId, span: Span) -> TNodeId {
        let from_tpe = self.u.intern_type(Type::Sym(None, method));
        let from = self.ident(method, from_tpe, span.end_pos());
        let nothing = type_ref(self.u, self.u.builtins().nothing);
        self.tpd.node(T::Return { expr, from }, span, Some(nothing), None)
    }

    /// The definition of value `s`.
    pub fn val_def(&mut self, s: SymbolId, tpt: TNodeId, rhs: Option<TNodeId>, span: Span) -> TNodeId {
        let name = self.term_name(s);
        self.tpd.node(T::ValDef { name, tpt, rhs }, span, None, Some(s))
    }

    /// The definition of method `s` with one clause of `params`.
    pub fn def_def(
        &mut self,
        s: SymbolId,
        params: &[TNodeId],
        tpt: TNodeId,
        rhs: TNodeId,
        span: Span,
    ) -> TNodeId {
        let name = self.term_name(s);
        let clause = self.tpd.list(params);
        let paramss = self.tpd.clauses(&[clause]);
        self.tpd.node(T::DefDef { name, paramss, tpt, rhs: Some(rhs) }, span, None, Some(s))
    }
}
