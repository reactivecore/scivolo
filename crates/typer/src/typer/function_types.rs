// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Function types `A => B` and `A ?=> B` (dotty's `typedFunctionType`) and the parts of function
//! types.

use super::dependent_params::mentions_params;
use super::type_ops::TypeOps;
use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use names::nme;
use syntax::ast::flags as f;
use syntax::ast::tree::ModsId;
use syntax::ast::{List, NodeId};
use tasty::{Type, TypeId};

/// The parameter and result types of a function type.
type Parts = (Vec<TypeId>, TypeId);

impl Typer<'_> {
    /// The class `scala.FunctionN` of `n` parameters.
    pub fn function_class(&mut self, n: usize) -> Option<TypeId> {
        self.class_of_full_name(&format!("scala.Function{n}"))
    }

    /// The class `scala.ContextFunctionN` of `n` parameters.
    pub fn context_function_class(&mut self, n: usize) -> Option<TypeId> {
        self.class_of_full_name(&format!("scala.ContextFunction{n}"))
    }

    fn class_of_full_name(&mut self, full_name: &str) -> Option<TypeId> {
        let c = self.resolve_full_name(full_name, true)?;
        Some(crate::defn::type_ref(self.u, c))
    }

    /// The expected parameter and result types of function value `id` of `n` parameters, a
    /// context function if `contextual`, under `pt`, and the single abstract method type it
    /// implements with that method's parameter clause; `Err` with the tree of a refusal.
    pub fn expected_function_parts(
        &mut self,
        id: NodeId,
        pt: Pt,
        n: usize,
        contextual: bool,
    ) -> Result<(Option<Parts>, Option<super::sams::SamParent>), TNodeId> {
        let Pt::Type(p) = pt else { return Ok((None, None)) };
        let z = self.zonk(p);
        let p = self.var_upper(z);
        let p = match *self.u.ty(p) {
            Type::Or(..) | Type::And(..) | Type::Flexible(_) => match self.function_type_in(p) {
                Some(f) => f,
                None => return Ok((None, None)),
            },
            _ => p,
        };
        if contextual {
            return Ok((self.context_function_parts(p).filter(|(params, _)| params.len() == n), None));
        }
        if let Some(parts) = self.function_parts(p, n) {
            return Ok((Some(parts), None));
        }
        if self.u.var(z).is_some() {
            return Ok((None, None));
        }
        if self.has_proto_wildcard(p) && !self.is_partial_function(p) {
            return Err(
                self.unsupported(id, "function against an underspecified single abstract method type")
            );
        }
        match self.sam_parts(p, n, true) {
            Ok(Some(s)) if self.context_function_parts(s.result).is_some() => {
                Err(self.unsupported(id, "single abstract method with a context function result"))
            }
            Ok(Some(s)) => Ok((Some((s.params, s.result)), Some((s.parent, s.clause)))),
            Ok(None) => Ok((None, None)),
            Err(what) => Err(self.unsupported(id, what)),
        }
    }

    /// Function type `(args) => body` as `FunctionN[args, body]`, `(args) ?=> body` of modifiers
    /// `mods` as `ContextFunctionN[args, body]`, an impure one as `ImpureFunctionN` or
    /// `ImpureContextFunctionN` unless dependent or erased (dotty's `typedFunctionType`).
    pub fn typed_function_type(
        &mut self,
        id: NodeId,
        (args, body): (List, NodeId),
        mods: Option<(ModsId, Box<[bool]>)>,
        ctx: Ctx,
    ) -> TNodeId {
        if let syntax::ast::Tree::CapturesAndResult { refs, parent } = *self.ast.tree(body) {
            return self.typed_retaining_function(id, (refs, parent), ctx);
        }
        let arg_ids = self.ast.items(args).to_vec();
        let n = arg_ids.len();
        let (contextual, erased, impure) = match mods {
            None => (false, vec![false; n], false),
            Some((m, erased)) => {
                let flags = self.ast.modifiers(m).flags;
                (flags.is(f::GIVEN), erased.to_vec(), flags.is(f::IMPURE))
            }
        };
        let dependent =
            arg_ids.first().is_some_and(|&a| matches!(self.ast.tree(a), syntax::ast::Tree::ValDef { .. }));
        if erased.contains(&true) {
            let typed = self.typed_erased_function_type(id, (&arg_ids, body), (&erased, contextual), ctx);
            return typed.unwrap_or_else(|| self.unsupported(id, "erased function type"));
        }
        let class = match (contextual, impure && !dependent) {
            (false, false) => self.function_class(n),
            (true, false) => self.context_function_class(n),
            (false, true) => self.class_of_full_name(&format!("scala.ImpureFunction{n}")),
            (true, true) => self.class_of_full_name(&format!("scala.ImpureContextFunction{n}")),
        };
        let Some(class) = class else {
            return self.unsupported(id, "function type of that many parameters");
        };
        if dependent {
            return self.typed_dependent_function_type(id, (&arg_ids, body), (class, contextual), ctx);
        }
        let parts = arg_ids.iter().chain([&body]);
        let (mut typed, vars): (Vec<TNodeId>, Vec<_>) =
            parts.enumerate().map(|(i, &a)| self.typed_pattern_type_arg(a, (class, i), ctx)).unzip();
        if matches!(self.tpd.tree(typed[n]), T::ByNameTypeTree { .. }) {
            return self.unsupported(id, "by-name function parameter");
        }
        if typed[..n].iter().any(|&p| {
            let tpe = self.tpd.tpe(p);
            self.derives_erased_class(tpe)
        }) {
            let typed = self.typed_erased_function_type(id, (&arg_ids, body), (&erased, contextual), ctx);
            return typed.unwrap_or_else(|| self.unsupported(id, "erased function type"));
        }
        let span = self.ast.span(id);
        let first = arg_ids.first().copied().unwrap_or(body);
        let tycon = self.type_tree(class, self.ast.span(first).start_pos(), false);
        let arg_types: Vec<TypeId> = typed.iter().map(|&t| self.tpd.tpe(t)).collect();
        let arg_types = self.pattern_bound_args(class, &mut typed, &vars, arg_types, ctx);
        let tpe = self.u.intern_applied(class, &arg_types);
        let args = self.tpd.list(&typed);
        self.tpd.node(T::AppliedTypeTree { tpt: tycon, args }, span, Some(tpe), None)
    }
}

