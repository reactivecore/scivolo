// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Numeric harmonization: the `Int` constants among numeric values converted to the one other
//! numeric class of the rest (dotty's `harmonize`, `harmonizeWith` and `harmonic`).

use super::{Pt, Typer};
use crate::constraint::attempt::Attempt;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::constant::Constant as C;
use tasty::{Type, TypeId, TypeVars};

/// The constraint before the trees to harmonize were typed, and its number of variables.
pub type Before = (Attempt, usize);

impl Typer<'_> {
    pub fn before_harmonic(&self) -> Before {
        (self.constraint.attempt(), self.constraint.count())
    }

    /// `trees`, typed against `pt` since `before`, harmonized unless `pt` is fully defined; then
    /// the constraint is reset to `before` and the trees adapted to `pt` again (dotty's
    /// `harmonic` with `harmonize`); an error if the constraint cannot be reset.
    pub fn harmonic(
        &mut self,
        trees: Vec<TNodeId>,
        pt: Pt,
        before: Option<Before>,
        ctx: Ctx,
    ) -> Result<Vec<TNodeId>, &'static str> {
        let open = match pt {
            Pt::Any => true,
            Pt::Type(t) => self.is_open(t),
            _ => false,
        };
        let Some(harmonized) = open.then(|| self.harmonized(&trees, ctx)).flatten() else {
            return Ok(trees);
        };
        let Pt::Type(pt) = pt else { return Ok(harmonized) };
        let Some((attempt, _)) = before.filter(|&(_, vars)| vars == self.constraint.count()) else {
            return Err("numeric harmonization of values with type variables");
        };
        self.constraint.abandon(attempt);
        Ok(harmonized.into_iter().map(|t| self.adapt_deep(t, pt, ctx)).collect())
    }

    /// Whether `t` has a type variable with different bounds (dotty's `isFullyDefined` with
    /// `ForceDegree.none` negated, a variable of equal bounds being instantiated).
    pub fn is_open(&mut self, t: TypeId) -> bool {
        let t = self.zonk(t);
        let mut vars = Vec::new();
        self.u.map_type(t, &mut |u, part| {
            if let Type::Var(v) = *u.types().get(part) {
                vars.push(v);
            }
            None
        });
        vars.into_iter().any(|v| {
            let (lo, hi) = self.constraint.bounds(v);
            self.zonk(lo) != self.zonk(hi)
        })
    }

    /// `trees` with their `Int` constants converted to the class of the others, if all are
    /// numeric, of one class but `Int`, and each constant converts exactly (dotty's `harmonize`).
    fn harmonized(&mut self, trees: &[TNodeId], ctx: Ctx) -> Option<Vec<TNodeId>> {
        let ints: Vec<Option<i32>> = trees.iter().map(|&t| self.int_constant(t)).collect();
        let class = self.target_class(trees, &ints)?;
        let lossy = |i: i32| class == "scala.Float" && i as f32 as i32 != i;
        let exact = |i: i32| super::constants::convert_to(&C::Int(i), &class).is_some() && !lossy(i);
        if !ints.iter().all(|i| i.is_none_or(exact)) {
            return None;
        }
        let target = self.resolve_full_name(&class, true)?;
        let target = crate::defn::type_ref(self.u, target);
        let adapted = trees.iter().zip(ints).map(|(&tree, i)| match i {
            Some(_) => self.adapt_deep(tree, target, ctx),
            None => tree,
        });
        Some(adapted.collect())
    }

    /// The numeric class the `Int` constants among `trees` convert to (dotty's `targetClass`).
    fn target_class(&mut self, trees: &[TNodeId], ints: &[Option<i32>]) -> Option<String> {
        let (mut class, mut int_seen) = (None, false);
        for (&tree, int) in trees.iter().zip(ints) {
            if int.is_some() {
                int_seen = true;
                continue;
            }
            let t = self.harmonized_type(tree);
            let c = self.numeric_class(t);
            if c.is_none() || class.is_some() && class != c {
                return None;
            }
            class = c;
        }
        class.filter(|c| int_seen && c != "scala.Int")
    }

    /// The numeric value class of `t`, of both sides of a union (dotty's `classSymbol`).
    fn numeric_class(&mut self, t: TypeId) -> Option<String> {
        if let Type::Or(a, b, _) = *self.u.types().get(t) {
            let a = self.numeric_class(a)?;
            return (self.numeric_class(b)? == a).then_some(a);
        }
        self.value_class(t).filter(|c| super::widening::is_numeric(c))
    }

    /// The value of `tree` if its type is an `Int` constant type, also through a reference to a
    /// constant; a case by its body.
    fn int_constant(&mut self, tree: TNodeId) -> Option<i32> {
        let constant = match *self.tpd.tree(tree) {
            T::CaseDef { body, .. } => return self.int_constant(body),
            T::Typed { .. } => self.constant_of_type(self.tpd.tpe(tree)),
            _ => self.constant_of(tree),
        };
        match constant? {
            C::Int(i) => Some(i),
            _ => None,
        }
    }

    fn harmonized_type(&mut self, tree: TNodeId) -> TypeId {
        match *self.tpd.tree(tree) {
            T::CaseDef { body, .. } => self.tpd.tpe(body),
            _ => self.tpd.tpe(tree),
        }
    }

    /// `tree` adapted to `pt`, a case by its body (dotty's `adaptDeep`).
    fn adapt_deep(&mut self, tree: TNodeId, pt: TypeId, ctx: Ctx) -> TNodeId {
        let T::CaseDef { pat, guard, body } = *self.tpd.tree(tree) else {
            return self.adapt(tree, Pt::Type(pt), ctx);
        };
        let body = self.adapt(body, Pt::Type(pt), ctx);
        let span = self.tpd.span(tree);
        self.tpd.node(T::CaseDef { pat, guard, body }, span, None, None)
    }
}
