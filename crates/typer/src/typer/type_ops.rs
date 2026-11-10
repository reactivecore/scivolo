// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//   compiler/src/dotty/tools/dotc/typer/Inferencing.scala
//! Type relations and queries over the universe and the constraint (dotty's `TypeComparer`), for
//! completed symbols; `Typer`'s methods of the same names forward to them.

use super::Typer;
use crate::constraint::Constraint;
use tasty::{ANY, Kind, SymbolId, Type, TypeId, Universe, VarId};

pub struct TypeOps<'a> {
    pub u: &'a mut Universe,
    pub constraint: &'a mut Constraint,
}

impl TypeOps<'_> {
    /// Whether `a` conforms to `b`, constraining type variables; a stable path also with its
    /// wildcards captured.
    pub fn conforms(&mut self, a: TypeId, b: TypeId) -> bool {
        self.u.is_subtype_in(a, b, self.constraint) || self.conforms_captured_path(a, b)
    }

    /// Whether `a <: b` without constraining type variables.
    pub fn frozen_conforms(&mut self, a: TypeId, b: TypeId) -> bool {
        let before = self.constraint.attempt();
        let result = self.conforms(a, b);
        self.constraint.abandon(before);
        result
    }

    /// `t` with instantiated type variables replaced by their instances, normalized.
    pub fn zonk(&mut self, t: TypeId) -> TypeId {
        super::infer::zonked(self.u, self.constraint, t, false)
    }

    /// Whether `t` has no uninstantiated type variables (dotty's `isFullyDefined` with
    /// `ForceDegree.none`).
    pub fn is_fully_defined(&mut self, t: TypeId) -> bool {
        self.open_vars(t).is_empty()
    }

    /// The uninstantiated type variables in `t`.
    pub fn open_vars(&mut self, t: TypeId) -> Vec<VarId> {
        let t = self.zonk(t);
        let mut vars = Vec::new();
        self.u.map_type(t, &mut |u, part| {
            if let Type::Var(v) = *u.types().get(part)
                && !vars.contains(&v)
            {
                vars.push(v);
            }
            None
        });
        vars
    }

    /// Whether `t` is `Any` (dotty's `isExactlyAny`).
    pub fn is_exactly_any(&mut self, t: TypeId) -> bool {
        t == ANY || self.u.class_symbol(t) == Some(self.u.builtins().any)
    }

    pub fn is_singleton(&mut self, t: TypeId) -> bool {
        match *self.u.types().get(t) {
            Type::Constant(..) | Type::Str(_) | Type::This(_) | Type::TermRef(..) | Type::Skolem(..) => true,
            Type::Sym(_, s) => !self.u.is_type_symbol(s),
            _ => false,
        }
    }

    pub fn ref_type(&mut self, prefix: Option<TypeId>, sym: SymbolId) -> TypeId {
        crate::defn::ref_type(self.u, prefix, sym)
    }

    /// The info of type member `s` selected on `pre`: that a refinement of `pre`'s type gives it,
    /// else its own.
    pub fn refined_info(&mut self, pre: Option<TypeId>, s: SymbolId) -> TypeId {
        let refined = pre.and_then(|p| self.u.refinement_info(p, s));
        refined.unwrap_or_else(|| self.u.info(s))
    }

    /// The parts of `t`, `t` included.
    pub fn type_parts(&mut self, t: TypeId) -> Vec<TypeId> {
        let mut parts = Vec::new();
        self.u.map_type(t, &mut |_, part| {
            parts.push(part);
            None
        });
        parts
    }

    /// The result of method type `t` after all its clauses.
    pub fn final_result(&mut self, t: TypeId) -> TypeId {
        match *self.u.types().get(t) {
            Type::Method { result, .. } | Type::Poly { result, .. } => self.final_result(result),
            Type::ByName(r) => r,
            _ => t,
        }
    }

    /// The type after the leading implicit parameter clauses of `t`, references to their
    /// parameters kept (dotty's `stripInferrable` of a method type).
    pub fn strip_implicit(&mut self, mut t: TypeId) -> TypeId {
        while let Type::Method { result, implicit: true, .. } = *self.u.types().get(t) {
            t = result;
        }
        t
    }

    /// The type arguments of `t` if it refers to class `name`.
    pub fn class_args(&mut self, t: TypeId, name: &str) -> Option<Vec<TypeId>> {
        let (tycon, args) = match self.u.types().get(t).clone() {
            Type::Applied(tycon, args) => (tycon, args),
            _ => (t, Vec::new()),
        };
        let class = self.u.class_symbol(tycon)?;
        (self.u.full_name(class) == name).then_some(args)
    }

    /// Finds the class `t` refers to, applied or not; `None` for other types.
    pub fn direct_class(&mut self, t: TypeId) -> Option<SymbolId> {
        let tycon = match self.u.types().get(t) {
            Type::Applied(tycon, _) => *tycon,
            _ => t,
        };
        let c = match self.u.types().get(tycon).clone() {
            Type::Sym(_, c) => c,
            Type::TypeRef(p, n) => self.u.resolve(p, &n, true)?,
            _ => return None,
        };
        (self.u.symbol(c).kind == Kind::Class).then_some(c)
    }

    /// Whether `t` is a `FunctionN` type.
    pub fn is_function_type(&mut self, t: TypeId) -> bool {
        self.applied_function(t, "scala.Function").is_some()
    }

    /// The parameter and result types of `t` if it is an application of class `{base}N`.
    pub fn applied_function(&mut self, t: TypeId, base: &str) -> Option<(Vec<TypeId>, TypeId)> {
        let t = self.zonk(t);
        let t = self.u.dealias(t);
        let Type::Applied(tycon, args) = self.u.types().get(t).clone() else { return None };
        let class = self.u.class_symbol(tycon)?;
        let (&result, params) = args.split_last()?;
        (self.u.full_name(class) == format!("{base}{}", params.len())).then(|| (params.to_vec(), result))
    }
}

