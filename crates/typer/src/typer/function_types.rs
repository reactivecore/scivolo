//! Function types `A => B` (dotty's `typedFunctionType`), the parts of function types, and the
//! single abstract method types that function values also implement (dotty's `SAMType`).

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{List, NodeId};
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
    /// The class `scala.FunctionN` of `n` parameters.
    pub fn function_class(&mut self, n: usize) -> Option<TypeId> {
        let c = self.resolve_full_name(&format!("scala.Function{n}"), true)?;
        Some(crate::defn::type_ref(self.u, c))
    }

    /// The parameter and result types of `t` if it is a `FunctionN` of `n` parameters.
    pub fn function_parts(&mut self, t: TypeId, n: usize) -> Option<(Vec<TypeId>, TypeId)> {
        let t = self.zonk(t);
        let t = self.u.dealias(t);
        let Type::Applied(tycon, args) = self.u.types().get(t).clone() else { return None };
        let class = self.u.class_symbol(tycon)?;
        (self.u.full_name(class) == format!("scala.Function{n}") && args.len() == n + 1)
            .then(|| (args[..n].to_vec(), args[n]))
    }

    /// The parameter and result types of the single abstract method of `t`, of `n` parameters,
    /// and `t`'s base type of the method's class (dotty's `SAMType`, `samParent`); `Err` for a
    /// single abstract method type outside the subset: of an abstract class, a trait with a class
    /// parent, with type variables or method type parameters.
    pub fn sam_parts(&mut self, t: TypeId, n: usize) -> Result<Option<Sam>, &'static str> {
        let t = self.zonk(t);
        let dealiased = self.u.dealias(t);
        let Some(class) = self.u.class_symbol(dealiased) else { return Ok(None) };
        let flags = self.u.symbol(class).flags;
        if !(flags.has(TRAIT) || flags.has(ABSTRACT)) || flags.has(FINAL) || flags.has(SEALED) {
            return Ok(None);
        }
        let [method] = self.abstract_methods(class)[..] else { return Ok(None) };
        if !flags.has(TRAIT) || !self.is_fully_defined(t) {
            return Err("single abstract method type outside the subset");
        }
        let b = self.u.builtins();
        let class_parent =
            self.u.linearization(class).into_iter().any(|p| {
                !self.u.symbol(p).flags.has(TRAIT) && p != b.object && p != b.any && p != b.matchable
            });
        let mut method_tparam = false;
        self.u.map_type(t, &mut |u, part| {
            if let Type::Sym(_, s) = *u.types().get(part) {
                let owner = u.symbol(s).owner;
                method_tparam |= u.symbol(s).kind == Kind::TypeParam
                    && owner.is_some_and(|o| u.symbol(o).kind == Kind::Def);
            }
            None
        });
        if class_parent || method_tparam {
            return Err("single abstract method type outside the subset");
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
                if self.is_concrete(m) || self.is_type_symbol(m) {
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

    /// Type `t` as dotty's messages show it: function types as `(A, B) => R`.
    pub fn show_type(&mut self, t: TypeId) -> String {
        let t = self.zonk(t);
        if let Type::Applied(tycon, args) = self.u.types().get(t).clone()
            && let Some(class) = self.u.class_symbol(tycon)
            && self.u.full_name(class) == format!("scala.Function{}", args.len() - 1)
        {
            let (params, result) = args.split_at(args.len() - 1);
            let params: Vec<String> = params.iter().map(|&p| self.show_type(p)).collect();
            let result = self.show_type(result[0]);
            return match params.as_slice() {
                [p] if !p.contains("=>") => format!("{p} => {result}"),
                _ => format!("({}) => {result}", params.join(", ")),
            };
        }
        self.u.show(t)
    }

    /// Function type `(args) => body` as `FunctionN[args, body]`.
    pub fn typed_function_type(&mut self, id: NodeId, args: List, body: NodeId, ctx: Ctx) -> TNodeId {
        let arg_ids = self.ast.items(args).to_vec();
        let Some(class) = self.function_class(arg_ids.len()) else {
            return self.unsupported(id, "function type of that many parameters");
        };
        let mut typed: Vec<TNodeId> = arg_ids.iter().map(|&a| self.typed_type(a, ctx)).collect();
        typed.push(self.typed_type(body, ctx));
        if typed.iter().any(|&t| matches!(self.tpd.tree(t), T::ByNameTypeTree { .. })) {
            return self.unsupported(id, "by-name function parameter");
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