impl TypeOps<'_> {
    /// Splits `t` into its parameter and result types if it is a `FunctionN` of `n` parameters,
    /// zonked and dealiased.
    pub fn function_parts(&mut self, t: TypeId, n: usize) -> Option<Parts> {
        self.applied_function(t, "scala.Function").filter(|(params, _)| params.len() == n)
    }

    /// Splits `t` into its parameter and result types if it is a `ContextFunctionN`, possibly
    /// refined by an `apply` whose result does not refer to its parameters (dotty's
    /// `dropDependentRefinement`).
    pub fn context_function_parts(&mut self, t: TypeId) -> Option<Parts> {
        let z = self.zonk(t);
        let d = self.u.dealias(z);
        if let Type::Refined(parent, name, info) = *self.u.ty(d)
            && self.u.name(name) == nme::APPLY
            && let Type::Method { binder, result, .. } = *self.u.ty(info)
            && !mentions_params(self.u, result, binder, None)
        {
            return self.applied_function(parent, "scala.ContextFunction");
        }
        self.applied_function(t, "scala.ContextFunction")
    }

    /// Checks whether `t` has a part left open by an expected type (dotty's `WildcardType`).
    pub fn has_proto_wildcard(&mut self, t: TypeId) -> bool {
        let mut found = false;
        self.u.map_type(t, &mut |u, part| {
            found |= matches!(u.ty(part), Type::Wildcard(_));
            None
        });
        found
    }

    /// Shows type `t` as dotty's messages do: function types as `(A, B) => R` or `A ?=> R`.
    pub fn show_type(&mut self, t: TypeId) -> String {
        let t = self.zonk(t);
        let (parts, arrow) = match self.context_function_parts(t) {
            Some(parts) => (Some(parts), "?=>"),
            None => (self.applied_function(t, "scala.Function"), "=>"),
        };
        if let Some((params, result)) = parts {
            let params: Vec<String> = params.iter().map(|&p| self.show_type(p)).collect();
            let result = self.show_type(result);
            return match params.as_slice() {
                [p] if !p.contains("=>") => format!("{p} {arrow} {result}"),
                _ => format!("({}) {arrow} {result}", params.join(", ")),
            };
        }
        self.u.show(t)
    }
}
