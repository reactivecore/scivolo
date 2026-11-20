// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Single abstract method types that function values also implement (dotty's `SAMType`).

use super::Typer;
use names::nme;
use tasty::tags::{ABSTRACT, FINAL, PRIVATE, SEALED, TRAIT};
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

pub type SamClause = (tasty::Binder, Vec<tasty::NameId>, SymbolId);
/// `parent` and `clause` of a `Sam`.
pub type SamParent = (TypeId, SamClause);

impl Typer<'_> {
    /// The parameter and result types of the single abstract method of `t`, of `n` parameters,
    /// and `t`'s base type of the method's class (dotty's `SAMType`, `samParent`); `Err` for a
    /// single abstract method type outside the subset: with type variables unless `open`.
    pub fn sam_parts(&mut self, t: TypeId, n: usize, open: bool) -> Result<Option<Sam>, &'static str> {
        let t = self.zonk(t);
        let Some(sam) = self.sam_shape(t, n)? else { return Ok(None) };
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
        if term_refined && !self.only_term_refinements(dealiased, unrefined) {
            return Ok(None);
        }
        let Some(class) = self.u.class_symbol(unrefined) else { return Ok(None) };
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
        if term_refined && !self.u.is_subtype(parent, dealiased) {
            return Ok(None);
        }
        let parent =
            if !term_refined && unrefined != dealiased && parent == unrefined { dealiased } else { parent };
        self.complete(method);
        let info = self.u.member_info(parent, method);
        let info = self.u.with_refined_aliases(info, parent);
        let Some(Binding { binder, params, result, implicit: false, .. }) = self.u.method(info) else {
            return Ok(None);
        };
        if params.len() != n || matches!(self.u.ty(result), Type::Method { .. } | Type::Poly { .. }) {
            return Ok(None);
        }
        let (names, params) = params.iter().cloned().unzip();
        Ok(Some(Sam { params, result, parent, clause: (binder, names, method) }))
    }

    /// The single abstract method of class `c`, a trait or abstract class neither final nor
    /// sealed; `apply` of `PartialFunction`.
    pub fn sam_method(&mut self, c: SymbolId) -> Option<SymbolId> {
        let flags = self.u.symbol(c).flags;
        if !(flags.has(TRAIT) || flags.has(ABSTRACT)) || flags.has(FINAL) || flags.has(SEALED) {
            return None;
        }
        if self.is_partial_function_class(c) {
            return self.class_members(c, nme::APPLY).first().copied();
        }
        let [method] = self.abstract_methods(c)[..] else { return None };
        Some(method)
    }

    /// Whether class `c`, and the first parent of a trait, have exactly one constructor applicable
    /// to no arguments, one of a repeated parameter too, or none (dotty's `samClass`); a private
    /// constructor of a Java class does not count.
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
        let ctors = self.u.decls_named(c, nme::CONSTRUCTOR);
        let mut applicable = 0;
        for &ctor in &ctors {
            self.complete(ctor);
            let info = self.u.info(ctor);
            let info = self.u.strip_poly(info);
            let Some((params, result)) = self.u.method(info).map(|m| (m.params.to_vec(), m.result)) else {
                continue;
            };
            let no_args = match params[..] {
                [] => true,
                [(_, p)] => self.repeated_elem(p).is_some(),
                _ => false,
            };
            let java_private = self.u.is_java_defined(ctor) && self.u.symbol(ctor).flags.has(PRIVATE);
            applicable += usize::from(no_args && self.u.method(result).is_none() && !java_private);
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
                if matches!(s.kind, Kind::Def | Kind::Val)
                    && s.name != nme::CONSTRUCTOR
                    && !names.contains(&s.name)
                {
                    names.push(s.name.clone());
                }
            }
        }
        let object = self.u.defn().object;
        let any = self.u.defn().any;
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
