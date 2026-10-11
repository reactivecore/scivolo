//! Constants and class literals (nscplugin's `genLiteral`).

use super::Gen;
use super::callee::is_primitive;
use crate::{Result, unsupported};
use nir::{Val, ValId};
use syntax::ast::constant::Constant;
use typer::tpd::TNodeId;

impl Gen<'_, '_> {
    pub fn gen_literal(&mut self, t: TNodeId, c: &Constant) -> Result<ValId> {
        let v = match *c {
            Constant::Unit => Val::Unit,
            Constant::Null => Val::Null,
            Constant::Boolean(true) => Val::True,
            Constant::Boolean(false) => Val::False,
            Constant::Char(c) => Val::Char(c),
            Constant::Int(i) => Val::Int(i),
            Constant::Long(l) => Val::Long(l),
            Constant::Float(f) => Val::Float(f.to_bits()),
            Constant::Double(d) => Val::Double(d.to_bits()),
            Constant::String(ref units) => match String::from_utf16(units) {
                Ok(s) => Val::String(self.str(&s)),
                Err(_) => return unsupported(self.l.tpd.span(t), "strings that are not valid Unicode"),
            },
        };
        Ok(self.val(v))
    }

    /// `classOf[tpe]`: the class of an object for its singleton type.
    pub fn gen_class_literal(&mut self, t: TNodeId, tpe: tasty::TypeId) -> Result<ValId> {
        let class = match *self.l.u.types().get(tpe) {
            tasty::Type::Sym(_, s) if self.l.u.is_module(s) => self.l.u.module_class(s),
            _ => self.l.u.class_symbol(tpe),
        };
        match class {
            Some(c) if is_primitive(&self.l.u.binary_name(c)) => {
                let simple = self.l.u.symbol(c).name.clone();
                let name = self.pool.top(&format!("scala.scalanative.runtime.Primitive{simple}"));
                Ok(self.val(Val::ClassOf(name)))
            }
            Some(c) => {
                let name = self.type_name(c);
                Ok(self.val(Val::ClassOf(name)))
            }
            _ => unsupported(self.l.tpd.span(t), "classOf of a type that is not a class"),
        }
    }
}
