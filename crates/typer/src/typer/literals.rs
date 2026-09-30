//! Literals and their constant types (dotty's `typedLiteral`, `ConstantType`).

use super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::NodeId;
use syntax::ast::constant::Constant;
use tasty::tags::*;
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// The type of literal `c`: `Unit` and `Null` for `()` and `null`, else a constant type.
    pub fn literal_type(&mut self, c: &Constant) -> TypeId {
        match c {
            Constant::Unit => crate::defn::type_ref(self.u, self.defn.unit),
            Constant::Null => crate::defn::type_ref(self.u, self.u.builtins().null),
            _ => self.constant_type(c),
        }
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

    pub fn typed_literal(&mut self, id: NodeId, value: Constant) -> TNodeId {
        let tpe = self.literal_type(&value);
        let span = self.ast.span(id);
        self.tpd.node(T::Literal { value }, span, Some(tpe), None)
    }
}
