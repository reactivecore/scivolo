//! Reflective calls through structural types (nscplugin's `genReflectiveCall`).

use super::Gen;
use super::apply::Receiver;
use crate::{Result, unsupported};
use nir::sig::Sig;
use nir::{Op, Type, TypeId, ValId};
use syntax::ast::Span;
use syntax::ast::constant::Constant;
use tasty::SymbolId;
use typer::tpd::{TNodeId, Tree};

impl Gen<'_, '_> {
    /// Whether `sym` is method `name` of `scala.reflect.Selectable`.
    pub fn is_reflective(&self, sym: SymbolId, name: &str) -> bool {
        let s = self.l.u.symbol(sym);
        s.name == name && s.owner.is_some_and(|o| self.l.u.full_name(o) == "scala.reflect.Selectable")
    }

    /// `recv.selectDynamic("name")` of `sym`: the proxy of member `name` of `recv.selectedValue`,
    /// called with no arguments.
    pub fn gen_reflective_select(
        &mut self,
        sym: SymbolId,
        recv: Receiver,
        args: &[TNodeId],
        span: Span,
    ) -> Result<ValId> {
        let (Receiver::Tree(recv), &[name]) = (recv, args) else {
            return unsupported(span, "reflective calls of other shapes");
        };
        self.reflective_call(sym, recv, name, Vec::new(), Vec::new(), span)
    }

    /// `recv.applyDynamic("name", classOf[P1], …)(args)` of `sym`, its argument lists flattened:
    /// the proxy of member `name` with parameter types `P1, …` of `recv.selectedValue`, called
    /// with `args`.
    pub fn gen_reflective_apply(
        &mut self,
        sym: SymbolId,
        recv: Receiver,
        args: &[TNodeId],
        span: Span,
    ) -> Result<ValId> {
        let (Receiver::Tree(recv), &[name, class_ofs, actuals]) = (recv, args) else {
            return unsupported(span, "reflective calls of other shapes");
        };
        let (Some(class_ofs), Some(actuals)) = (self.seq_elems(class_ofs), self.seq_elems(actuals)) else {
            return unsupported(span, "reflective calls with arguments that are not sequence literals");
        };
        let mut formals = Vec::new();
        for c in class_ofs {
            let Tree::ClassLiteral { tpe } = *self.l.tpd.tree(self.l.tpd.strip_typed(c)) else {
                return unsupported(
                    span,
                    "reflective calls with parameter types that are not class literals",
                );
            };
            formals.push(self.gen_type(tpe));
        }
        self.reflective_call(sym, recv, name, formals, actuals, span)
    }

    /// The elements of `wrapXArray(seq)` for a sequence literal `seq`.
    fn seq_elems(&self, t: TNodeId) -> Option<Vec<TNodeId>> {
        let Tree::Apply { fun, args } = *self.l.tpd.tree(self.l.tpd.strip_typed(t)) else { return None };
        let fun = match *self.l.tpd.tree(fun) {
            Tree::TypeApply { fun, .. } => fun,
            _ => fun,
        };
        let wrap = self.l.tpd.sym(fun).is_some_and(|s| {
            let name = &self.l.u.symbol(s).name;
            name.starts_with("wrap") && name.ends_with("Array") || name == "genericWrapArray"
        });
        let (true, &[seq]) = (wrap, self.l.tpd.items(args)) else { return None };
        let Tree::SeqLiteral { elems, .. } = *self.l.tpd.tree(self.l.tpd.strip_typed(seq)) else {
            return None;
        };
        Some(self.l.tpd.items(elems).to_vec())
    }

    fn reflective_call(
        &mut self,
        sym: SymbolId,
        recv: TNodeId,
        name: TNodeId,
        formals: Vec<TypeId>,
        actuals: Vec<TNodeId>,
        span: Span,
    ) -> Result<ValId> {
        let Tree::Literal { value: Constant::String(units) } = self.l.tpd.tree(name) else {
            return unsupported(span, "reflective calls of a name that is not a literal");
        };
        let name = String::from_utf16_lossy(units);
        if formals.len() != actuals.len() {
            return unsupported(span, "reflective calls with as many parameter types as arguments");
        }
        let owner = self.l.u.symbol(sym).owner.expect("a method's owner");
        let selected = self.l.u.decl(owner, "selectedValue").ok().and_then(|d| d.first().copied());
        let Some(selected) = selected else { return unsupported(span, "Selectable.selectedValue") };
        let receiver = self.gen_expr(recv)?;
        let obj = self.call_method(selected, false, receiver, Vec::new());
        let object = self.pool.top("java.lang.Object");
        let object = self.ty(Type::Ref { name: object, exact: false, nullable: true });
        let mut params = vec![object];
        let mut args = vec![obj];
        for (&a, &f) in actuals.iter().zip(&formals) {
            let v = self.gen_expr(a)?;
            let boxed = self.boxed_class(f);
            let from = self.val_ty(v);
            let same_class = match (self.pool.ty(from), self.pool.ty(f)) {
                (Type::Ref { name: a, .. }, Type::Ref { name: b, .. }) => a == b,
                _ => true,
            };
            args.push(if same_class { v } else { self.emit(Op::As { ty: f, obj: v }) });
            params.push(boxed.unwrap_or(f));
        }
        let sig = self.pool.sig(&Sig::Proxy(name, formals));
        let ptr = self.emit(Op::Dynmethod { obj, sig });
        let ty = self.ty(Type::Function(params, object));
        Ok(self.emit(Op::Call { ty, ptr, args }))
    }
}
