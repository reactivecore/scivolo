//! Single abstract method types that function values also implement (dotty's `SAMType`).

use super::Typer;
use tasty::tags::{ABSTRACT, FINAL, SEALED, TRAIT};
use tasty::{Kind, SymbolId, Type, TypeId};

/// A single abstract method type: the method's parameter and result types, and the base type of
/// its class that a function value implementing it gets (dotty's `samParent`).
pub struct Sam {
    pub params: Vec<TypeId>,
    pub result: TypeId,
    pub parent: TypeId,
}

impl Typer<'_> {
    /// The parameter and result types of the single abstract method of `t`, of `n` parameters,
    /// and `t`'s base type of the method's class (dotty's `SAMType`, `samParent`); `Err` for a
    /// single abstract method type outside the subset: with type variables unless `open` and
    /// not of a Java class.
    pub fn sam_parts(&mut self, t: TypeId, n: usize, open: bool) -> Result<Option<Sam>, &'static str> {
        let t = self.zonk(t);
        let dealiased = self.u.dealias(t);
        let Some(class) = self.u.class_symbol(dealiased) else { return Ok(None) };
        self.ensure_self(class);
        if self.u.given_self_type(class).is_some() {
            return Ok(None);
        }
        let Some(method) = self.sam_method(class) else { return Ok(None) };
        if self.u.symbol(method).flags.has(tasty::tags::INLINE) {
            return Err("single abstract method type of an inline method");
        }
        let open = open && !self.is_java_class_name(class);
        if !open && !self.is_fully_defined(t) && !self.is_partial_function(t) {
            return Err("single abstract method type outside the subset");
        }
        if !self.no_args_needed(class)? {
            return Ok(None);
        }
        let parent = self.u.base_type(t, class).ok_or("single abstract method type outside the subset")?;
        let parent = self
            .without_wildcard_args(parent, class, method)
            .ok_or("single abstract method type of a wildcard argument outside the subset")?;
        let info = self.u.member_info(parent, method);
        let Type::Method { params, result, implicit: false, .. } = self.u.types().get(info).clone() else {
            return Ok(None);
        };
        if params.len() != n || matches!(self.u.types().get(result), Type::Method { .. } | Type::Poly { .. })
        {
            return Ok(None);
        }
        Ok(Some(Sam { params: params.into_iter().map(|(_, p)| p).collect(), result, parent }))
    }

    /// `parent`, an application of class `c`, its wildcard arguments replaced by their lower
    /// bound where the type parameter is a parameter type of `method` and occurs nowhere else,
    /// by their upper bound where it occurs at most as the result type (dotty's `samParent`);
    /// `None` otherwise or if `method` is not declared in `c`.
    fn without_wildcard_args(&mut self, parent: TypeId, c: SymbolId, method: SymbolId) -> Option<TypeId> {
        let Type::Applied(tycon, args) = self.u.types().get(parent).clone() else { return Some(parent) };
        if !args.iter().any(|&a| matches!(self.u.types().get(a), Type::Bounds(..))) {
            return Some(parent);
        }
        if self.u.symbol(method).owner != Some(c) {
            return None;
        }
        let tparams = self.u.type_params(c);
        let info = self.u.info(method);
        let Type::Method { params, result, .. } = self.u.types().get(info).clone() else { return None };
        let occurs = |t: &mut Self, ty: TypeId, p: SymbolId| {
            let mut found = false;
            t.u.map_type(ty, &mut |u, part| {
                found |= matches!(*u.types().get(part), Type::Sym(_, s) if s == p);
                None
            });
            found
        };
        let mut new_args = Vec::new();
        for (i, &a) in args.iter().enumerate() {
            let Type::Bounds(lo, hi) = *self.u.types().get(a) else {
                new_args.push(a);
                continue;
            };
            let p = tparams.get(i).copied()?;
            let is_p = |t: &Self, ty: TypeId| matches!(*t.u.types().get(ty), Type::Sym(_, s) if s == p);
            let direct = params.iter().any(|&(_, pt)| is_p(self, pt));
            let nested = params.iter().any(|&(_, pt)| !is_p(self, pt) && occurs(self, pt, p));
            let in_result = occurs(self, result, p);
            let result_nested = in_result && !is_p(self, result);
            new_args.push(match (direct, nested || result_nested, in_result) {
                (true, false, false) => lo,
                (false, false, _) => hi,
                _ => return None,
            });
        }
        Some(self.u.intern_type(Type::Applied(tycon, new_args)))
    }

    /// The single abstract method of class `c`, a trait or abstract class neither final nor
    /// sealed; `apply` of `PartialFunction`.
    pub fn sam_method(&mut self, c: SymbolId) -> Option<SymbolId> {
        let flags = self.u.symbol(c).flags;
        if !(flags.has(TRAIT) || flags.has(ABSTRACT)) || flags.has(FINAL) || flags.has(SEALED) {
            return None;
        }
        if self.is_partial_function_class(c) {
            return self.class_members(c, "apply").first().copied();
        }
        let [method] = self.abstract_methods(c)[..] else { return None };
        Some(method)
    }

    /// Whether class `c`, and the first parent of a trait, have exactly one constructor applicable
    /// to no arguments, or none (dotty's `samClass`); `Err` for a repeated parameter's.
    fn no_args_needed(&mut self, c: SymbolId) -> Result<bool, &'static str> {
        if !self.takes_no_args(c)? {
            return Ok(false);
        }
        if !self.u.symbol(c).flags.has(TRAIT) {
            return Ok(true);
        }
        let first = self.u.parents(c).first().and_then(|&p| self.u.class_symbol(p));
        first.map_or(Ok(true), |p| self.takes_no_args(p))
    }

    fn takes_no_args(&mut self, c: SymbolId) -> Result<bool, &'static str> {
        self.complete(c);
        let ctors = self.u.decl(c, "<init>").unwrap_or_default();
        let mut applicable = 0;
        for ctor in &ctors {
            let mut info = self.u.info(*ctor);
            if let Type::Poly { result, .. } = *self.u.types().get(info) {
                info = result;
            }
            let Type::Method { params, result, .. } = self.u.types().get(info).clone() else { continue };
            if let [(_, p)] = params[..]
                && self.repeated_elem(p).is_some()
            {
                return Err("single abstract method type of a class with a repeated constructor parameter");
            }
            applicable +=
                usize::from(params.is_empty() && !matches!(self.u.types().get(result), Type::Method { .. }));
        }
        Ok(ctors.is_empty() || applicable == 1)
    }

    /// Whether `t` is a `PartialFunction` type.
    pub fn is_partial_function(&mut self, t: TypeId) -> bool {
        let t = self.zonk(t);
        let t = self.u.dealias(t);
        self.u.class_symbol(t).is_some_and(|c| self.is_partial_function_class(c))
    }

    fn is_partial_function_class(&self, c: SymbolId) -> bool {
        self.u.full_name(c) == "scala.PartialFunction"
    }

    /// The abstract term members of class `c` that do not match members of `Object` (dotty's
    /// `possibleSamMethods`).
    fn abstract_methods(&mut self, c: SymbolId) -> Vec<SymbolId> {
        let mut names: Vec<String> = Vec::new();
        for base in self.u.linearization(c) {
            self.complete(base);
            for d in self.u.decls(base).unwrap_or_default() {
                let s = self.u.symbol(d);
                if matches!(s.kind, Kind::Def | Kind::Val) && s.name != "<init>" && !names.contains(&s.name) {
                    names.push(s.name.clone());
                }
            }
        }
        let object = self.u.builtins().object;
        let any = self.u.builtins().any;
        let site = crate::defn::type_ref(self.u, c);
        let mut out = Vec::new();
        for name in names {
            for m in self.class_members(c, &name) {
                if self.is_concrete(m) || self.u.is_type_symbol(m) {
                    continue;
                }
                let mut in_object = self.class_members(object, &name);
                in_object.extend(self.class_members(any, &name));
                if !in_object.into_iter().any(|o| self.same_member(site, o, m)) {
                    out.push(m);
                }
            }
        }
        out
    }
}
