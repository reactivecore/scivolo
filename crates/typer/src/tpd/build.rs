//! Typed trees made from symbols and types (dotty's `tpd` constructors).

use super::{List, TNodeId, Tpd, Tree as T};
use crate::defn::{Defn, type_ref};
use syntax::ast::constant::Constant;
use syntax::ast::{Span, TName};
use syntax::names::Names;
use tasty::tags::*;
use tasty::{SymbolId, Type, TypeId, Universe};

pub struct Build<'a> {
    tpd: &'a mut Tpd,
    u: &'a mut Universe,
    names: &'a mut Names,
    defn: &'a Defn,
}

/// dotty's `ConstantType(c)`.
pub fn constant_type_of(u: &mut Universe, c: &Constant) -> TypeId {
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
    u.intern_type(t)
}

impl<'a> Build<'a> {
    pub fn new(tpd: &'a mut Tpd, u: &'a mut Universe, names: &'a mut Names, defn: &'a Defn) -> Build<'a> {
        Build { tpd, u, names, defn }
    }

    /// The term name of `s`.
    pub fn term_name(&mut self, s: SymbolId) -> TName {
        TName { name: self.names.intern(&self.u.symbol(s).name), is_type: false }
    }

    /// The type of literal `c`: `Unit` and `Null` for `()` and `null`, else a constant type.
    pub fn literal_type(&mut self, c: &Constant) -> TypeId {
        match c {
            Constant::Unit => type_ref(self.u, self.defn.unit),
            Constant::Null => type_ref(self.u, self.u.builtins().null),
            _ => constant_type_of(self.u, c),
        }
    }

    pub fn literal(&mut self, value: Constant, span: Span) -> TNodeId {
        let tpe = self.literal_type(&value);
        self.tpd.node(T::Literal { value }, span, Some(tpe), None)
    }

    /// An identifier of type `tpe`, a reference to a symbol, by its term name (dotty's
    /// `Ident(tp)`).
    pub fn ident(&mut self, tpe: TypeId, span: Span) -> TNodeId {
        let Type::Sym(_, s) = *self.u.types().get(tpe) else { unreachable!("a reference type") };
        let name = self.term_name(s);
        self.tpd.node(T::Ident { name }, span, Some(tpe), Some(s))
    }

    /// An identifier of static or local symbol `s` by its term name, of its type from where its
    /// owner's members are visible.
    pub fn static_ident(&mut self, s: SymbolId, span: Span) -> TNodeId {
        let tpe = type_ref(self.u, s);
        self.ident(tpe, span)
    }

    /// `qual.m` by the term name of member `m`, seen from the type of `qual`.
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
        let from = self.ident(from_tpe, span.end_pos());
        let nothing = type_ref(self.u, self.u.builtins().nothing);
        self.tpd.node(T::Return { expr, from }, span, Some(nothing), None)
    }

    /// The definition of value `s`.
    pub fn val_def(&mut self, s: SymbolId, tpt: TNodeId, rhs: Option<TNodeId>, span: Span) -> TNodeId {
        let name = self.term_name(s);
        self.tpd.node(T::ValDef { name, tpt, rhs }, span, None, Some(s))
    }

    /// The definition of method `s` with parameter clauses `paramss`.
    pub fn def_def(
        &mut self,
        s: SymbolId,
        paramss: &[&[TNodeId]],
        tpt: TNodeId,
        rhs: TNodeId,
        span: Span,
    ) -> TNodeId {
        let name = self.term_name(s);
        let clauses: Vec<List> = paramss.iter().map(|ps| self.tpd.list(ps)).collect();
        let paramss = self.tpd.clauses(&clauses);
        self.tpd.node(T::DefDef { name, paramss, tpt, rhs: Some(rhs) }, span, None, Some(s))
    }
}
