// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//   compiler/src/dotty/tools/dotc/core/TypeEval.scala
//! `scala.compiletime.ops` applications on constants folded (dotty's
//! `tryCompiletimeConstantFold`, for `int`, `long`, `boolean` and `any`), and compared
//! (`compareCompiletimeAppliedType`).

use super::super::Cmp;
use crate::Kind;
use crate::tags::{FALSEconst, INTconst, LONGconst, TRUEconst};
use crate::types::{Type, TypeId};

/// A constant an operation takes or gives.
#[derive(Clone, Copy, PartialEq)]
enum C {
    Int(i32),
    Long(i64),
    Bool(bool),
}

impl Cmp<'_> {
    /// The object of `scala.compiletime.ops` and the name of the operation `tycon` refers to.
    pub fn compiletime_op(&mut self, tycon: TypeId) -> Option<(String, String)> {
        let s = match *self.u.types().get(tycon) {
            Type::Sym(_, s) => s,
            Type::TypeRef(..) => self.u.resolve_ref(tycon)?,
            _ => return None,
        };
        if self.u.symbol(s).kind != Kind::Type {
            return None;
        }
        let owner = self.u.full_name(self.u.symbol(s).owner?);
        let module = owner.strip_prefix("scala.compiletime.ops.")?.strip_suffix('$')?.to_owned();
        Some((module, self.u.symbol(s).name.clone()))
    }

    /// Whether `tycon` is `scala.compiletime.ops.int.S`.
    pub fn is_succ(&mut self, tycon: TypeId) -> bool {
        self.compiletime_op(tycon).is_some_and(|(m, n)| m == "int" && n == "S")
    }

    /// The constant type operation application `t` folds to.
    pub fn fold_compiletime(&mut self, t: TypeId) -> Option<TypeId> {
        let Type::Applied(tycon, args) = self.u.types().get(t).clone() else { return None };
        let (module, name) = self.compiletime_op(tycon)?;
        let mut values = Vec::new();
        for a in args {
            let a = self.widen_term_refs(a);
            let a = self.normal(a);
            values.push(match *self.u.types().get(a) {
                Type::Constant(INTconst, n) => C::Int(n as i32),
                Type::Constant(LONGconst, n) => C::Long(n),
                Type::Constant(TRUEconst, _) => C::Bool(true),
                Type::Constant(FALSEconst, _) => C::Bool(false),
                _ => return None,
            });
        }
        let folded = fold(&module, &name, &values)?;
        Some(self.u.intern_type(match folded {
            C::Int(n) => Type::Constant(INTconst, n as i64),
            C::Long(n) => Type::Constant(LONGconst, n),
            C::Bool(true) => Type::Constant(TRUEconst, 0),
            C::Bool(false) => Type::Constant(FALSEconst, 0),
        }))
    }

    /// `t` with term references replaced by their underlying types, repeatedly.
    pub fn widen_term_refs(&mut self, mut t: TypeId) -> TypeId {
        for _ in 0..64 {
            let term = match self.u.types().get(t) {
                Type::TermRef(..) => true,
                Type::Sym(_, s) => self.u.symbol(*s).kind.is_term(),
                _ => false,
            };
            match term.then(|| self.u.widen_once(t)).flatten() {
                Some(w) => t = w,
                None => return t,
            }
        }
        t
    }

    /// `op <: other` (or `other <: op` if `from_below`) for an operation application `op`:
    /// `S[n]` as `n + 1`, `S[x]` against `n` as `x` against `n - 1`, others folded.
    pub fn compare_compiletime(&mut self, op: TypeId, other: TypeId, from_below: bool) -> bool {
        let Type::Applied(tycon, args) = self.u.types().get(op).clone() else { return false };
        let (x, y) = if self.is_succ(tycon) {
            let [arg] = args[..] else { return false };
            match self.nat_value(arg) {
                Some(n) if n < i32::MAX as i64 => {
                    (self.u.intern_type(Type::Constant(INTconst, n + 1)), other)
                }
                _ => match self.nat_value(other) {
                    Some(n) if n > 0 => (arg, self.u.intern_type(Type::Constant(INTconst, n - 1))),
                    _ => return false,
                },
            }
        } else {
            match self.fold_compiletime(op) {
                Some(folded) => (folded, other),
                None => return false,
            }
        };
        if from_below { self.recur(y, x) } else { self.recur(x, y) }
    }
}

fn fold(module: &str, name: &str, args: &[C]) -> Option<C> {
    use C::*;
    Some(match (module, name, args) {
        ("any", "==", [a, b]) => Bool(equal(a, b)),
        ("any", "!=", [a, b]) => Bool(!equal(a, b)),
        ("int", "S", [Int(n)]) if *n >= 0 && *n < i32::MAX => Int(n + 1),
        ("int", "Abs", [Int(n)]) => Int(n.wrapping_abs()),
        ("int", "Negate", [Int(n)]) => Int(n.wrapping_neg()),
        ("int", "ToLong", [Int(n)]) => Long(*n as i64),
        ("long", "Abs", [Long(n)]) => Long(n.wrapping_abs()),
        ("long", "Negate", [Long(n)]) => Long(n.wrapping_neg()),
        ("long", "ToInt", [Long(n)]) => Int(*n as i32),
        ("int", _, [Int(a), Int(b)]) => match int_op(name, *a as i64, *b as i64)? {
            Long(n) => Int(n as i32),
            c => c,
        },
        ("long", _, [Long(a), Long(b)]) => int_op(name, *a, *b)?,
        ("boolean", "!", [Bool(a)]) => Bool(!a),
        ("boolean", "&&", [Bool(a), Bool(b)]) => Bool(*a && *b),
        ("boolean", "||", [Bool(a), Bool(b)]) => Bool(*a || *b),
        ("boolean", "^", [Bool(a), Bool(b)]) => Bool(a ^ b),
        _ => return None,
    })
}

/// `a == b` as Scala compares boxed values: numbers by value, whatever their width.
fn equal(a: &C, b: &C) -> bool {
    let number = |c: &C| match *c {
        C::Int(n) => Some(n as i64),
        C::Long(n) => Some(n),
        C::Bool(_) => None,
    };
    match (number(a), number(b)) {
        (Some(x), Some(y)) => x == y,
        _ => a == b,
    }
}

/// Binary operation `name` on integers, wrapping as the JVM does for the width the caller
/// narrows to.
fn int_op(name: &str, a: i64, b: i64) -> Option<C> {
    use C::*;
    Some(match name {
        "+" => Long(a.wrapping_add(b)),
        "-" => Long(a.wrapping_sub(b)),
        "*" => Long(a.wrapping_mul(b)),
        "/" if b != 0 => Long(a.wrapping_div(b)),
        "%" if b != 0 => Long(a.wrapping_rem(b)),
        "Min" => Long(a.min(b)),
        "Max" => Long(a.max(b)),
        "<" => Bool(a < b),
        ">" => Bool(a > b),
        "<=" => Bool(a <= b),
        ">=" => Bool(a >= b),
        _ => return None,
    })
}
