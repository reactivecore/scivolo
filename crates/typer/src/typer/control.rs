// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Control flow: `if`, `while`, `return` (dotty's `typedIf`, `typedWhileDo`, `typedReturn`).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::defn::type_ref;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::constant::Constant;
use syntax::ast::{NodeId, Span};
use tasty::{Kind, SymbolId, TypeId};

impl Typer<'_> {
    pub fn boolean(&mut self) -> TypeId {
        let boolean = self.resolve_full_name("scala.Boolean", true).expect("scala.Boolean");
        type_ref(self.u, boolean)
    }

    pub fn unit_type(&mut self) -> TypeId {
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
            Pt::Fun(_) | Pt::PolyFun(_) => Pt::Any,
            pt => pt,
        };
        let before = self.before_harmonic();
        let branches = vec![self.typed(thenp, branch_pt, ctx), self.typed(elsep, branch_pt, ctx)];
        let (thenp, elsep) = match self.harmonic(branches, branch_pt, Some(before), ctx) {
            Ok(h) => (h[0], h[1]),
            Err(what) => return self.unsupported(id, what),
        };
        let (a, b) = (self.tpd.tpe(thenp), self.tpd.tpe(elsep));
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
}
