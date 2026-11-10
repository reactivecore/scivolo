// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Nullables.scala
//! The conditionals of null tests (dotty's `notNullConditional`, `CompareNull`).

use super::super::Typer;
use super::{Flow, Nullables, union};
use crate::ctx::Ctx;
use crate::tpd::{List, TNodeId, Tpd, Tree as T};
use syntax::ast::constant::Constant;
use tasty::{TypeId, Universe};

/// Holds the references known not to be null if a condition is true and if it is false: `x` and
/// none for `x != null && x.nonEmpty`.
#[derive(Default)]
pub struct Conditional {
    pub if_true: Vec<TypeId>,
    pub if_false: Vec<TypeId>,
}

impl Typer<'_> {
    /// Gives `ctx` of the argument of `f` what its receiver tells if `f` is `&&` or `||`: in
    /// `x != null && x.nonEmpty` the argument knows `x` is not null (dotty's
    /// `nullableInArgContext`).
    pub fn nullable_in_arg_ctx(&mut self, f: TNodeId, ctx: Ctx) -> Ctx {
        let T::Select { qualifier, .. } = *self.tpd.tree(f) else { return ctx };
        let when = match boolean_op(&self.tpd, self.u, f).as_deref() {
            Some("&&") => true,
            Some("||") => false,
            _ => return ctx,
        };
        self.nullable_ctx_if(qualifier, when, ctx)
    }
}

impl Nullables {
    /// Finds the references known not to be null if condition `t` is true and if it is false.
    pub fn not_null_conditional(&mut self, f: &mut Flow, t: TNodeId) -> Conditional {
        match f.tpd.tree(t).clone() {
            T::Block { expr, .. } | T::Inlined { expansion: expr, .. } => self.not_null_conditional(f, expr),
            T::Select { qualifier, .. } if boolean_op(f.tpd, f.types.u, t).as_deref() == Some("unary_!") => {
                let Conditional { if_true, if_false } = self.not_null_conditional(f, qualifier);
                Conditional { if_true: if_false, if_false: if_true }
            }
            T::Apply { fun, args } => {
                if let Some((operand, equal)) = compare_null(f, fun, args) {
                    return match self.tracked(f, operand) {
                        Some(r) if equal => Conditional { if_true: Vec::new(), if_false: vec![r] },
                        Some(r) => Conditional { if_true: vec![r], if_false: Vec::new() },
                        None => Conditional::default(),
                    };
                }
                let (T::Select { qualifier: x, .. }, [y]) = (f.tpd.tree(fun).clone(), f.tpd.items(args))
                else {
                    return Conditional::default();
                };
                let y = *y;
                let op = boolean_op(f.tpd, f.types.u, fun);
                let (x, y) = (self.not_null_conditional(f, x), self.not_null_conditional(f, y));
                let meet = |a: Vec<TypeId>, b: &[TypeId]| a.into_iter().filter(|r| b.contains(r)).collect();
                match op.as_deref() {
                    Some("&&") => Conditional {
                        if_true: union(x.if_true, &y.if_true),
                        if_false: meet(x.if_false, &y.if_false),
                    },
                    Some("||") => Conditional {
                        if_true: meet(x.if_true, &y.if_true),
                        if_false: union(x.if_false, &y.if_false),
                    },
                    _ => Conditional::default(),
                }
            }
            _ => Conditional::default(),
        }
    }
}

/// Finds the operand of null test `fun(args)` and whether it tests equality: `x` and true for
/// `x == null` or `null eq x` (dotty's `CompareNull`).
fn compare_null(f: &mut Flow, fun: TNodeId, args: List) -> Option<(TNodeId, bool)> {
    let &[arg] = f.tpd.items(args) else { return None };
    let (op, l) = match *f.tpd.tree(fun) {
        T::Select { qualifier, .. } => (fun, qualifier),
        T::Apply { fun: op, args: inner } => match *f.tpd.items(inner) {
            [l] => (op, l),
            _ => return None,
        },
        _ => return None,
    };
    let u = &mut *f.types.u;
    let m = f.tpd.sym(op)?;
    let (name, owner) = (u.symbol(m).name.clone(), u.symbol(m).owner?);
    let b = u.defn();
    let known = match (op == fun, name.as_str()) {
        (true, "==" | "!=") => owner == b.any,
        (true, "eq" | "ne") => owner == b.object || owner == f.defn.object,
        (false, "eq" | "ne") => u.module_class(f.defn.predef) == Some(owner),
        _ => false,
    };
    let operand = match (is_null_literal(f.tpd, arg), is_null_literal(f.tpd, l)) {
        _ if !known => return None,
        (true, _) => l,
        (_, true) => arg,
        _ => return None,
    };
    Some((operand, matches!(name.as_str(), "==" | "eq")))
}

/// Tells whether `t` is the literal `null`.
pub fn is_null_literal(tpd: &Tpd, t: TNodeId) -> bool {
    matches!(tpd.tree(t), T::Literal { value: Constant::Null })
}

/// Finds the name of the `Boolean` method `f` selects.
fn boolean_op(tpd: &Tpd, u: &Universe, f: TNodeId) -> Option<String> {
    let m = tpd.sym(f)?;
    let owner = u.symbol(m).owner?;
    (u.full_name(owner) == crate::defn::BOOLEAN).then(|| u.symbol(m).name.clone())
}
