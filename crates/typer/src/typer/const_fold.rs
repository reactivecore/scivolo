//! Typing operations on constants as their constant results (dotty's `ConstFold`).

use super::Typer;
use super::const_ops::{fold_binop, fold_unop};
use crate::tpd::{TNodeId, Tree as T, constant_type_of};
use syntax::ast::constant::Constant as C;
use tasty::Type;
use tasty::tags::*;

impl Typer<'_> {
    /// The constant `tree` denotes, by its form or its type (dotty's `ConstantTree`).
    pub fn constant_of(&mut self, tree: TNodeId) -> Option<C> {
        match *self.tpd.tree(tree) {
            T::Literal { value: C::Null } => return Some(C::Null),
            T::Typed { expr, .. } => return self.constant_of(expr),
            _ => {}
        }
        let mut t = self.tpd.get(tree).tpe?;
        loop {
            t = match self.u.types().get(t).clone() {
                Type::ByName(r) => r,
                Type::Sym(_, s) if !self.u.is_type_symbol(s) => match self.method_info(t) {
                    Some(info) if info != t => info,
                    _ => return None,
                },
                Type::Str(s) => return Some(C::String(s.encode_utf16().collect())),
                Type::Constant(tag, bits) => return constant(tag, bits),
                _ => return None,
            }
        }
    }

    /// Selection `tree` of unary operation `name`, typed as its result on a constant.
    pub fn fold_select(&mut self, tree: TNodeId, name: &str) {
        let T::Select { qualifier, .. } = *self.tpd.tree(tree) else { return };
        if let Some(c) = self.constant_of(qualifier).and_then(|x| fold_unop(name, &x)) {
            self.tpd.get_mut(tree).tpe = Some(constant_type_of(self.u, &c));
        }
    }

    /// Application `tree` of a binary operation, typed as its result on two constants.
    pub fn fold_apply(&mut self, tree: TNodeId) {
        let T::Apply { fun, args } = *self.tpd.tree(tree) else { return };
        let T::Select { qualifier, name } = *self.tpd.tree(fun) else { return };
        let &[arg] = self.tpd.items(args) else { return };
        let op = self.text(name.name).to_owned();
        let (Some(x), Some(y)) = (self.constant_of(qualifier), self.constant_of(arg)) else { return };
        if let Some(c) = fold_binop(&op, &x, &y) {
            self.tpd.get_mut(tree).tpe = Some(constant_type_of(self.u, &c));
        }
    }
}

/// The constant of a constant type of tag `tag` and bits `bits`.
fn constant(tag: u8, bits: i64) -> Option<C> {
    Some(match tag {
        UNITconst => C::Unit,
        TRUEconst => C::Boolean(true),
        FALSEconst => C::Boolean(false),
        CHARconst => C::Char(bits as u16),
        INTconst => C::Int(bits as i32),
        LONGconst => C::Long(bits),
        FLOATconst => C::Float(f32::from_bits(bits as u32)),
        DOUBLEconst => C::Double(f64::from_bits(bits as u64)),
        NULLconst => C::Null,
        _ => return None,
    })
}