impl Typer<'_> {
    pub fn types(&mut self) -> TypeOps<'_> {
        TypeOps { u: self.u, constraint: &mut self.constraint }
    }

    pub fn conforms(&mut self, a: TypeId, b: TypeId) -> bool {
        self.types().conforms(a, b)
    }

    pub fn conforms_captured_path(&mut self, a: TypeId, b: TypeId) -> bool {
        self.types().conforms_captured_path(a, b)
    }

    pub fn zonk(&mut self, t: TypeId) -> TypeId {
        self.types().zonk(t)
    }

    pub fn is_fully_defined(&mut self, t: TypeId) -> bool {
        self.types().is_fully_defined(t)
    }

    pub fn open_vars(&mut self, t: TypeId) -> Vec<VarId> {
        self.types().open_vars(t)
    }

    pub fn wild_approx(&mut self, t: TypeId) -> TypeId {
        self.types().wild_approx(t)
    }

    pub fn is_exactly_any(&mut self, t: TypeId) -> bool {
        self.types().is_exactly_any(t)
    }

    pub fn is_singleton(&mut self, t: TypeId) -> bool {
        self.types().is_singleton(t)
    }

    pub fn ref_type(&mut self, prefix: Option<TypeId>, sym: SymbolId) -> TypeId {
        self.types().ref_type(prefix, sym)
    }

    pub fn refined_info(&mut self, pre: Option<TypeId>, s: SymbolId) -> TypeId {
        self.types().refined_info(pre, s)
    }

    pub fn referenced(&mut self, t: TypeId) -> Option<SymbolId> {
        self.types().referenced(t)
    }

    pub fn value_class(&mut self, t: TypeId) -> Option<String> {
        self.types().value_class(t)
    }

    pub fn widens_to(&mut self, from: TypeId, to: TypeId) -> bool {
        self.types().widens_to(from, to)
    }

    pub fn type_parts(&mut self, t: TypeId) -> Vec<TypeId> {
        self.types().type_parts(t)
    }

    pub fn final_result(&mut self, t: TypeId) -> TypeId {
        self.types().final_result(t)
    }

    pub fn strip_implicit(&mut self, t: TypeId) -> TypeId {
        self.types().strip_implicit(t)
    }

    pub fn is_function_type(&mut self, t: TypeId) -> bool {
        self.types().is_function_type(t)
    }

    pub fn class_args(&mut self, t: TypeId, name: &str) -> Option<Vec<TypeId>> {
        self.types().class_args(t, name)
    }
}
