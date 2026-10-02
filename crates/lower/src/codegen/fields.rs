//! Fields: a `var` per field of a class, loads and stores (nscplugin's `genClassFields`,
//! `genFieldName`, and the field cases of `genSelect` and `genAssign`).

use super::Gen;
use crate::symbols::{has, is_static_module};
use crate::{Result, unsupported};
use nir::sig::{Scope, Sig};
use nir::{Attr, Defn, DefnKind, GlobalId, Op, PosId, Val, ValId};
use syntax::ast::Span;
use tasty::SymbolId;
use tasty::tags::{MUTABLE, PRIVATE};
use typer::tpd::{TNodeId, Tree};

impl Gen<'_, '_> {
    /// Whether `s` is a field: one `Memoize` made, or a private local value.
    pub fn is_field(&self, s: SymbolId) -> bool {
        self.l.fields.contains_key(&s)
    }

    /// A `var` for each field of class `c`, in declaration order.
    pub fn gen_class_fields(&mut self, c: SymbolId, class_pos: PosId) {
        let decls = self.l.u.decls(c).unwrap_or_default();
        let fields: Vec<SymbolId> = decls.into_iter().filter(|&f| self.is_field(f)).collect();
        for f in fields {
            let pos = self.sym_pos(f, c);
            let pos = if pos == self.no_pos { class_pos } else { pos };
            let mutable = has(self.l.u, f, MUTABLE);
            let attrs = if mutable { Vec::new() } else { vec![Attr::Final] };
            let info = self.l.u.info(f);
            let ty = self.gen_type(info);
            let rhs = self.val(Val::Zero(ty));
            let name = self.field_name(f);
            self.defns.push(Defn { name, attrs, pos, kind: DefnKind::Var { ty, rhs } });
        }
    }

    pub fn field_name(&mut self, f: SymbolId) -> GlobalId {
        let owner = self.l.u.symbol(f).owner.expect("a field's class");
        let owner = self.type_name(owner);
        let scope = if has(self.l.u, f, PRIVATE) && !has(self.l.u, f, MUTABLE) {
            Scope::Private(owner)
        } else {
            Scope::Public
        };
        let id = self.local_name(f);
        self.pool.member(owner, &Sig::Field(id, scope))
    }

    /// The value of field `f` of `qual`, or of `this` for an identifier.
    pub fn gen_field_load(&mut self, t: TNodeId, f: SymbolId, qual: Option<TNodeId>) -> Result<ValId> {
        let ty = self.gen_type(self.l.tpd.tpe(t));
        let obj = self.field_owner(f, qual, self.l.tpd.span(t))?;
        let name = self.field_name(f);
        Ok(self.emit(Op::Fieldload { ty, obj, name }))
    }

    /// The object holding field `f`: a static object's instance, else `qual` or `this`.
    fn field_owner(&mut self, f: SymbolId, qual: Option<TNodeId>, span: Span) -> Result<ValId> {
        let owner = self.l.u.symbol(f).owner.expect("a field's class");
        match qual {
            _ if is_static_module(self.l.u, owner) => Ok(self.gen_module(owner)),
            Some(q) => self.gen_expr(q),
            None => self.this_of(owner, span),
        }
    }

    /// `lhs = rhs` for a field `lhs`.
    pub fn gen_assign(&mut self, t: TNodeId, lhs: TNodeId, rhs: TNodeId) -> Result<ValId> {
        let span = self.l.tpd.span(t);
        let (qualifier, Some(f)) = (self.l.tpd.tree(lhs).clone(), self.l.tpd.sym(lhs)) else {
            return unsupported(span, "an assignment without a symbol");
        };
        let qualifier = match qualifier {
            Tree::Select { qualifier, .. } => Some(qualifier),
            _ => None,
        };
        if let Some(&slot) = self.m.env.get(&f).filter(|_| self.m.mutable.contains(&f)) {
            let saved = self.at_span(span);
            let value = self.gen_expr(rhs)?;
            let ty = self.gen_type(self.l.tpd.tpe(lhs));
            self.check_conforms(value, ty, span)?;
            let v = self.emit(Op::Varstore { slot, value });
            self.m.pos = saved;
            return Ok(v);
        }
        if !self.is_field(f) {
            return unsupported(span, "assignments through setters");
        }
        let saved = self.at_span(span);
        let obj = self.field_owner(f, qualifier, span)?;
        let ty = self.gen_type(self.l.tpd.tpe(lhs));
        let value = self.gen_expr(rhs)?;
        self.check_conforms(value, ty, span)?;
        let name = self.field_name(f);
        let v = self.emit(Op::Fieldstore { ty, obj, name, value });
        self.m.pos = saved;
        Ok(v)
    }
}
