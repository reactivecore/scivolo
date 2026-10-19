// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/ConstFold.scala
//! Operations on constants typed by their results (dotty's `ConstFold`, which also runs when
//! trees are copied).

use super::Inlining;
use super::purity::PRIMITIVE_OWNERS;
use crate::tpd::{TNodeId, Tree as T, constant_type_of};
use crate::typer::const_ops::{fold_binop, fold_unop};

impl Inlining<'_> {
    /// Gives selection or application `t` the constant type of its operation on constants, if it
    /// is one.
    pub fn fold_constants(&mut self, t: TNodeId) {
        let folded = match *self.tpd.tree(t) {
            T::Select { qualifier, name } => {
                let owner = self.tpd.sym(t).and_then(|s| self.u.symbol(s).owner).map(|o| self.u.full_name(o));
                let primitive = owner.is_some_and(|o| PRIMITIVE_OWNERS.contains(&o.as_str()));
                let x = if primitive { self.constant_operand(qualifier) } else { None };
                x.and_then(|x| fold_unop(self.names.text(name.name), &x))
            }
            T::Apply { fun, args } => match (self.tpd.tree(fun).clone(), self.tpd.items(args)) {
                (T::Select { qualifier, name }, &[arg]) => {
                    match (self.constant_operand(qualifier), self.constant_operand(arg)) {
                        (Some(x), Some(y)) => fold_binop(self.names.text(name.name), &x, &y),
                        _ => None,
                    }
                }
                _ => None,
            },
            _ => None,
        };
        if let Some(c) = folded {
            self.tpd.get_mut(t).tpe = Some(constant_type_of(self.u, &c));
        }
    }
}
