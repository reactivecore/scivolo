//! The types `TypeAssigner` gives applications, type applications, closures and blocks.

use super::{Refusal, Unpickler};
use crate::defn::type_ref;
use crate::tpd::TNodeId;
use crate::typer::seen_from::normalized;
use crate::typer::type_apply::instantiate_poly;
use tasty::{Binder, Kind, SymbolId, Type, TypeId};

impl Unpickler<'_> {
    /// The class of full name `full`.
    pub fn class_symbol(&mut self, full: &str) -> Result<SymbolId, Refusal> {
        let r = self.u.class_ref(full);
        self.u.class_symbol(r).ok_or("a class missing from the library")
    }

    /// The class of full name `full`, referred to from where it is visible.
    pub fn class_type(&mut self, full: &str) -> Result<TypeId, Refusal> {
        let c = self.class_symbol(full)?;
        Ok(type_ref(self.u, c))
    }

    /// The type `fun` applies, seen from its prefix, and the method it refers to.
    fn method_type(&mut self, fun: TNodeId) -> (TypeId, Option<SymbolId>) {
        let tpe = self.tpd.tpe(fun);
        let meth = self.tpd.sym(self.tpd.meth_part(fun));
        match *self.u.types().get(tpe) {
            Type::Sym(p, s) if self.u.symbol(s).kind == Kind::Def => {
                let info = match p.filter(|&p| !matches!(self.u.types().get(p), Type::Package(_))) {
                    Some(p) => self.u.member_info(p, s),
                    None => self.u.info(s),
                };
                (normalized(self.u, info), Some(s))
            }
            _ => (tpe, meth),
        }
    }

    /// The type of `fun(args)`: the result of `fun`'s method type, its parameters replaced by the
    /// arguments' types.
    pub fn apply_type(&mut self, fun: TNodeId, args: &[TNodeId]) -> Result<TypeId, Refusal> {
        if self.tpd.sym(fun).is_some_and(|s| self.u.symbol(s).name == "<init>") {
            return Err("constructor call in a library inline body");
        }
        let (info, meth) = self.method_type(fun);
        let Type::Method { binder, params, result, .. } = self.u.types().get(info).clone() else {
            return Err("application of a value in a library inline body");
        };
        if params.len() != args.len() {
            return Err("application with missing arguments in a library inline body");
        }
        let names: Vec<String> = params.into_iter().map(|(n, _)| n).collect();
        let args: Vec<TypeId> = args.iter().map(|&a| self.tpd.tpe(a)).collect();
        let mut unstable = false;
        let tpe = self.u.map_type(result, &mut |u, part| {
            let i = match *u.types().get(part) {
                Type::Param(b, i) if b == binder => i as usize,
                Type::Sym(None, p)
                    if meth.is_some() && u.symbol(p).owner == meth && u.symbol(p).kind == Kind::Param =>
                {
                    names.iter().position(|n| *n == u.symbol(p).name)?
                }
                _ => return None,
            };
            let arg = *args.get(i)?;
            unstable |= !is_singleton(u, arg);
            Some(arg)
        });
        match unstable {
            true => Err("dependent result of an unstable argument in a library inline body"),
            false => Ok(tpe),
        }
    }

    /// The type of `fun[args]`: the result of `fun`'s polymorphic type instantiated.
    pub fn type_apply_type(&mut self, fun: TNodeId, args: &[TNodeId]) -> Result<TypeId, Refusal> {
        let (info, meth) = self.method_type(fun);
        let Type::Poly { binder, params, result } = self.u.types().get(info).clone() else {
            return Err("type application of a value in a library inline body");
        };
        if params.len() != args.len() {
            return Err("type application with missing arguments in a library inline body");
        }
        let names: Vec<String> = params.into_iter().map(|(n, _)| n).collect();
        let args: Vec<TypeId> = args.iter().map(|&a| self.tpd.tpe(a)).collect();
        Ok(instantiate_poly(self.u, result, (binder, &names), &args, meth))
    }

    /// The function type of the closure of local method `meth` (dotty's `toFunctionType`).
    pub fn function_type(&mut self, meth: TNodeId) -> Result<TypeId, Refusal> {
        let s = self.tpd.sym(meth).ok_or("closure of an unknown method in a library inline body")?;
        let info = self.u.info(s);
        let Type::Method { binder, params, result, contextual, .. } = self.u.types().get(info).clone() else {
            return Err("closure of a method without parameters in a library inline body");
        };
        if mentions_params(self.u, result, binder, s) {
            return Err("dependent closure in a library inline body");
        }
        let kind = if contextual { "ContextFunction" } else { "Function" };
        let class = self.class_symbol(&format!("scala.{kind}{}", params.len()))?;
        let args = params.into_iter().map(|(_, p)| p).chain([result]).collect();
        Ok(crate::defn::applied(self.u, class, args))
    }

    /// The type of a block of locals `locals` and expression type `tpe`: `tpe` widened if it
    /// refers to them (dotty's `avoid`).
    pub fn block_type(&mut self, tpe: TypeId, locals: &[SymbolId]) -> Result<TypeId, Refusal> {
        let mentions = |u: &mut tasty::Universe, t| locals.iter().any(|&l| mentions_symbol(u, t, l));
        if !mentions(self.u, tpe) {
            return Ok(tpe);
        }
        let widened = self.u.widen(tpe);
        match mentions(self.u, widened) {
            true => Err("block of a type referring to its locals in a library inline body"),
            false => Ok(widened),
        }
    }
}

