//! Constants in inline expansions: the constants trees denote by their types, the literals that
//! replace them, and the trees of singleton types (dotty's `ConstantValue`, `constToLiteral`,
//! `tpd.singleton`).

use super::{Inlining, Unsupported};
use crate::tpd::{TNodeId, Tree as T, constant_type_of};
use syntax::ast::constant::Constant as C;
use syntax::ast::{Span, TName};
use tasty::{Const, Type, TypeId, constant};

impl Inlining<'_> {
    /// `t` as a literal if its type is a constant and it is idempotent (dotty's `constToLiteral`).
    pub fn const_to_literal(&mut self, t: TNodeId) -> Result<TNodeId, Unsupported> {
        let Some(tpe) = self.tpd.get(t).tpe else { return Ok(t) };
        let Some(value) = self.constant_of_type(tpe) else { return Ok(t) };
        if !self.is_idempotent_expr(t) {
            return Err((self.tpd.span(t), "inline operation on constants with side effects"));
        }
        let tpe = constant_type_of(self.u, &value);
        Ok(self.tpd.node(T::Literal { value }, self.tpd.span(t), Some(tpe), None))
    }

    /// The constant tree `t` denotes by its type (dotty's `ConstantValue`).
    pub fn constant_of(&mut self, t: TNodeId) -> Option<C> {
        let tpe = self.tpd.get(t).tpe?;
        self.constant_of_type(tpe)
    }

    /// The constant of type `t`, of a constant type or a reference to a term of one.
    pub fn constant_of_type(&mut self, t: TypeId) -> Option<C> {
        match self.u.types().get(t).clone() {
            Type::Str(s) => Some(C::String(s.encode_utf16().collect())),
            Type::Constant(tag, bits) => Some(match constant(tag, bits)? {
                Const::Unit => C::Unit,
                Const::Bool(b) => C::Boolean(b),
                Const::Byte(n) => C::Int(n as i32),
                Const::Short(n) => C::Int(n as i32),
                Const::Char(c) => C::Char(c as u16),
                Const::Int(n) => C::Int(n),
                Const::Long(n) => C::Long(n),
                Const::Float(f) => C::Float(f),
                Const::Double(d) => C::Double(d),
                _ => return None,
            }),
            Type::ByName(r) => self.constant_of_type(r),
            Type::Sym(_, s) if self.u.symbol(s).kind.is_term() => {
                let info = self.u.info(s);
                let constant = matches!(self.u.types().get(info), Type::Constant(..) | Type::Str(_));
                constant.then(|| self.constant_of_type(info))?
            }
            _ => None,
        }
    }

    /// A tree of singleton type `t` at `span`: a literal or a reference to a local term.
    pub fn singleton(&mut self, t: TypeId, span: Span) -> Option<TNodeId> {
        match *self.u.types().get(t) {
            Type::Constant(..) | Type::Str(_) => {
                let value = self.constant_of_type(t)?;
                Some(self.tpd.node(T::Literal { value }, span, Some(t), None))
            }
            Type::Sym(None, s) => {
                let name = self.names.intern(&self.u.symbol(s).name.clone());
                let name = TName { name, is_type: false };
                Some(self.tpd.node(T::Ident { name }, span, Some(t), Some(s)))
            }
            _ => None,
        }
    }
}
