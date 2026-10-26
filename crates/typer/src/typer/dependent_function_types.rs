// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Dependent function types `FunctionN[A, B'] { def apply(x: A): B }`: made from a method's
//! parameters and result (`toFunctionType`, `nonDependentResultApprox`), and taken apart.

use super::Typer;
use super::avoid_map::Avoided;
use tasty::{Binder, Kind, SymbolId, Type, TypeId};

/// The parameter types and result of an `apply`, and the binder and names of its parameters.
pub type ApplyParts = ((Vec<TypeId>, TypeId), (Binder, Vec<String>));

impl Typer<'_> {
    /// The parameter types and result of the `apply` refining dependent function type `t` of `n`
    /// parameters, a context function type if `contextual`, and the binder and names of its
    /// parameters.
    pub fn dependent_function_parts(&mut self, t: TypeId, n: usize, contextual: bool) -> Option<ApplyParts> {
        let t = self.zonk(t);
        let t = self.u.dealias(t);
        let Type::Refined(parent, name, info) = self.u.types().get(t).clone() else { return None };
        let Type::Method { binder, params, result, .. } = self.u.types().get(info).clone() else {
            return None;
        };
        let parent_parts = match contextual {
            true => self.context_function_parts(parent),
            false => self.function_parts(parent, n),
        };
        if name != "apply" || params.len() != n || parent_parts.is_none() {
            return None;
        }
        let (names, types) = params.into_iter().unzip();
        Some(((types, result), (binder, names)))
    }

    /// The parameters and result of the `apply` of dependent context function type `t`.
    pub fn dependent_context_function(&mut self, t: TypeId) -> Option<(Vec<(String, TypeId)>, TypeId)> {
        let Type::Refined(parent, name, info) = self.u.types().get(t).clone() else { return None };
        let Type::Method { params, result, .. } = self.u.types().get(info).clone() else { return None };
        self.context_function_parts(parent).filter(|_| name == "apply")?;
        Some((params, result))
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
        let approx = self.avoid_at(projected, 1, Avoided::new(params, &[]))?.hi();
        let approx = self.normalized(approx);
        if self.refers_to(approx, params) {
            return None;
        }
        let names = params.iter().zip(&types).map(|(&p, &t)| (self.u.symbol(p).name.clone(), t)).collect();
        let result = self.u.map_type(result, &mut |u, part| match *u.types().get(part) {
            Type::Sym(None, s) => {
                let i = params.iter().position(|&p| p == s)?;
                Some(u.intern_type(Type::Param(binder, i as u32)))
            }
            _ => None,
        });
        let method = Type::Method { binder, params: names, result, implicit: contextual, contextual };
        let method = self.u.intern_type(method);
        let mut args = types;
        args.push(approx);
        let core = self.u.intern_type(Type::Applied(class, args));
        Some(self.u.intern_type(Type::Refined(core, "apply".into(), method)))
    }

    /// `t` at variance `v` with the type members selected from `params` replaced, through class
    /// arguments, refinements and methods: by projections from the parameters' types where they
    /// occur covariantly, by their bounds' intersection contravariantly.
    fn projected(&mut self, t: TypeId, params: &[SymbolId], v: i8) -> TypeId {
        match self.u.types().get(t).clone() {
            Type::Sym(Some(pre), s) if self.u.symbol(s).kind == Kind::Type => {
                match *self.u.types().get(pre) {
                    Type::Sym(None, p) if params.contains(&p) => {
                        let info = self.u.info(p);
                        let bounds = self.u.member_info(info, s);
                        match *self.u.types().get(bounds) {
                            _ if v > 0 => self.u.intern_type(Type::Sym(Some(info), s)),
                            Type::Bounds(lo, hi) if v < 0 => self.u.intern_type(Type::And(lo, hi)),
                            _ => t,
                        }
                    }
                    _ => t,
                }
            }
            Type::Applied(tycon, args) if v > 0 && self.u.class_symbol(tycon).is_none() => {
                let projected = self.projected(tycon, params, v);
                match projected != tycon && !args.iter().any(|&a| self.refers_to(a, params)) {
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
                let args = args
                    .iter()
                    .enumerate()
                    .map(|(i, &a)| match variances.get(i).copied().unwrap_or(0) {
                        0 => a,
                        w => self.projected(a, params, v * w),
                    })
                    .collect();
                self.u.intern_type(Type::Applied(tycon, args))
            }
            Type::Refined(parent, name, info) => {
                let parent = self.projected(parent, params, v);
                let info = self.projected(info, params, v);
                self.u.intern_type(Type::Refined(parent, name, info))
            }
            Type::Method { binder, params: ps, result, implicit, contextual } => {
                let ps = ps.into_iter().map(|(n, p)| (n, self.projected(p, params, -v))).collect();
                let result = self.projected(result, params, v);
                self.u.intern_type(Type::Method { binder, params: ps, result, implicit, contextual })
            }
            _ => t,
        }
    }
}
