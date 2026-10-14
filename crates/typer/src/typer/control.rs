//! Control flow: `if`, `while`, assignments, `return`, `throw` (dotty's `typedIf`,
//! `typedWhileDo`, `typedAssign`, `typedReturn`, `typedThrow`).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::defn::type_ref;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::constant::Constant;
use syntax::ast::{NodeId, Span, TName};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    pub fn boolean(&mut self) -> TypeId {
        let boolean = self.resolve_full_name("scala.Boolean", true).expect("scala.Boolean");
        type_ref(self.u, boolean)
    }

    fn unit_type(&mut self) -> TypeId {
        type_ref(self.u, self.defn.unit)
    }

    pub fn typed_if(
        &mut self,
        id: NodeId,
        (cond, thenp, elsep, inline): (NodeId, NodeId, Option<NodeId>, bool),
        pt: Pt,
        ctx: Ctx,
    ) -> TNodeId {
        let span = self.ast.span(id);
        if inline {
            self.check_in_inline_context("inline if", span, ctx);
        }
        let boolean = self.boolean();
        let cond = self.typed(cond, Pt::Type(boolean), ctx);
        let Some(elsep) = elsep else {
            let unit = self.unit_type();
            let thenp = self.typed(thenp, Pt::Type(unit), ctx);
            let elsep = self.build().literal(Constant::Unit, span.end_pos());
            return self.tpd.node(T::If { cond, thenp, elsep, inline }, span, Some(unit), None);
        };
        let branch_pt = match pt {
            _ if self.is_incomplete(elsep) => Pt::Type(self.unit_type()),
            Pt::Fun(_) | Pt::PolyFun => Pt::Any,
            pt => pt,
        };
        let thenp = self.typed(thenp, branch_pt, ctx);
        let elsep = self.typed(elsep, branch_pt, ctx);
        let (a, b) = (self.tpd.tpe(thenp), self.tpd.tpe(elsep));
        if matches!(pt, Pt::Any) && self.needs_harmonizing(a, b) {
            return self.unsupported(id, "numeric harmonization");
        }
        let tpe = self.u.lub(a, b);
        self.tpd.node(T::If { cond, thenp, elsep, inline }, span, Some(tpe), None)
    }

    /// Reports `what` at `span` unless `ctx` is in an inline method (dotty's
    /// `checkInInlineContext`).
    pub fn check_in_inline_context(&mut self, what: &str, span: Span, ctx: Ctx) {
        if self.enclosing_inline(ctx.owner).is_none() {
            self.error_at(span, format!("{what} can only be used in an inline method"));
        }
    }

    /// Whether else-branch `elsep` ends in an `if` without `else`.
    fn is_incomplete(&self, elsep: NodeId) -> bool {
        match *self.ast.tree(elsep) {
            syntax::ast::Tree::If { elsep: None, .. } => true,
            syntax::ast::Tree::If { elsep: Some(e), .. } => self.is_incomplete(e),
            _ => false,
        }
    }

    pub fn typed_while(&mut self, id: NodeId, cond: NodeId, body: NodeId, ctx: Ctx) -> TNodeId {
        let boolean = self.boolean();
        let cond = self.typed(cond, Pt::Type(boolean), ctx);
        let unit = self.unit_type();
        let body = self.typed(body, Pt::Type(unit), ctx);
        let span = self.ast.span(id);
        self.tpd.node(T::WhileDo { cond, body }, span, Some(unit), None)
    }

    /// `x = v` for a variable `x` (dotty's `typedAssign`, without setters and `update`).
    pub fn typed_assign(&mut self, id: NodeId, lhs: NodeId, rhs: NodeId, ctx: Ctx) -> TNodeId {
        if let syntax::ast::Tree::Apply { fun, args } = *self.ast.tree(lhs) {
            let name = TName { name: self.name("update"), is_type: false };
            let select = self.ast.node(syntax::ast::Tree::Select { qualifier: fun, name });
            let fun_span = self.ast.span(fun);
            self.ast.with_span(select, Span::synthetic(fun_span.start(), fun_span.end()));
            let mut items = self.ast.items(args).to_vec();
            items.push(rhs);
            let args = self.ast.list(&items);
            let update = self.ast.node(syntax::ast::Tree::Apply { fun: select, args });
            let span = Span::synthetic(self.ast.span(lhs).start(), self.ast.span(rhs).end());
            self.ast.with_span(update, span);
            return self.typed_apply(update, select, args, Pt::Any, ctx);
        }
        let lhs = self.typed(lhs, Pt::Any, ctx);
        let var = self.tpd.sym(lhs).filter(|&s| {
            let s = self.u.symbol(s);
            s.kind == Kind::Val && s.flags.has(tasty::tags::MUTABLE)
        });
        let Some(var) = var else { return self.unsupported(id, "assignment to a non-variable") };
        self.complete(var);
        let info = self.u.info(var);
        let rhs = self.typed(rhs, Pt::Type(info), ctx);
        let unit = self.unit_type();
        let span = self.ast.span(id);
        self.tpd.node(T::Assign { lhs, rhs }, span, Some(unit), None)
    }

    /// `return e` from the enclosing method, whose result type is declared.
    pub fn typed_return(&mut self, id: NodeId, expr: Option<NodeId>, ctx: Ctx) -> TNodeId {
        let Some(method) = self.enclosing_method(ctx.owner) else {
            return self.unsupported(id, "return outside a method");
        };
        if self.u.symbol(method).flags.has(tasty::tags::INLINE) {
            return self.unsupported(id, "return from an inline method");
        }
        let Some(&result) = self.syms.def_results.get(&method).filter(|_| self.has_declared_result(method))
        else {
            return self.unsupported(id, "return from a method without a result type");
        };
        let span = self.ast.span(id);
        let expr = match expr {
            Some(e) => self.typed(e, Pt::Type(result), ctx),
            None => {
                let unit = self.build().literal(Constant::Unit, span);
                self.adapt(unit, Pt::Type(result), ctx)
            }
        };
        self.build().return_(expr, method, span)
    }

    /// The method `owner` is in, not a constructor or a lambda (dotty's `isRealMethod`).
    fn enclosing_method(&self, owner: SymbolId) -> Option<SymbolId> {
        let mut o = Some(owner);
        while let Some(s) = o {
            let sym = self.u.symbol(s);
            match sym.kind {
                Kind::Def if sym.name != "<init>" && sym.name != "$anonfun" => return Some(s),
                Kind::Class | Kind::Package => return None,
                _ => o = sym.owner,
            }
        }
        None
    }

    fn has_declared_result(&self, method: SymbolId) -> bool {
        self.syms.declared_results.contains(&method)
    }

    /// dotty's `<special-ops>.throw(x: Throwable): Nothing`, defined on first use.
    fn throw_method(&mut self) -> SymbolId {
        let throw = self.syms.throw_method.unwrap_or_else(|| crate::defn::throw_method(self.u));
        self.syms.throw_method = Some(throw);
        throw
    }

    /// `throw e`: a call of dotty's `<special-ops>.throw`.
    pub fn typed_throw(&mut self, id: NodeId, expr: NodeId, ctx: Ctx) -> TNodeId {
        let throwable = match self.defn.throwable(self.u) {
            Ok(t) => type_ref(self.u, t),
            Err(e) => return self.unsupported(id, &e),
        };
        let expr = self.typed(expr, Pt::Type(throwable), ctx);
        let throw = self.throw_method();
        let span = self.ast.span(id);
        let fun_tpe = self.u.intern_type(Type::Sym(None, throw));
        let at = self.tpd.span(expr).start_pos();
        let fun = self.build().ident(throw, fun_tpe, at);
        let nothing = type_ref(self.u, self.u.builtins().nothing);
        self.build().apply(fun, &[expr], nothing, span)
    }
}
