//! Function value parameters without a type that dotty cannot infer either, reported as
//! `AnonymousFunctionMissingParamType` (dotty's `typedFunctionValue`, `inferredFromTarget`).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::symbols::Completion;
use crate::tpd::TNodeId;
use syntax::ast::{NodeId, Tree};
use tasty::tags::{ABSTRACT, FINAL, OBJECT, SEALED, TRAIT};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Whether `p` is a parameter without a type.
    pub fn is_untyped_param(&self, p: NodeId) -> bool {
        matches!(*self.ast.tree(p), Tree::ValDef { tpt, .. }
            if matches!(self.ast.tree(tpt), Tree::TypeTree) && !self.splices.contains_key(&tpt))
    }

    /// `pt` of function `id` of `params` and `body`, instantiated if it is a type variable, a
    /// parameter has no type and the body gives none.
    pub fn instantiated_pt(&mut self, id: NodeId, (params, body): (&[NodeId], NodeId), pt: Pt) -> Pt {
        let Pt::Type(p) = pt else { return pt };
        let t = self.zonk(p);
        if !matches!(self.u.types().get(t), Type::Var(_))
            || !params.iter().any(|&q| self.is_untyped_param(q))
            || self.may_infer_from_callee(params, body)
        {
            return pt;
        }
        match self.fully_defined(t) {
            Ok(t) => Pt::Type(t),
            Err(what) => {
                self.unsupported(id, what);
                pt
            }
        }
    }

    /// Reports parameter `p` without a type of function `id` of `params` and `body` typed against
    /// `pt` in `ctx`, which gives the parameter none; its type tree. `None` where dotty may still
    /// infer the type or words the message otherwise.
    pub fn missing_param_type(
        &mut self,
        (id, p): (NodeId, NodeId),
        (params, body): (&[NodeId], NodeId),
        pt: Pt,
        ctx: Ctx,
    ) -> Option<TNodeId> {
        let shown = self.missing_param_type_shown(id, (params, body), pt, ctx)?;
        let Tree::ValDef { name, .. } = *self.ast.tree(p) else { return None };
        let text = self.text(name.name).to_owned();
        Some(self.error(p, super::errors::missing_parameter_type(&text, shown.as_deref())))
    }

    /// The expected type dotty's message shows for function `id`, `Some(None)` for none.
    fn missing_param_type_shown(
        &mut self,
        id: NodeId,
        (params, body): (&[NodeId], NodeId),
        pt: Pt,
        ctx: Ctx,
    ) -> Option<Option<String>> {
        if !matches!(self.ast.tree(id), Tree::Function { mods: None, wildcard: false, .. }) {
            return None;
        }
        let synthetic = params.iter().any(
            |&p| matches!(*self.ast.tree(p), Tree::ValDef { name, .. } if self.text(name.name).contains('$')),
        );
        if synthetic || self.may_infer_from_callee(params, body) {
            return None;
        }
        match pt {
            Pt::Any => self.expects_nothing(id, ctx).then_some(None),
            Pt::Type(p) => {
                let t = self.zonk(p);
                (self.is_fully_defined(t) && self.is_sam_free_class(t)).then(|| Some(self.u.show_user(t)))
            }
            Pt::Fun(_) | Pt::PolyFun => None,
        }
    }

    /// Whether `body` applies a callee to arguments among which every parameter is (dotty's
    /// `calleeType` may then give their types).
    fn may_infer_from_callee(&self, params: &[NodeId], body: NodeId) -> bool {
        if self.splices.contains_key(&body) {
            return true;
        }
        let mut b = body;
        while let Tree::Parens { t: inner } | Tree::Annotated { arg: inner, .. } = *self.ast.tree(b) {
            b = inner;
        }
        let Tree::Apply { args, .. } = *self.ast.tree(b) else { return false };
        let args = self.ast.items(args);
        params.iter().all(|&p| {
            let Tree::ValDef { name, .. } = *self.ast.tree(p) else { return true };
            args.iter().any(|&a| {
                let a = match *self.ast.tree(a) {
                    Tree::Typed { expr, .. } => expr,
                    _ => a,
                };
                matches!(*self.ast.tree(a), Tree::Ident { name: n } if n == name)
            })
        })
    }

    /// Whether `t` is a type of a class that is final, sealed or concrete, so neither a function
    /// nor a single abstract method type.
    fn is_sam_free_class(&mut self, t: TypeId) -> bool {
        let mut t = self.u.dealias(t);
        for _ in 0..16 {
            let tycon = match self.u.types().get(t) {
                Type::Applied(tycon, _) => *tycon,
                _ => t,
            };
            let c = match self.u.types().get(tycon).clone() {
                Type::Sym(_, c) => c,
                Type::TypeRef(p, n) => match self.u.resolve(p, &n, true) {
                    Some(c) => c,
                    None => return false,
                },
                _ => return false,
            };
            if self.u.symbol(c).kind == Kind::Type && tycon == t {
                let info = self.u.info(c);
                let Type::Alias(alias) = *self.u.types().get(info) else { return false };
                t = self.u.dealias(alias);
                continue;
            }
            let s = self.u.symbol(c);
            let has = |tag| s.flags.has(tag);
            return s.kind == Kind::Class
                && !has(OBJECT)
                && (has(FINAL) || has(SEALED) || !(has(TRAIT) || has(ABSTRACT)));
        }
        false
    }

    /// Whether function `id`, typed in `ctx` without an expected type, is where dotty expects
    /// none either: reached from the right-hand side of the definition being completed, one that
    /// inherits no type, through blocks, conditionals, cases and function bodies.
    fn expects_nothing(&mut self, id: NodeId, ctx: Ctx) -> bool {
        let mut owner = ctx.owner;
        while self.u.symbol(owner).name == "$anonfun" {
            let Some(o) = self.u.symbol(owner).owner else { return false };
            owner = o;
        }
        let Some(&Completion::Completing(def)) = self.syms.completions.get(&owner) else { return false };
        let (Tree::ValDef { rhs: Some(rhs), .. } | Tree::DefDef { rhs: Some(rhs), .. }) = *self.ast.tree(def)
        else {
            return false;
        };
        self.inherits_nothing(owner) && self.leads_to(rhs, id)
    }

    /// Whether `sym` is no default getter and no base class of its owner declares its name.
    fn inherits_nothing(&mut self, sym: SymbolId) -> bool {
        let name = self.u.symbol(sym).name.clone();
        if name.contains("$default$") {
            return false;
        }
        let Some(cls) = self.u.symbol(sym).owner.filter(|&o| self.u.symbol(o).kind == Kind::Class) else {
            return true;
        };
        let bases: Vec<SymbolId> = self.u.linearization(cls).into_iter().skip(1).collect();
        bases.into_iter().all(|b| self.u.decl(b, &name).is_ok_and(|d| d.is_empty()))
    }

    fn leads_to(&self, from: NodeId, id: NodeId) -> bool {
        let case_leads =
            |c: NodeId| matches!(*self.ast.tree(c), Tree::CaseDef { body, .. } if self.leads_to(body, id));
        from == id
            || match *self.ast.tree(from) {
                Tree::Parens { t } | Tree::Block { expr: Some(t), .. } | Tree::Function { body: t, .. } => {
                    self.leads_to(t, id)
                }
                Tree::If { thenp, elsep: Some(e), .. } => self.leads_to(thenp, id) || self.leads_to(e, id),
                Tree::Match { selector: Some(_), cases, .. } => {
                    self.ast.items(cases).iter().any(|&c| case_leads(c))
                }
                _ => false,
            }
    }
}
