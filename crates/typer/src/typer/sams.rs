// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Single abstract method types that function values also implement (dotty's `SAMType`).

use super::Typer;
use tasty::tags::{ABSTRACT, FINAL, SEALED, TRAIT};
use tasty::{Binding, Kind, SymbolId, Type, TypeId};

/// A single abstract method type: the method's parameter and result types, the base type of its
/// class that a function value implementing it gets (dotty's `samParent`), and the method with
/// the binder and names of its parameters.
pub struct Sam {
    pub params: Vec<TypeId>,
    pub result: TypeId,
    pub parent: TypeId,
    pub clause: SamClause,
}

pub type SamClause = (tasty::Binder, Vec<String>, SymbolId);
/// `parent` and `clause` of a `Sam`.
pub type SamParent = (TypeId, SamClause);

impl Typer<'_> {
    /// The parameter and result types of the single abstract method of `t`, of `n` parameters,
    /// and `t`'s base type of the method's class (dotty's `SAMType`, `samParent`); `Err` for a
    /// single abstract method type outside the subset: with type variables unless `open` and
    /// not of a Java class.
    pub fn sam_parts(&mut self, t: TypeId, n: usize, open: bool) -> Result<Option<Sam>, &'static str> {
        let t = self.zonk(t);
        let Some(sam) = self.sam_shape(t, n)? else { return Ok(None) };
        let open = open && !self.u.class_symbol(sam.parent).is_some_and(|c| self.is_java_class_name(c));
        if !open && !self.is_fully_defined(t) && !self.is_partial_function(t) {
            return Err("single abstract method type outside the subset");
        }
        Ok(Some(sam))
    }

    /// `sam_parts` of `t` with type variables in it, for applicability only (dotty's `SAMType`
    /// in `SAMargOK`).
    pub fn sam_shape(&mut self, t: TypeId, n: usize) -> Result<Option<Sam>, &'static str> {
        let t = self.zonk(t);
        let dealiased = self.u.dealias(t);
        let (unrefined, term_refined) = self.u.without_refinements(dealiased);
        let Some(class) = self.u.class_symbol(unrefined).filter(|_| !term_refined) else { return Ok(None) };
        self.ensure_self(class);
        if self.u.given_self_type(class).is_some() {
            return Ok(None);
        }
        let Some(method) = self.sam_method(class) else { return Ok(None) };
        if self.u.symbol(method).flags.has(tasty::tags::INLINE) {
            return Err("single abstract method type of an inline method");
        }
        if !self.no_args_needed(class) {
            return Ok(None);
        }
        let parent =
            self.u.base_type(unrefined, class).ok_or("single abstract method type outside the subset")?;
        let parent = self
            .without_wildcard_args(parent, class, method)
            .ok_or("single abstract method type of a wildcard argument outside the subset")?;
        let parent = if unrefined != dealiased && parent == unrefined { dealiased } else { parent };
        let info = self.u.member_info(parent, method);
        let info = self.u.with_refined_aliases(info, parent);
        let Some(Binding { binder, params, result, implicit: false, .. }) = self.u.method(info) else {
            return Ok(None);
        };
        if params.len() != n || matches!(self.u.types().get(result), Type::Method { .. } | Type::Poly { .. })
        {
            return Ok(None);
        }
        let (names, params) = params.iter().cloned().unzip();
        Ok(Some(Sam { params, result, parent, clause: (binder, names, method) }))
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
        let mut new_args = Vec::new();
        for (i, &a) in args.iter().enumerate() {
            let Type::Bounds(lo, hi) = *self.u.types().get(a) else {
                new_args.push(a);
                continue;
            };
            let p = tparams.get(i).copied()?;
            let mut seen = (false, false);
            self.occurrences(info, p, 1, &mut seen).then_some(())?;
            new_args.push(if seen == (false, true) { lo } else { hi });
        }
        Some(self.u.intern_type(Type::Applied(tycon, new_args)))
    }

    /// Records in `seen` whether type parameter `p` occurs in `t` at variance `v` covariantly and
    /// whether contravariantly, an invariant occurrence as both; `false` for a form not followed.
    fn occurrences(&mut self, t: TypeId, p: SymbolId, v: i8, seen: &mut (bool, bool)) -> bool {
        match self.u.types().get(t).clone() {
            Type::Sym(_, s) if s == p => {
                seen.0 |= v >= 0;
                seen.1 |= v <= 0;
                true
            }
            Type::Applied(tycon, args) => {
                let Some(class) = self.u.class_symbol(tycon) else { return !self.refers_to(t, &[p]) };
                let variances = self.u.variances(class);
                args.iter().enumerate().all(|(i, &a)| {
                    let w = variances.get(i).copied().unwrap_or(0);
                    self.occurrences(a, p, v * w, seen)
                })
            }
            Type::Method { params, result, .. } => {
                params.iter().all(|&(_, q)| self.occurrences(q, p, -v, seen))
                    && self.occurrences(result, p, v, seen)
            }
            Type::Bounds(lo, hi) => self.occurrences(lo, p, -v, seen) && self.occurrences(hi, p, v, seen),
            Type::ByName(r) => self.occurrences(r, p, v, seen),
            Type::And(a, b) | Type::Or(a, b, _) => {
                self.occurrences(a, p, v, seen) && self.occurrences(b, p, v, seen)
            }
            _ => !self.refers_to(t, &[p]),
        }
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
    /// to no arguments, one of a repeated parameter too, or none (dotty's `samClass`).
    fn no_args_needed(&mut self, c: SymbolId) -> bool {
        if !self.takes_no_args(c) {
            return false;
        }
        if !self.u.symbol(c).flags.has(TRAIT) {
            return true;
        }
        let first = self.u.parents(c).first().and_then(|&p| self.u.class_symbol(p));
        first.is_none_or(|p| self.takes_no_args(p))
    }

    fn takes_no_args(&mut self, c: SymbolId) -> bool {
        self.complete(c);
        let ctors = self.u.decl(c, "<init>").unwrap_or_default();
        let mut applicable = 0;
        for ctor in &ctors {
            let info = self.u.info(*ctor);
            let info = self.u.strip_poly(info);
            let Type::Method { params, result, .. } = self.u.types().get(info).clone() else { continue };
            let no_args = match params[..] {
                [] => true,
                [(_, p)] => self.repeated_elem(p).is_some(),
                _ => false,
            };
            applicable += usize::from(no_args && !matches!(self.u.types().get(result), Type::Method { .. }));
        }
        ctors.is_empty() || applicable == 1
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
