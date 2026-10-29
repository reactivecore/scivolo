// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/package.scala
//   compiler/src/dotty/tools/dotc/printing/RefinedPrinter.scala
//! Applied types: tuples `(A, B)`, functions `A => B` and `A ?=> B`, repeated `A*`, infix
//! types `A & B`, `A | B`, `A *: T`, else `F[A, B]`.

use super::{DOT, GLOBAL, INFIX, Printer, paren};
use crate::tags::INFIX as INFIX_FLAG;
use crate::types::{Type, TypeId};

/// The precedence of infix operator `op` (dotty's `parsing.precedence`).
fn precedence(op: &str) -> u8 {
    match op.chars().next().unwrap_or(' ') {
        c if c.is_alphabetic() || c == '_' || c == '$' => 1,
        '|' => 2,
        '^' => 3,
        '&' => 4,
        '=' | '!' => 5,
        '<' | '>' => 6,
        ':' => 7,
        '+' | '-' => 8,
        '*' | '/' | '%' => 9,
        _ => 10,
    }
}

/// The number of parameters of function class `full`, and whether it is a context function.
fn function_arity(full: &str) -> Option<(usize, bool)> {
    let name = full.strip_prefix("scala.")?;
    let name = name.strip_prefix("Impure").unwrap_or(name);
    let (name, contextual) = match name.strip_prefix("Context") {
        Some(n) => (n, true),
        None => (name, false),
    };
    Some((name.strip_prefix("Function")?.parse().ok()?, contextual))
}

