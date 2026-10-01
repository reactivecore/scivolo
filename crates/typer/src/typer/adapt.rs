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
        if matches!(self.u.types().get(tpe), Type::Error) || matches!(pt, Pt::Args(_)) {
            return tree;
        }
        let span = self.tpd.span(tree);
        let method = self.method_info(tpe).map(|i| self.u.types().get(i).clone());
        if matches!(method, Some(Type::Method { .. } | Type::Poly { .. })) {
            self.unsupported_at(span, "method value");
            return tree;
        }
        let Pt::Type(pt) = pt else { return tree };
        if self.u.is_subtype(tpe, pt) {
            return tree;
        }
        if let Some(converted) = self.converted_constant(tree, pt) {
            return converted;
        }
        if self.u.class_symbol(pt) == Some(self.defn.unit) {
            self.check_unit_purity(tree);
            return self.discarded(tree, span);
        }
        self.unsupported_at(span, "adaptation");
        tree
    }

    /// `{ tree; () }` (dotty's `Block(tree :: Nil, unitLiteral)`).
    fn discarded(&mut self, tree: TNodeId, span: Span) -> TNodeId {
        let unit = crate::defn::type_ref(self.u, self.defn.unit);
        let literal = self.tpd.node(T::Literal { value: Constant::Unit }, span.end_pos(), Some(unit), None);
        let stats = self.tpd.list(&[tree]);
        self.tpd.node(T::Block { stats, expr: literal }, span.to_synthetic(), Some(unit), None)
    }

    /// Literal `tree` converted to the primitive class of `pt` (dotty's `Constant.convertTo`).
    fn converted_constant(&mut self, tree: TNodeId, pt: TypeId) -> Option<TNodeId> {
        let T::Literal { ref value } = *self.tpd.tree(tree) else { return None };
        let value = value.clone();
        let target = self.u.class_symbol(pt).map(|c| self.u.full_name(c))?;
        let (converted, lossy) = match (&value, target.as_str()) {
            (Constant::Char(c), "scala.Int") => (Constant::Int(*c as i32), false),
            (Constant::Int(i), "scala.Char") if (0..=0xFFFF).contains(i) => (Constant::Char(*i as u16), false),
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
        let tpe = self.literal_type(&converted);
        Some(self.tpd.node(T::Literal { value: converted }, span, Some(tpe), None))
    }
}
