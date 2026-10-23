// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Choosing among the alternatives a name denotes when it is not applied, by the expected type.

use super::{Pt, Typer};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// The only alternative of `syms` without parameter lists, its type conforming to `pt`
    /// unless that is a function type, else the only one with an implicit clause only, else for a single abstract method type `pt` the only
    /// method taking its parameters (dotty's `resolveOverloaded` without arguments).
    pub fn parameterless_alternative(
        &mut self,
        syms: &[SymbolId],
        prefix: Option<TypeId>,
        pt: Pt,
    ) -> Result<SymbolId, &'static str> {
        if let Pt::Type(p) = pt
            && let Some((args, _)) = (0..=22).find_map(|n| self.function_parts(p, n))
        {
            return self.taking_params(syms, prefix, &args, Some(p));
        }
        let (mut found, mut implicit) = (Vec::new(), Vec::new());
        for &s in syms {
            self.complete(s);
            let tpe = self.ref_type(prefix, s);
            let info = self.method_info(tpe).unwrap_or(tpe);
            let info = self.mapped_info(info, s);
            let result = match self.u.types().get(info).clone() {
                Type::ByName(r) => r,
                Type::Poly { result, .. } => match self.u.types().get(result) {
                    Type::Method { params, implicit: false, .. } if !params.is_empty() => continue,
                    _ => return Err("overloaded reference with a polymorphic alternative"),
                },
                Type::Method { params, result, implicit: false, .. } if params.is_empty() => result,
                Type::Method { result, implicit: true, .. } => {
                    implicit.push((s, result));
                    continue;
                }
                Type::Method { .. } => continue,
                _ => info,
            };
            if self.result_fits(result, pt) {
                found.push(s);
            }
        }
        if found.is_empty() {
            found = implicit.into_iter().filter(|&(_, r)| self.result_fits(r, pt)).map(|(s, _)| s).collect();
        }
        if let (Pt::Type(p), []) = (pt, &found[..])
            && let Some(sam) = (0..=22).find_map(|n| self.sam_shape(p, n).ok().flatten())
        {
            return self.taking_params(syms, prefix, &sam.params, None);
        }
        match found[..] {
            [s] => Ok(s),
            _ => Err("overloaded reference"),
        }
    }

    /// Whether an alternative of result type `result` is a value that may conform to `pt`.
    fn result_fits(&mut self, result: TypeId, pt: Pt) -> bool {
        if matches!(self.u.types().get(result), Type::Method { .. } | Type::Poly { .. }) {
            return false;
        }
        match pt {
            Pt::Type(p) => !self.is_fully_defined(p) || self.u.is_subtype(result, p),
            _ => true,
        }
    }

    /// The only value of `syms` conforming to function type `fun`, else the only method whose
    /// parameters take arguments of types `args` (dotty's `normalizedCompatible` against a
    /// function type, a value more specific than a method, and the `SAMType` case of
    /// `resolveOverloaded1`).
    fn taking_params(
        &mut self,
        syms: &[SymbolId],
        prefix: Option<TypeId>,
        args: &[TypeId],
        fun: Option<TypeId>,
    ) -> Result<SymbolId, &'static str> {
        let (mut fitting, mut values) = (Vec::new(), Vec::new());
        for &s in syms {
            self.complete(s);
            if self.u.symbol(s).kind != Kind::Def {
                let tpe = self.ref_type(prefix, s);
                values.extend(fun.filter(|&f| self.u.is_subtype(tpe, f)).map(|_| s));
                continue;
            }
            let Some((formals, false)) = self.param_types(s, prefix, false) else {
                return Err("overloaded reference against a function type");
            };
            if formals.len() == args.len()
                && args.iter().zip(&formals).all(|(&a, &f)| self.u.is_subtype(a, f))
            {
                fitting.push(s);
            }
        }
        match (&values[..], &fitting[..]) {
            ([s], _) | ([], [s]) => Ok(*s),
            _ => Err("overloaded reference against a function type"),
        }
    }
}
