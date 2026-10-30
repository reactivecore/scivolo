// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Parameter types of function values from the method their body applies (dotty's
//! `calleeType` and `inferredFromTarget` in `typedFunctionValue`).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use syntax::ast::{NodeId, Tree};
use tasty::{Kind, Type, TypeId};

impl Typer<'_> {
    /// Whether the callee's parameter type may give a parameter of expected type `formal` its
    /// type: none, a wildcard without lower bound, or one not fully defined.
    pub fn wants_callee_type(&mut self, formal: Option<TypeId>) -> bool {
        formal.is_none_or(|f| self.is_bottom_wildcard(f) || !self.is_fully_defined(f))
    }

    /// The type of a parameter of expected type `formal` and callee parameter type `target`
    /// (dotty's `inferredFromTarget`): `target`, or `formal` constrained by it and fully defined.
    pub fn callee_formal(&mut self, formal: Option<TypeId>, target: TypeId) -> Option<TypeId> {
        let Some(formal) = formal.filter(|&f| !self.is_bottom_wildcard(f)) else { return Some(target) };
        if self.is_fully_defined(formal) {
            return None;
        }
        self.conforms(formal, target);
        let defined = self.fully_defined(formal).ok()?;
        (self.u.class_symbol(defined) != Some(self.u.builtins().nothing)).then_some(defined)
    }

    fn is_bottom_wildcard(&mut self, formal: TypeId) -> bool {
        let formal = self.zonk(formal);
        let Type::Bounds(lo, _) = *self.u.types().get(formal) else { return false };
        self.u.class_symbol(lo) == Some(self.u.builtins().nothing)
    }

    /// The types of `params` from the method `body` applies, `f(… x1, …, xN …)` with each
    /// parameter an argument once and `f` a path to a single method of fully defined parameter
    /// types not referring to the parameters; `None` otherwise. A qualifier of `f` is typed in
    /// `ctx` and spliced.
    pub fn callee_param_types(&mut self, params: &[NodeId], body: NodeId, ctx: Ctx) -> Option<Vec<TypeId>> {
        let mut b = body;
        while let Tree::Parens { t } = *self.ast.tree(b) {
            b = t;
        }
        let Tree::Apply { fun, args } = *self.ast.tree(b) else { return None };
        let args = self.ast.items(args).to_vec();
        let mut index = Vec::new();
        for &p in params {
            let Tree::ValDef { name, .. } = *self.ast.tree(p) else { return None };
            let is_param = |s: &Self, a: NodeId| {
                let a = if let Tree::Typed { expr, .. } = *s.ast.tree(a) { expr } else { a };
                matches!(*s.ast.tree(a), Tree::Ident { name: n } if n == name)
            };
            let positions: Vec<usize> = (0..args.len()).filter(|&i| is_param(self, args[i])).collect();
            let [i] = positions[..] else { return None };
            if self
                .ast
                .exists_subtree(fun, &|ast, n| matches!(*ast.tree(n), Tree::Ident { name: m } if m == name))
            {
                return None;
            }
            index.push(i);
        }
        let method = self.callee_method(fun, ctx)?;
        let formals = self.u.method(method)?.param_types();
        let mut types = Vec::new();
        for i in index {
            let &p = formals.get(i)?;
            let local = |s: &mut Self, t: TypeId| {
                let mut found = false;
                s.u.map_type(t, &mut |u, part| {
                    found |= matches!(*u.types().get(part), Type::Param(..) | Type::ByName(_))
                        || matches!(*u.types().get(part), Type::Sym(_, s) if u.symbol(s).kind == Kind::Param);
                    None
                });
                found
            };
            if local(self, p) || self.repeated_elem(p).is_some() || !self.is_fully_defined(p) {
                return None;
            }
            types.push(p);
        }
        Some(types)
    }

    /// The info of the single method path `fun` refers to, its qualifier typed in `ctx` and
    /// spliced; a structural method's from its refinement.
    fn callee_method(&mut self, fun: NodeId, ctx: Ctx) -> Option<TypeId> {
        if let Some(&typed) = self.splices.get(&fun) {
            let tpe = self.tpd.tpe(typed);
            return Some(self.method_info(tpe).unwrap_or(tpe));
        }
        match *self.ast.tree(fun) {
            Tree::Ident { name } if !name.is_type => {
                let text = self.text(name.name).to_owned();
                let found = self.find_ref(&text, false, ctx).ok()??;
                let [s] = found.syms[..] else { return None };
                self.complete(s);
                Some(match found.prefix {
                    Some(prefix) => self.u.member_info(prefix, s),
                    None => self.u.info(s),
                })
            }
            Tree::Select { qualifier, name } if self.text(name.name) == "<init>" => {
                let Tree::New { tpt } = *self.ast.tree(qualifier) else { return None };
                let Tree::Ident { name } = *self.ast.tree(tpt) else { return None };
                let text = self.text(name.name).to_owned();
                let [c] = self.find_ref(&text, true, ctx).ok()??.syms[..] else { return None };
                self.complete(c);
                if self.u.symbol(c).kind != Kind::Class || !self.u.type_params(c).is_empty() {
                    return None;
                }
                let [init] = self.class_members(c, "<init>")[..] else { return None };
                self.complete(init);
                Some(self.u.info(init))
            }
            Tree::Select { qualifier, name } if !name.is_type && self.is_path_tree(qualifier) => {
                let text = self.text(name.name).to_owned();
                let qual = self.typed(qualifier, Pt::Any, ctx);
                let qualifier = self.spliced(qual);
                self.ast.get_mut(fun).tree = Tree::Select { qualifier, name };
                let tpe = self.zonk(self.tpd.tpe(qual));
                let tpe = self.u.widen(tpe);
                let syms: Vec<_> = self
                    .type_members(tpe, &text)
                    .into_iter()
                    .filter(|&s| !self.u.is_type_symbol(s))
                    .collect();
                match syms[..] {
                    [s] => {
                        self.complete(s);
                        Some(self.u.member_info(tpe, s))
                    }
                    [] => self.u.term_refinement_named(tpe, &text),
                    _ => None,
                }
            }
            Tree::Apply { fun, .. } => {
                let method = self.callee_method(fun, ctx)?;
                match *self.u.types().get(method) {
                    Type::Method { result, implicit: false, .. } => Some(result),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Whether `t` is a path of identifiers and selections.
    fn is_path_tree(&self, t: NodeId) -> bool {
        match *self.ast.tree(t) {
            Tree::Ident { .. } | Tree::This { .. } => true,
            Tree::Select { qualifier, .. } => self.is_path_tree(qualifier),
            _ => false,
        }
    }
}
