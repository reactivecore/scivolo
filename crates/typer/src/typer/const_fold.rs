//! Typing operations on constants as their constant results (dotty's `ConstFold`).

use super::Typer;
use super::const_ops::{fold_binop, fold_unop};
use super::constants::of_type;
use crate::tpd::{TNodeId, Tree as T, constant_type_of};
use syntax::ast::constant::Constant as C;
use tasty::tags::CLASSconst;
use tasty::{PRIMITIVE_VALUE_CLASSES, Type, TypeId};

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
                Type::Constant(tag, bits) => return of_type(tag, bits),
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

    /// Application `tree` of a binary operation, typed as its result on two constants, or of
    /// `getClass` on a primitive value, typed as the class constant.
    pub fn fold_apply(&mut self, tree: TNodeId) {
        let T::Apply { fun, args } = *self.tpd.tree(tree) else { return };
        if let T::TypeApply { fun: get_class, .. } = *self.tpd.tree(fun)
            && let T::Select { qualifier, name } = *self.tpd.tree(get_class)
            && self.text(name.name) == "getClass"
            && self.tpd.items(args).is_empty()
        {
            let widened = self.tpd.tpe(qualifier);
            let widened = self.u.widen(widened);
            if self.is_primitive_value_type(widened) {
                let class = Type::Constant(CLASSconst, i64::from(widened.0));
                self.tpd.get_mut(tree).tpe = Some(self.u.intern_type(class));
            }
            return;
        }
        let T::Select { qualifier, name } = *self.tpd.tree(fun) else { return };
        let &[arg] = self.tpd.items(args) else { return };
        let op = self.text(name.name).to_owned();
        let (Some(x), Some(y)) = (self.constant_of(qualifier), self.constant_of(arg)) else { return };
        if let Some(c) = fold_binop(&op, &x, &y) {
            self.tpd.get_mut(tree).tpe = Some(constant_type_of(self.u, &c));
        }
    }

    /// Whether `t` is a reference to a primitive value class (dotty's `isPrimitiveValueType`).
    fn is_primitive_value_type(&mut self, t: TypeId) -> bool {
        if !matches!(self.u.types().get(t), Type::Sym(..) | Type::TypeRef(..)) {
            return false;
        }
        let Some(class) = self.u.class_symbol(t) else { return false };
        PRIMITIVE_VALUE_CLASSES.contains(&self.u.full_name(class).as_str())
    }
}
