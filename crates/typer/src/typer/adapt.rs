//! Fitting a typed tree to what its context expects (dotty's `adapt`), as far as the subset
//! goes: a tree that fits as it is, a numeric constant converted, a value discarded for `Unit`.

use super::{Pt, Typer};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::Span;
use syntax::ast::constant::Constant;
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// `tree` fitted to `pt`: unchanged if it conforms; a tree that needs more of dotty's `adapt`
    /// than the subset has is outside it.
    pub fn adapt(&mut self, tree: TNodeId, pt: Pt) -> TNodeId {
        let Some(tpe) = self.tpd.get(tree).tpe else { return tree };
        if matches!(self.u.types().get(tpe), Type::Error) || matches!(pt, Pt::Fun(_) | Pt::PolyFun) {
            return tree;
        }
        let span = self.tpd.span(tree);
        if let Type::Method { implicit: true, .. } = self.u.types().get(tpe) {
            self.unsupported_at(span, "implicit arguments");
            return tree;
        }
        let method = self.method_info(tpe).or(Some(tpe)).map(|i| self.u.types().get(i).clone());
        if let Some(Type::Method { params, result, implicit: false, .. }) = &method
            && params.is_empty()
            && self.is_auto_applicable(tree)
        {
            let args = self.tpd.list(&[]);
            let apply = self.tpd.node(T::Apply { fun: tree, args }, span.to_synthetic(), Some(*result), None);
            return self.adapt(apply, pt);
        }
        if matches!(method, Some(Type::Method { .. } | Type::Poly { .. })) {
            self.unsupported_at(span, "method value");
            return tree;
        }
        let Pt::Type(pt) = pt else { return tree };
        if self.conforms(tpe, pt) {
            return tree;
        }
        if let Some(converted) = self.converted_constant(tree, pt) {
            return converted;
        }
        if let Some(widened) = self.numeric_view(tree, pt) {
            return widened;
        }
        if self.u.class_symbol(pt) == Some(self.defn.unit) {
            self.check_unit_purity(tree);
            return self.discarded(tree, span);
        }
        self.unsupported_at(span, "adaptation");
        tree
    }

    /// Whether reference `tree` to a method with an empty parameter list may be called without
    /// one (dotty's auto-application): a member of `Any` other than `getClass`, or of a `java.*`
    /// or `javax.*` class.
    fn is_auto_applicable(&mut self, tree: TNodeId) -> bool {
        let Some(s) = self.tpd.sym(tree) else { return false };
        let Some(owner) = self.u.symbol(s).owner else { return false };
        if owner == self.u.builtins().any {
            return self.u.symbol(s).name != "getClass";
        }
        let name = self.u.full_name(owner);
        name.starts_with("java.") || name.starts_with("javax.")
    }

    /// `{ tree; () }` (dotty's `Block(tree :: Nil, unitLiteral)`).
    fn discarded(&mut self, tree: TNodeId, span: Span) -> TNodeId {
        let unit = crate::defn::type_ref(self.u, self.defn.unit);
        let literal = self.tpd.node(T::Literal { value: Constant::Unit }, span.end_pos(), Some(unit), None);
        let stats = self.tpd.list(&[tree]);
        self.tpd.node(T::Block { stats, expr: literal }, span.to_synthetic(), Some(unit), None)
    }

    /// The constant of `tree` converted to the primitive class of `pt` (dotty's
    /// `Constant.convertTo`, `adaptConstant`).
    fn converted_constant(&mut self, tree: TNodeId, pt: TypeId) -> Option<TNodeId> {
        let value = self.constant_of(tree)?;
        let target = self.u.class_symbol(pt).map(|c| self.u.full_name(c))?;
        let (converted, lossy) = match (&value, target.as_str()) {
            (Constant::Char(c), "scala.Int") => (Constant::Int(*c as i32), false),
            (Constant::Int(i), "scala.Char") if (0..=0xFFFF).contains(i) => {
                (Constant::Char(*i as u16), false)
            }
            (Constant::Int(i), "scala.Long") => (Constant::Long(*i as i64), false),
            (Constant::Char(c), "scala.Long") => (Constant::Long(*c as i64), false),
            (Constant::Int(i), "scala.Float") => (Constant::Float(*i as f32), (*i as f32) as i32 != *i),
            (Constant::Long(l), "scala.Float") => (Constant::Float(*l as f32), (*l as f32) as i64 != *l),
            (Constant::Char(c), "scala.Float") => (Constant::Float(*c as f32), false),
            (Constant::Int(i), "scala.Double") => (Constant::Double(*i as f64), false),
            (Constant::Long(l), "scala.Double") => (Constant::Double(*l as f64), (*l as f64) as i64 != *l),
            (Constant::Char(c), "scala.Double") => (Constant::Double(*c as f64), false),
            (Constant::Float(f), "scala.Double") => (Constant::Double(*f as f64), false),
            _ => return None,
        };
        let span = self.tpd.span(tree);
        if lossy {
            let short = target.rsplit('.').next().unwrap_or(&target);
            let msg = format!(
                "Widening conversion from {} to {short} loses precision.\nWrite `.to{short}` instead.",
                value.tag()
            );
            self.diags.warning(span.point(), msg);
        }
        Some(self.with_constant(tree, converted))
    }

    /// `tree` evaluated for constant `value`: the literal alone if `tree` is pure.
    fn with_constant(&mut self, tree: TNodeId, value: Constant) -> TNodeId {
        let span = self.tpd.span(tree);
        let tpe = self.literal_type(&value);
        if let T::Block { stats, expr } = *self.tpd.tree(tree) {
            let expr = self.with_constant(expr, value);
            return self.tpd.node(T::Block { stats, expr }, span, Some(tpe), None);
        }
        let pure = matches!(self.tpd.tree(tree), T::Literal { .. }) || self.is_idempotent_expr(tree);
        let literal = self.tpd.node(T::Literal { value }, span, Some(tpe), None);
        if pure {
            return literal;
        }
        let stats = self.tpd.list(&[tree]);
        self.tpd.node(T::Block { stats, expr: literal }, span.to_synthetic(), Some(tpe), None)
    }
}