impl Unpickler<'_> {
    /// Refuses tree `t` if its type refers to a match type, which dotty reduces.
    pub fn check_type(&mut self, t: TNodeId) -> Result<(), Refusal> {
        let Some(tpe) = self.tpd.get(t).tpe else { return Ok(()) };
        let mut found = false;
        self.u.map_type(tpe, &mut |u, part| {
            let s = match u.types().get(part).clone() {
                Type::Match(_) => {
                    found = true;
                    return None;
                }
                Type::Sym(_, s) => Some(s),
                Type::TypeRef(p, n) => u.resolve(p, &n, true),
                _ => None,
            };
            if let Some(s) = s.filter(|&s| u.symbol(s).kind == Kind::Type) {
                let info = u.info(s);
                found |= match u.types().get(info).clone() {
                    Type::Alias(a) | Type::Bounds(_, a) => is_match(u, a),
                    other => matches!(other, Type::Lambda { result, .. } if is_match(u, result)),
                };
            }
            None
        });
        match found {
            true => Err("match type in a library inline body"),
            false => Ok(()),
        }
    }
}

/// Whether `t` is a match type, also as the result of a type lambda.
fn is_match(u: &tasty::Universe, t: TypeId) -> bool {
    match *u.types().get(t) {
        Type::Match(_) => true,
        Type::Lambda { result, .. } => is_match(u, result),
        _ => false,
    }
}

/// Whether `t` is a path, a `this` or a constant.
fn is_singleton(u: &tasty::Universe, t: TypeId) -> bool {
    match *u.types().get(t) {
        Type::This(_) | Type::Constant(..) | Type::Str(_) | Type::Package(_) => true,
        Type::Sym(_, s) => u.symbol(s).kind.is_term(),
        _ => false,
    }
}

fn mentions_symbol(u: &mut tasty::Universe, t: TypeId, s: SymbolId) -> bool {
    let mut found = false;
    u.map_type(t, &mut |u, part| {
        found |= matches!(*u.types().get(part), Type::Sym(_, x) if x == s);
        None
    });
    found
}

/// Whether `t` refers to a parameter of binder `binder` or of method `meth`.
fn mentions_params(u: &mut tasty::Universe, t: TypeId, binder: Binder, meth: SymbolId) -> bool {
    let mut found = false;
    u.map_type(t, &mut |u, part| {
        found |= match *u.types().get(part) {
            Type::Param(b, _) => b == binder,
            Type::Sym(None, p) => u.symbol(p).owner == Some(meth) && u.symbol(p).kind == Kind::Param,
            _ => false,
        };
        None
    });
    found
}