impl Printer<'_> {
    pub fn applied(&mut self, tycon: TypeId, args: &[TypeId], prec: u8) -> String {
        if let Some(named) = self.named_tuple(tycon, args) {
            return named;
        }
        if let Some(elems) = self.tuple_elems(tycon, args)
            && elems.len() >= 2
        {
            return format!("({})", self.args_text(&elems));
        }
        let name = self.full_name_of(tycon).unwrap_or_default();
        if name == "scala.<repeated>" && args.len() == 1 {
            return format!("{}*", self.text(args[0], DOT));
        }
        if let Some((arity, contextual)) = function_arity(&name)
            && arity + 1 == args.len()
        {
            return self.function(args, contextual, prec);
        }
        if let Some(op) = self.infix_op(tycon, args.len()) {
            return self.infix(&op, args[0], args[1], prec);
        }
        format!("{}[{}]", self.text(tycon, DOT), self.args_text(args))
    }

    /// `l op r` with the parentheses its associativity needs.
    pub fn infix(&mut self, op: &str, l: TypeId, r: TypeId, prec: u8) -> String {
        let right = op.ends_with(':');
        let own = precedence(op);
        let left_prec = if right || self.assoc_mismatch(l, right) { own + 1 } else { own };
        let right_prec = if !right || self.assoc_mismatch(r, right) { own + 1 } else { own };
        let text = format!("{} {op} {}", self.arg_text(l, left_prec), self.arg_text(r, right_prec));
        paren(own, prec, text)
    }

    fn assoc_mismatch(&mut self, t: TypeId, right: bool) -> bool {
        match self.u.types().get(t).clone() {
            Type::Applied(c, args) => {
                self.infix_op(c, args.len()).is_some_and(|op| op.ends_with(':') != right)
            }
            Type::And(..) | Type::Or(..) => right,
            _ => false,
        }
    }

    /// The name of type constructor `tycon` applied to `arity` arguments if it is shown infix.
    fn infix_op(&mut self, tycon: TypeId, arity: usize) -> Option<String> {
        if arity != 2 {
            return None;
        }
        let s = match self.u.types().get(tycon).clone() {
            Type::Sym(_, s) => s,
            Type::TypeRef(p, n) => self.u.resolve(p, &n, true)?,
            _ => return None,
        };
        let sym = self.u.symbol(s);
        let symbolic = !sym.name.starts_with(|c: char| c.is_alphabetic());
        (symbolic || sym.flags.has(INFIX_FLAG)).then(|| sym.name.clone())
    }

    fn function(&mut self, args: &[TypeId], contextual: bool, prec: u8) -> String {
        let (params, result) = args.split_at(args.len() - 1);
        let params = match params {
            [p] if !contextual && !self.is_tuple_class(*p) => self.arg_text(*p, INFIX),
            _ => format!("({})", self.args_text(params)),
        };
        let arrow = if contextual { "?=>" } else { "=>" };
        let text = format!("{params} {arrow} {}", self.arg_text(result[0], GLOBAL));
        paren(GLOBAL, prec, text)
    }

    /// Whether `t` is a function class or `PolyFunction`, the parent of a dependent or
    /// polymorphic function type.
    pub fn is_function_class(&mut self, t: TypeId) -> bool {
        let name = self.full_name_of(t).unwrap_or_default();
        name == "scala.PolyFunction" || function_arity(&name).is_some()
    }

    /// The `apply` method type `t` of a function refinement as a function: `(x: A) => x.type`,
    /// `[A] => (x: A) => A`.
    pub fn as_function(&mut self, t: TypeId) -> String {
        let text = match self.u.types().get(t).clone() {
            Type::Method { binder, params, result, implicit, .. } => {
                self.binders.push((binder, params.iter().map(|(n, _)| n.clone()).collect()));
                let arrow = if implicit { "?=>" } else { "=>" };
                format!("({}) {arrow} {}", self.params(&params), self.as_function(result))
            }
            Type::Poly { binder, params, result } => {
                self.binders.push((binder, params.iter().map(|(n, _)| n.clone()).collect()));
                format!("[{}] => {}", self.type_params(&params), self.as_function(result))
            }
            _ => return self.text(t, GLOBAL),
        };
        self.binders.pop();
        text
    }

    /// The type constructor `F` of eta-expansion `[X, Y] =>> F[X, Y]`, unless `F[X, Y]` shows
    /// as a tuple, function, repeated or infix type.
    pub fn eta_contracted(&mut self, t: TypeId) -> Option<TypeId> {
        let Type::Lambda { binder, params, result } = self.u.types().get(t).clone() else { return None };
        let Type::Applied(tycon, args) = self.u.types().get(result).clone() else { return None };
        let own = |i: usize, a: &Type| match a {
            Type::Param(b, j) => *b == binder && *j as usize == i,
            Type::Local(_, n) => *n == params[i].0,
            _ => false,
        };
        let types = self.u.types();
        let all_own =
            args.len() == params.len() && args.iter().enumerate().all(|(i, &a)| own(i, types.get(a)));
        let name = self.full_name_of(tycon).unwrap_or_default();
        let special = name == "scala.<repeated>"
            || function_arity(&name).is_some()
            || self.tuple_elems(tycon, &args).is_some()
            || self.infix_op(tycon, args.len()).is_some();
        (all_own && !special).then_some(tycon)
    }

    fn is_tuple_class(&mut self, t: TypeId) -> bool {
        match self.u.types().get(t).clone() {
            Type::Applied(c, args) => {
                let name = self.full_name_of(c).unwrap_or_default();
                name.strip_prefix("scala.Tuple").is_some_and(|n| n.parse() == Ok(args.len()))
            }
            _ => false,
        }
    }

    /// The element types of tuple `tycon[args]`: `TupleN` or `*:` ending in `EmptyTuple`.
    pub fn tuple_elems(&mut self, tycon: TypeId, args: &[TypeId]) -> Option<Vec<TypeId>> {
        let name = self.full_name_of(tycon)?;
        if name.strip_prefix("scala.Tuple").is_some_and(|n| n.parse() == Ok(args.len())) {
            return Some(args.to_vec());
        }
        let ("scala.*:", [head, tail]) = (name.as_str(), args) else { return None };
        let mut elems = vec![*head];
        match self.u.types().get(*tail).clone() {
            Type::Applied(c, rest) => elems.extend(self.tuple_elems(c, &rest)?),
            _ if self.full_name_of(*tail).as_deref() == Some("scala.EmptyTuple") => {}
            _ => return None,
        }
        Some(elems)
    }

    fn args_text(&mut self, args: &[TypeId]) -> String {
        let texts: Vec<String> = args.iter().map(|&a| self.arg_text(a, GLOBAL)).collect();
        texts.join(", ")
    }

    /// Type argument `t`: `?` and its bounds for a wildcard.
    pub fn arg_text(&mut self, t: TypeId, prec: u8) -> String {
        match *self.u.types().get(t) {
            Type::Bounds(lo, hi) if lo != hi => format!("?{}", self.rhs(t)),
            Type::Bounds(lo, _) => self.text(lo, prec),
            _ => self.text(t, prec),
        }
    }
}
