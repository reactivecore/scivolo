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
    fn boolean(&mut self) -> TypeId {
        let boolean = self.resolve_full_name("scala.Boolean", true).expect("scala.Boolean");
        type_ref(self.u, boolean)
    }

    fn unit_type(&mut self) -> TypeId {
        type_ref(self.u, self.defn.unit)
    }

    fn unit_literal(&mut self, span: Span) -> TNodeId {
        let unit = self.unit_type();
        self.tpd.node(T::Literal { value: Constant::Unit }, span, Some(unit), None)
    }

    pub fn typed_if(
        &mut self,
        id: NodeId,
        cond: NodeId,
        thenp: NodeId,
        elsep: Option<NodeId>,
        pt: Pt,
        ctx: Ctx,
    ) -> TNodeId {
        let boolean = self.boolean();
        let cond = self.typed(cond, Pt::Type(boolean), ctx);
        let span = self.ast.span(id);
        let Some(elsep) = elsep else {
            let unit = self.unit_type();
            let thenp = self.typed(thenp, Pt::Type(unit), ctx);
            let elsep = self.unit_literal(span.end_pos());
            return self.tpd.node(T::If { cond, thenp, elsep }, span, Some(unit), None);
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
        self.tpd.node(T::If { cond, thenp, elsep }, span, Some(tpe), None)
    }

    /// Whether else-branch `elsep` ends in an `if` without `else`.
    fn is_incomplete(&self, elsep: NodeId) -> bool {
        match *self.ast.tree(elsep) {
            syntax::ast::Tree::If { elsep: None, .. } => true,
            syntax::ast::Tree::If { elsep: Some(e), .. } => self.is_incomplete(e),
            _ => false,
        }
    }

    /// Whether types `a` and `b` are different numeric classes, which dotty's `harmonize` widens.
    fn needs_harmonizing(&mut self, a: TypeId, b: TypeId) -> bool {
        let (ca, cb) = (self.value_class(a), self.value_class(b));
        let numeric = |c: &Option<String>| c.as_deref().is_some_and(super::widening::is_numeric);
        numeric(&ca) && numeric(&cb) && ca != cb
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
        let Some(&result) = self.syms.def_results.get(&method).filter(|_| self.has_declared_result(method))
        else {
            return self.unsupported(id, "return from a method without a result type");
        };
        let span = self.ast.span(id);
        let expr = match expr {
            Some(e) => self.typed(e, Pt::Type(result), ctx),
            None => {
                let unit = self.unit_literal(span);
                self.adapt(unit, Pt::Type(result), ctx)
            }
        };
        let name = TName { name: self.name(&self.u.symbol(method).name.clone()), is_type: false };
        let from_tpe = self.u.intern_type(Type::Sym(None, method));
        let from = self.tpd.node(T::Ident { name }, span.end_pos(), Some(from_tpe), Some(method));
        let nothing = type_ref(self.u, self.u.builtins().nothing);
        self.tpd.node(T::Return { expr, from }, span, Some(nothing), None)
    }

    /// The method `owner` is in, not a constructor.
    fn enclosing_method(&self, owner: SymbolId) -> Option<SymbolId> {
        let mut o = Some(owner);
        while let Some(s) = o {
            let sym = self.u.symbol(s);
            match sym.kind {
                Kind::Def if sym.name != "<init>" => return Some(s),
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
        if let Some(t) = self.syms.throw_method {
            return t;
        }
        let ops = self.u.enter_package("<special-ops>");
        let throw = self.syms.define(self.u, "throw", Kind::Def, ops, syntax::ast::flags::METHOD);
        let throwable = type_ref(self.u, self.defn.throwable);
        let nothing = type_ref(self.u, self.u.builtins().nothing);
        let binder = self.syms.fresh_binder();
        let info = self.u.intern_type(Type::Method {
            binder,
            params: vec![("x".into(), throwable)],
            result: nothing,
            implicit: false,
            contextual: false,
        });
        self.u.set_info(throw, info);
        self.syms.throw_method = Some(throw);
        throw
    }

    /// `throw e`: a call of dotty's `<special-ops>.throw`.
    pub fn typed_throw(&mut self, id: NodeId, expr: NodeId, ctx: Ctx) -> TNodeId {
        let throwable = type_ref(self.u, self.defn.throwable);
        let expr = self.typed(expr, Pt::Type(throwable), ctx);
        let throw = self.throw_method();
        let span = self.ast.span(id);
        let fun_tpe = self.u.intern_type(Type::Sym(None, throw));
        let name = TName { name: self.name("throw"), is_type: false };
        let fun =
            self.tpd.node(T::Ident { name }, self.tpd.span(expr).start_pos(), Some(fun_tpe), Some(throw));
        let args = self.tpd.list(&[expr]);
        let nothing = type_ref(self.u, self.u.builtins().nothing);
        self.tpd.node(T::Apply { fun, args }, span, Some(nothing), None)
    }
}
