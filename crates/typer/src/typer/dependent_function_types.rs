// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Dependent function types `FunctionN[A, B'] { def apply(x: A): B }`: made from a method's
//! parameters and result (`toFunctionType`, `nonDependentResultApprox`), and taken apart.

use super::Typer;
use super::avoid::Avoid;
use tasty::{Binder, Kind, NameId, SymbolId, Type, TypeId};

/// The parameter types and result of an `apply`, and the binder and names of its parameters.
pub type ApplyParts = ((Vec<TypeId>, TypeId), (Binder, Vec<NameId>));

impl Typer<'_> {
    /// The parameter types and result of the `apply` refining dependent function type `t` of `n`
    /// parameters, a context function type if `contextual`, and the binder and names of its
    /// parameters.
    pub fn dependent_function_parts(&mut self, t: TypeId, n: usize, contextual: bool) -> Option<ApplyParts> {
        let t = self.zonk(t);
        let t = self.u.dealias(t);
        let Type::Refined(parent, name, info) = *self.u.ty(t) else { return None };
        let Type::Method { binder, params, result, .. } = *self.u.ty(info) else {
            return None;
        };
        let parent_parts = match contextual {
            true => self.context_function_parts(parent),
            false => self.function_parts(parent, n),
        };
        let params = self.u.params_of(params);
        if self.u.name(name) != "apply" || params.len() != n || parent_parts.is_none() {
            return None;
        }
        let (names, types) = params.iter().copied().unzip();
        Some(((types, result), (binder, names)))
    }

    /// The parameters and result of the `apply` of dependent context function type `t`.
    pub fn dependent_context_function(&mut self, t: TypeId) -> Option<(Vec<(NameId, TypeId)>, TypeId)> {
        let Type::Refined(parent, name, info) = *self.u.ty(t) else { return None };
        let Type::Method { params, result, .. } = *self.u.ty(info) else { return None };
        self.context_function_parts(parent).filter(|_| self.u.name(name) == "apply")?;
        Some((self.u.params_of(params).to_vec(), result))
    }

    /// `class[P1, …, B'] { def apply(params): result }` (dotty's `MethodType.toFunctionType`), the
    /// parameters `params` referred to through `binder` in the refinement; `None` if the result
    /// cannot be approximated without them.
    pub fn dependent_function_of(
        &mut self,
        (class, contextual): (TypeId, bool),
        params: &[SymbolId],
        binder: Binder,
        result: TypeId,
    ) -> Option<TypeId> {
        let types: Vec<TypeId> = params.iter().map(|&p| self.u.info(p)).collect();
        let projected = self.projected(result, params, 1);
        if self.refined_by_dependent_method(projected, params) {
            return None;
        }
        let approx = Avoid::new(params, &[]).at(self, projected, 1)?.hi();
        let approx = self.normalized(approx);
        if self.refers_to(approx, params) {
            return None;
        }
        let names: Vec<_> = params.iter().zip(&types).map(|(&p, &t)| (self.u.interned_name(p), t)).collect();
        let result = self.u.map_type(result, &mut |u, part| match *u.ty(part) {
            Type::Sym(None, s) => {
                let i = params.iter().position(|&p| p == s)?;
                Some(u.intern_type(Type::Param(binder, i as u32)))
            }
            _ => None,
        });
        let method = self.u.intern_method(binder, &names, result, contextual, contextual);
        let mut args = types;
        args.push(approx);
        let core = self.u.intern_applied(class, &args);
        let apply = self.u.intern_name("apply");
        Some(self.u.intern_type(Type::Refined(core, apply, method)))
    }

    /// Whether `t` has a refinement by a method whose type refers to `params` and to its own
    /// parameters.
    fn refined_by_dependent_method(&mut self, t: TypeId, params: &[SymbolId]) -> bool {
        let mut found = Vec::new();
        self.u.map_type(t, &mut |u, part| {
            if let Type::Refined(_, _, info) = *u.ty(part)
                && let Some(m) = u.method(info)
            {
                found.push((info, m.binder, m.result));
            }
            None
        });
        found.into_iter().any(|(info, binder, result)| {
            let mut own = false;
            self.u.map_type(result, &mut |u, part| {
                own |= u.param_ref(part).is_some_and(|(b, _)| b == binder);
                None
            });
            own && self.refers_to(info, params)
        })
    }

    /// `t` at variance `v` with the type members selected from `params` replaced, through class
    /// arguments, refinements and methods: by projections from the parameters' types where they
    /// occur covariantly, by their bounds' intersection contravariantly.
    fn projected(&mut self, t: TypeId, params: &[SymbolId], v: i8) -> TypeId {
        let node = *self.u.ty(t);
        match node {
            Type::Sym(Some(pre), s) if self.u.symbol(s).kind == Kind::Type => match self.u.sym_ref(pre) {
                Some((None, p)) if params.contains(&p) => {
                    let info = self.u.info(p);
                    let bounds = self.u.member_info(info, s);
                    match self.u.bounds(bounds) {
                        _ if v > 0 => self.u.intern_type(Type::Sym(Some(info), s)),
                        Some((lo, hi)) if v < 0 => self.u.intern_type(Type::And(lo, hi)),
                        _ => t,
                    }
                }
                _ => t,
            },
            Type::Applied(tycon, args) if v > 0 && self.u.class_symbol(tycon).is_none() => {
                let projected = self.projected(tycon, params, v);
                let list = self.u.types_of(args).to_vec();
                match projected != tycon && !list.into_iter().any(|a| self.refers_to(a, params)) {
                    true => self.u.intern_type(Type::Applied(projected, args)),
                    false => t,
                }
            }
            Type::Or(a, b, soft) => {
                let (a, b) = (self.projected(a, params, v), self.projected(b, params, v));
                if a == b { a } else { self.u.intern_type(Type::Or(a, b, soft)) }
            }
            Type::And(a, b) => {
                let (a, b) = (self.projected(a, params, v), self.projected(b, params, v));
                self.u.intern_type(Type::And(a, b))
            }
            Type::Annotated(a, annot) if self.refers_to(annot, params) => self.projected(a, params, v),
            Type::Applied(tycon, args) if self.u.class_symbol(tycon).is_some() => {
                let class = self.u.class_symbol(tycon).expect("a class");
                let variances = self.u.variances(class);
                let args: Vec<_> = (self.u.types_of(args).to_vec().into_iter().enumerate())
                    .map(|(i, a)| match variances.get(i).copied().unwrap_or(0) {
                        0 => a,
                        w => self.projected(a, params, v * w),
                    })
                    .collect();
                self.u.intern_applied(tycon, &args)
            }
            Type::Refined(parent, name, info) => {
                let parent = self.projected(parent, params, v);
                let info = self.projected(info, params, v);
                self.u.intern_type(Type::Refined(parent, name, info))
            }
            Type::Method { binder, params: ps, result, implicit, contextual } => {
                let ps: Vec<_> = (self.u.params_of(ps).to_vec().into_iter())
                    .map(|(n, p)| (n, self.projected(p, params, -v)))
                    .collect();
                let result = self.projected(result, params, v);
                self.u.intern_method(binder, &ps, result, implicit, contextual)
            }
            _ => t,
        }
    }
}
