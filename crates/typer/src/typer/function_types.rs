//! Function types `A => B` and `A ?=> B` (dotty's `typedFunctionType`) and the parts of function
//! types.

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
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

    /// The parameter and result types of `t` if it is a `FunctionN` of `n` parameters.
    pub fn function_parts(&mut self, t: TypeId, n: usize) -> Option<Parts> {
        self.applied_function(t, "scala.Function").filter(|(params, _)| params.len() == n)
    }

    /// The parameter and result types of `t` if it is a `ContextFunctionN`.
    pub fn context_function_parts(&mut self, t: TypeId) -> Option<Parts> {
        self.applied_function(t, "scala.ContextFunction")
    }

    /// The parameter and result types of `t` if it is an application of class `{base}N`.
    fn applied_function(&mut self, t: TypeId, base: &str) -> Option<Parts> {
        let t = self.zonk(t);
        let t = self.u.dealias(t);
        let Type::Applied(tycon, args) = self.u.types().get(t).clone() else { return None };
        let class = self.u.class_symbol(tycon)?;
        let (&result, params) = args.split_last()?;
        (self.u.full_name(class) == format!("{base}{}", params.len())).then(|| (params.to_vec(), result))
    }

    /// The expected parameter and result types of function value `id` of `n` parameters, a
    /// context function if `contextual`, under `pt`, and the single abstract method type it
    /// implements; `Err` with the tree of a refusal.
    pub fn expected_function_parts(
        &mut self,
        id: NodeId,
        pt: Pt,
        n: usize,
        contextual: bool,
    ) -> Result<(Option<Parts>, Option<TypeId>), TNodeId> {
        let Pt::Type(p) = pt else { return Ok((None, None)) };
        if contextual {
            return Ok((self.context_function_parts(p).filter(|(params, _)| params.len() == n), None));
        }
        if let Some(parts) = self.function_parts(p, n) {
            return Ok((Some(parts), None));
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
            Ok(Some(s)) => Ok((Some((s.params, s.result)), Some(s.parent))),
            Ok(None) => Ok((None, None)),
            Err(what) => Err(self.unsupported(id, what)),
        }
    }

    /// Whether `t` has a part left open by an expected type (dotty's `WildcardType`).
    pub fn has_proto_wildcard(&mut self, t: TypeId) -> bool {
        let mut found = false;
        self.u.map_type(t, &mut |u, part| {
            found |= matches!(u.types().get(part), Type::Wildcard(_));
            None
        });
        found
    }

    /// Type `t` as dotty's messages show it: function types as `(A, B) => R` or `A ?=> R`.
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

    /// Function type `(args) => body` as `FunctionN[args, body]`, `(args) ?=> body` of modifiers
    /// `mods` as `ContextFunctionN[args, body]`.
    pub fn typed_function_type(
        &mut self,
        id: NodeId,
        (args, body): (List, NodeId),
        mods: Option<(ModsId, Box<[bool]>)>,
        ctx: Ctx,
    ) -> TNodeId {
        let contextual = match mods {
            None => false,
            Some((m, erased)) => {
                let flags = self.ast.modifiers(m).flags;
                if erased.contains(&true) || flags.is(f::IMPURE) || !flags.is(f::GIVEN) {
                    return self.unsupported(id, "erased or impure function type");
                }
                true
            }
        };
        let arg_ids = self.ast.items(args).to_vec();
        let n = arg_ids.len();
        let class = if contextual { self.context_function_class(n) } else { self.function_class(n) };
        let Some(class) = class else {
            return self.unsupported(id, "function type of that many parameters");
        };
        let mut typed: Vec<TNodeId> = arg_ids.iter().map(|&a| self.typed_type(a, ctx)).collect();
        typed.push(self.typed_type(body, ctx));
        if typed.iter().any(|&t| matches!(self.tpd.tree(t), T::ByNameTypeTree { .. })) {
            return self.unsupported(id, "by-name function parameter");
        }
        let erased = self.resolve_full_name("scala.compiletime.Erased", true);
        let derives_erased = |t: &mut Self, p: TNodeId| {
            let tpe = t.tpd.tpe(p);
            erased.zip(t.u.class_symbol(tpe)).is_some_and(|(e, c)| t.u.derives_from(c, e))
        };
        if typed[..n].iter().any(|&p| derives_erased(self, p)) {
            return self.unsupported(id, "erased or impure function type");
        }
        let span = self.ast.span(id);
        let first = arg_ids.first().copied().unwrap_or(body);
        let tycon = self.type_tree(class, self.ast.span(first).start_pos(), false);
        let arg_types: Vec<TypeId> = typed.iter().map(|&t| self.tpd.tpe(t)).collect();
        let tpe = self.u.intern_type(Type::Applied(class, arg_types));
        let args = self.tpd.list(&typed);
        self.tpd.node(T::AppliedTypeTree { tpt: tycon, args }, span, Some(tpe), None)
    }
}
