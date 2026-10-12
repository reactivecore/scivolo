//! Constant types, through the type operations of `scala.compiletime.ops` on constants (dotty's
//! `TypeComparer.constValue`, `TypeEval.tryCompiletimeConstantFold`): `int`, `long`, `boolean`,
//! `string` and `any`; `float` and `double` are outside the subset.

use super::Inlining;
use crate::typer::{fold_binop, fold_unop};
use syntax::ast::constant::Constant as C;
use tasty::{Type, TypeId};

impl Inlining<'_> {
    /// The constant type `t` normalizes to.
    pub fn const_value(&mut self, t: TypeId) -> Option<C> {
        if let Some(c) = self.constant_of_type(t) {
            return Some(c);
        }
        let d = self.u.dealias(t);
        if d != t {
            return self.const_value(d);
        }
        let Type::Applied(tycon, args) = self.u.types().get(t).clone() else { return None };
        let Type::Sym(_, op) = *self.u.types().get(tycon) else { return None };
        let owner = self.u.full_name(self.u.symbol(op).owner?);
        let module = owner.strip_prefix("scala.compiletime.ops.")?.strip_suffix('$')?.to_owned();
        let name = self.u.symbol(op).name.clone();
        let args: Vec<C> = args.into_iter().map(|a| self.const_value(a)).collect::<Option<_>>()?;
        fold_type_op(&module, &name, &args)
    }
}

/// Type operation `name` of object `module` of `scala.compiletime.ops` applied to `args`.
fn fold_type_op(module: &str, name: &str, args: &[C]) -> Option<C> {
    match (module, name, args) {
        ("any", "==" | "!=", [a, b]) if same_kind(a, b) => Some(C::Boolean((a == b) == (name == "=="))),
        ("any" | "int", "ToString", [a]) => to_string(a),
        ("int", "S", [C::Int(n)]) if *n >= 0 && *n < i32::MAX => Some(C::Int(n + 1)),
        ("int", _, [C::Int(_)]) | ("long", _, [C::Long(_)]) => unary(name, &args[0]),
        ("int", _, [C::Int(_), C::Int(_)]) | ("long", _, [C::Long(_), C::Long(_)]) => {
            binary(name, &args[0], &args[1])
        }
        ("boolean", "!", [C::Boolean(a)]) => Some(C::Boolean(!a)),
        ("boolean", "&&" | "||" | "^", [a @ C::Boolean(_), b @ C::Boolean(_)]) => fold_binop(name, a, b),
        ("string", "+", [a @ C::String(_), b @ C::String(_)]) => fold_binop("+", a, b),
        ("string", "Length", [C::String(s)]) => Some(C::Int(s.len() as i32)),
        ("string", "CharAt", [C::String(s), C::Int(i)]) => {
            s.get(usize::try_from(*i).ok()?).map(|&c| C::Char(c))
        }
        ("string", "Substring", [C::String(s), C::Int(b), C::Int(e)]) => {
            let (b, e) = (usize::try_from(*b).ok()?, usize::try_from(*e).ok()?);
            Some(C::String(s.get(b..e)?.iter().copied().collect()))
        }
        _ => None,
    }
}

fn unary(name: &str, a: &C) -> Option<C> {
    match (name, a) {
        ("Abs", C::Int(n)) => Some(C::Int(n.wrapping_abs())),
        ("Abs", C::Long(n)) => Some(C::Long(n.wrapping_abs())),
        ("Negate", _) => fold_unop("unary_-", a),
        ("NumberOfLeadingZeros", C::Int(n)) => Some(C::Int(n.leading_zeros() as i32)),
        ("NumberOfLeadingZeros", C::Long(n)) => Some(C::Int(n.leading_zeros() as i32)),
        ("ToInt", _) => fold_unop("toInt", a),
        ("ToLong", _) => fold_unop("toLong", a),
        _ => None,
    }
}

fn binary(name: &str, a: &C, b: &C) -> Option<C> {
    let (C::Long(x), C::Long(y)) = (widened(a), widened(b)) else { return None };
    let op = match name {
        "Min" => return Some(narrowed(a, x.min(y))),
        "Max" => return Some(narrowed(a, x.max(y))),
        "BitwiseAnd" => "&",
        "BitwiseOr" => "|",
        "+" | "-" | "*" | "/" | "%" | "<" | ">" | ">=" | "<=" | "^" | ">>" | "<<" | ">>>" => name,
        _ => return None,
    };
    fold_binop(op, a, b)
}

fn widened(c: &C) -> C {
    match *c {
        C::Int(n) => C::Long(n.into()),
        _ => c.clone(),
    }
}

fn narrowed(like: &C, n: i64) -> C {
    match like {
        C::Int(_) => C::Int(n as i32),
        _ => C::Long(n),
    }
}

/// `a.toString` for the constants whose text is the same in Rust.
fn to_string(a: &C) -> Option<C> {
    let text = match a {
        C::Int(n) => n.to_string(),
        C::Long(n) => n.to_string(),
        C::Boolean(b) => b.to_string(),
        C::String(s) => return Some(C::String(s.clone())),
        C::Char(c) => String::from_utf16(&[*c]).ok()?,
        _ => return None,
    };
    Some(C::String(text.encode_utf16().collect()))
}

/// Whether `a` and `b` are constants of one type that is not `Float` or `Double`.
fn same_kind(a: &C, b: &C) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b) && !matches!(a, C::Float(_) | C::Double(_))
}
