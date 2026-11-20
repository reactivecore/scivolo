// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeApplications.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Sequence arguments `xs*` passed to repeated parameters (dotty's `typedWildcardStarArgExpr`,
//! `translateParameterized`).

use super::{Pt, Typer};
use crate::ctx::{Ctx, Mode};
use crate::defn::{ARRAY, IMMUTABLE_SEQ};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, Tree};
use syntax::config::SourceVersion;
use tasty::{NOTHING, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// `sequence_arg` of `a`, or `pre` if overload resolution typed it with elements of `elem`.
    pub fn pretyped_sequence_arg(
        &mut self,
        a: NodeId,
        pre: Option<TNodeId>,
        elem: TypeId,
        ctx: Ctx,
    ) -> TNodeId {
        let Some(pre) = pre else { return self.sequence_arg(a, Some(elem), ctx) };
        match self.repeated_elem(self.tpd.tpe(pre)) {
            Some(e) if self.conforms(e, elem) => pre,
            _ => self.unsupported(a, "sequence argument of an overload"),
        }
    }

    /// Sequence argument `a`, `xs*`, passed to a repeated parameter of element type `elem`:
    /// `xs` typed as a `Seq[elem] | Array[? <: elem]` (a `Seq[elem]` in a quote pattern), without
    /// an expected type if no `elem`, ascribed `<repeated>[T]` with `T` its element type, as
    /// `<repeated>[Int | String]` for `xs` of `List[Int] | List[String]`; `null*` ascribed its own
    /// type `Null`; refuses a union or intersection of arrays, as `Array[Int] | Array[String]`.
    pub fn sequence_arg(&mut self, a: NodeId, elem: Option<TypeId>, ctx: Ctx) -> TNodeId {
        let (Some(seq_class), Some(array_class), &Tree::Typed { expr, tpt }) = (
            self.resolve_full_name(IMMUTABLE_SEQ, true),
            self.resolve_full_name(ARRAY, true),
            self.ast.tree(a),
        ) else {
            return self.unsupported(a, "sequence argument");
        };
        let pt = elem.map_or(Pt::Any, |elem| {
            let seq = crate::defn::type_ref(self.u, seq_class);
            let seq = self.u.intern_applied(seq, &[elem]);
            if ctx.mode.is(Mode::QUOTED_PATTERN) {
                return Pt::Type(seq);
            }
            let array = crate::defn::type_ref(self.u, array_class);
            let below = self.u.intern_type(Type::Bounds(NOTHING, elem));
            let array = self.u.intern_applied(array, &[below]);
            Pt::Type(self.u.union(seq, array, false))
        });
        let expr = self.typed(expr, pt, ctx);
        let expr_tpe = self.tpd.tpe(expr);
        let b = self.u.defn();
        let widened = self.u.widen(expr_tpe);
        if self.u.class_symbol(widened).is_some_and(|c| c == b.null || c == b.nothing) {
            let tpt = self.type_tree(widened, self.ast.span(tpt), false);
            return self.tpd.node(T::Typed { expr, tpt }, self.ast.span(a), Some(widened), None);
        }
        let from = if self.u.base_type(widened, array_class).is_some() { array_class } else { seq_class };
        if from == array_class && self.is_compound_sequence(widened) {
            return self.unsupported(a, "sequence argument of a union or intersection of arrays");
        }
        let Some(arg) = self.sequence_elem(widened, from) else {
            return self.unsupported(a, "sequence argument");
        };
        let repeated = self.repeated_type(arg);
        let tpt = self.type_tree(repeated, self.ast.span(tpt), false);
        let simplified = self.simplified(repeated);
        self.tpd.node(T::Typed { expr, tpt }, self.ast.span(a), Some(simplified), None)
    }

    /// Checks whether `tpe`, its sides of a bottom type dropped, is a union or intersection, as
    /// `Array[Int] | Array[String]`.
    fn is_compound_sequence(&mut self, tpe: TypeId) -> bool {
        let tpe = self.u.widen(tpe);
        let tpe = self.u.dealias(tpe);
        let ty = *self.u.ty(tpe);
        match ty {
            Type::Or(a, b, _) if self.is_bottom_type(a) => self.is_compound_sequence(b),
            Type::Or(a, b, _) if self.is_bottom_type(b) => self.is_compound_sequence(a),
            Type::Or(..) | Type::And(..) => true,
            _ => false,
        }
    }

    /// Finds the element type of sequence `tpe`, an instance of class `from`: that of each side
    /// of a union or intersection, a side of a bottom type dropped, an argument's upper bound
    /// (dotty's `elemType` in `translateParameterized`); `None` if `tpe` is no instance of `from`.
    fn sequence_elem(&mut self, tpe: TypeId, from: SymbolId) -> Option<TypeId> {
        let tpe = self.u.widen(tpe);
        let tpe = self.u.dealias(tpe);
        let ty = *self.u.ty(tpe);
        match ty {
            Type::Or(a, b, _) if self.is_bottom_type(a) => self.sequence_elem(b, from),
            Type::Or(a, b, _) if self.is_bottom_type(b) => self.sequence_elem(a, from),
            Type::Or(a, b, soft) => {
                let (x, y) = (self.sequence_elem(a, from)?, self.sequence_elem(b, from)?);
                Some(if x == y { x } else { self.u.union(x, y, soft) })
            }
            Type::And(a, b) => {
                let (x, y) = (self.sequence_elem(a, from)?, self.sequence_elem(b, from)?);
                match self.source_version.is_at_least(SourceVersion::FUTURE_VERSION) {
                    _ if x == y => Some(x),
                    true => Some(self.u.union(x, y, false)),
                    false => Some(self.u.intern_type(Type::And(x, y))),
                }
            }
            _ => {
                let base = self.u.base_type(tpe, from)?;
                let (_, args) = self.u.applied(base)?;
                Some(self.u.bounds(args[0]).map_or(args[0], |(_, hi)| hi))
            }
        }
    }
}
