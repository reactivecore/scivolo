//! Object instances: `module` loads, selections of nested objects (dotty's `SelectStatic`) and
//! `C.this`.

use super::Gen;
use crate::phases::select_static::is_static_owner_ref;
use crate::symbols::{is_module_class, is_package_object, is_static};
use crate::{Result, unsupported};
use nir::{Op, ValId};
use syntax::ast::Span;
use tasty::SymbolId;
use typer::tpd::TNodeId;

impl Gen<'_, '_> {
    /// The instance of object `sym`, its value or its class.
    pub fn gen_module(&mut self, sym: SymbolId) -> ValId {
        let class = if is_module_class(self.l.u, sym) { Some(sym) } else { self.l.u.module_class(sym) };
        let name = self.module_name(class.unwrap_or(sym));
        self.emit(Op::Module(name))
    }

    /// Selection `t` of object `sym` nested in a class, on `qual`: the object at the selection's
    /// start for a path of static objects, else `{ qual; sym }`.
    pub fn gen_nested_module(&mut self, t: TNodeId, sym: SymbolId, qual: TNodeId) -> Result<ValId> {
        let span = self.l.tpd.span(t);
        if is_static_owner_ref(self.l, qual) {
            let saved = self.at_span(if span.exists() { span.with_point(span.start()) } else { span });
            let v = self.gen_module(sym);
            self.m.pos = saved;
            return Ok(v);
        }
        let qual_span = self.l.tpd.span(qual);
        let qual_pos = self.pos_or(qual_span, self.m.pos);
        let scope = self.enter_scope(qual_pos);
        self.gen_expr(qual)?;
        let saved = self.at_span(if qual_span.exists() { qual_span.end_pos() } else { span });
        let v = self.gen_module(sym);
        self.m.pos = saved;
        self.m.scope = scope;
        Ok(v)
    }

    pub fn gen_this(&mut self, t: TNodeId) -> Result<ValId> {
        let span = self.l.tpd.span(t);
        let Some(sym) = self.l.tpd.sym(t) else { return unsupported(span, "`this` without a class") };
        let saved = self.at_span(span);
        let v = self.this_of(sym, span)?;
        self.m.pos = saved;
        Ok(v)
    }

    /// `C.this` for class `c`: an object's instance, or the method's `this`.
    pub fn this_of(&mut self, c: SymbolId, span: Span) -> Result<ValId> {
        let class = self.class.expect("a class");
        let package_object = is_package_object(self.l.u, c);
        let static_method = self.m.this.is_none();
        let static_module = is_module_class(self.l.u, c) && is_static(self.l.u, c);
        if (c != class || static_method) && static_module || package_object {
            return Ok(self.gen_module(c));
        }
        let method_owner = self.m.sym.and_then(|m| self.l.u.symbol(m).owner);
        let mut owner = self.l.u.symbol(class).owner;
        while let Some(o) = owner {
            if o == c {
                return unsupported(span, "`this` of an enclosing class");
            }
            owner = self.l.u.symbol(o).owner;
        }
        match self.m.this {
            Some(this) if c == class || method_owner == Some(class) => Ok(this),
            _ => unsupported(span, "`this` of an enclosing class"),
        }
    }
}
