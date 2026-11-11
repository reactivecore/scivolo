// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Literals and their types (dotty's `typedLiteral`).

use super::implicits::is_misfit;
use super::{Pt, Typer};
use crate::ctx::{Ctx, Mode};
use crate::tpd::TNodeId;
use syntax::ast::constant::Constant;
use syntax::ast::from_digits;
use syntax::ast::tree::NumberKind;
use syntax::ast::{NodeId, TName, Tree};
use syntax::config::Features;
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// A number literal: of the primitive type expected if there is one, else an `Int` or a
    /// `Double` by its form (dotty's `typedNumber`).
    pub fn typed_number(
        &mut self,
        id: NodeId,
        (digits, kind): (&str, NumberKind),
        pt: Pt,
        ctx: Ctx,
    ) -> TNodeId {
        let target = match pt {
            Pt::Type(t) => self.u.class_symbol(t).map(|c| self.u.full_name(c)),
            _ => None,
        };
        let primitive = matches!(
            target.as_deref(),
            Some(
                "scala.Int"
                    | "scala.Char"
                    | "scala.Byte"
                    | "scala.Short"
                    | "scala.Long"
                    | "scala.Float"
                    | "scala.Double"
            )
        );
        let generic = Features::GENERIC_NUMBER_LITERALS;
        if let Pt::Type(t) = pt
            && !primitive
            && self.feature_enabled(generic, "experimental.genericNumberLiterals", ctx)
        {
            match self.generic_number(id, (digits, kind), t, ctx) {
                Ok(Some(tree)) => return tree,
                Ok(None) => {}
                Err(what) => return self.unsupported(id, what),
            }
        }
        let whole = |r| from_digits::int(digits, r as u32).map(Constant::Int);
        let value = match (target.as_deref(), kind) {
            (Some("scala.Int" | "scala.Char" | "scala.Byte" | "scala.Short"), NumberKind::Whole(r)) => {
                whole(r)
            }
            (Some("scala.Long"), NumberKind::Whole(r)) => {
                from_digits::long(digits, r as u32).map(Constant::Long)
            }
            (Some("scala.Float"), k) if k != NumberKind::Whole(16) => {
                let float = from_digits::float(digits);
                if let (Ok(f), Ok(i)) = (float, digits.parse::<i32>())
                    && f as i32 != i
                {
                    let msg =
                        "Widening conversion from Int to Float loses precision.\nWrite `.toFloat` instead.";
                    self.diags.warning(self.ast.span(id).point(), msg);
                }
                float.map(Constant::Float)
            }
            (Some("scala.Double"), k) if k != NumberKind::Whole(16) => {
                from_digits::double(digits).map(Constant::Double)
            }
            (_, NumberKind::Whole(r)) => whole(r),
            _ => from_digits::double(digits).map(Constant::Double),
        };
        let value = match value {
            Ok(v) => v,
            Err(msg) => return self.error(id, msg),
        };
        self.typed_literal(id, value)
    }

    /// Number literal `id` as `fromDigits` of the given `FromDigits` instance for `pt`, in a
    /// pattern as the expression of a block; none if there is none.
    fn generic_number(
        &mut self,
        id: NodeId,
        (digits, kind): (&str, NumberKind),
        pt: TypeId,
        ctx: Ctx,
    ) -> Result<Option<TNodeId>, &'static str> {
        let target = self.u.dealias(pt);
        let value = !matches!(
            self.u.ty(target),
            Type::Method { .. } | Type::Poly { .. } | Type::Bounds(..) | Type::ByName(_) | Type::Wildcard(_)
        );
        if !value || !self.is_fully_defined(target) {
            return Ok(None);
        }
        let class = match kind {
            NumberKind::Whole(10) => "scala.util.FromDigits",
            NumberKind::Whole(_) => "scala.util.FromDigits$.WithRadix",
            NumberKind::Decimal => "scala.util.FromDigits$.Decimal",
            NumberKind::Floating => "scala.util.FromDigits$.Floating",
        };
        let class = self.resolve_full_name(class, true).ok_or("generic number literal without FromDigits")?;
        let wanted = crate::defn::applied(self.u, class, vec![target]);
        let span = self.ast.span(id);
        let arg = match self.implicit_arg(wanted, span.start_pos(), ctx, 0) {
            Ok(arg) => arg,
            Err(what) if is_misfit(what) => return Ok(None),
            Err(what) => return Err(what),
        };
        let pattern = ctx.mode.is(Mode::PATTERN);
        let ctx = ctx.with_mode(ctx.mode.without(Mode::PATTERN));
        let qualifier = self.spliced(arg);
        let name = TName { name: self.name("fromDigits"), is_type: false };
        let fun = self.ast.node(Tree::Select { qualifier, name });
        self.ast.with_span(fun, span);
        let mut values = vec![Constant::String(digits.encode_utf16().collect())];
        if let NumberKind::Whole(r) = kind
            && r != 10
        {
            values.push(Constant::Int(i32::from(r)));
        }
        let args: Vec<NodeId> = values
            .into_iter()
            .map(|value| {
                let arg = self.ast.node(Tree::Literal { value });
                self.ast.with_span(arg, span.end_pos());
                arg
            })
            .collect();
        let args = self.ast.list(&args);
        let app = self.ast.node(Tree::Apply { fun, args });
        self.ast.with_span(app, span.to_synthetic());
        let typed = self.typed(app, Pt::Type(pt), ctx);
        if !pattern {
            return Ok(Some(typed));
        }
        let (stats, span) = (self.tpd.list(&[]), self.tpd.span(typed));
        Ok(Some(self.build().block(stats, typed, span)))
    }

    pub fn typed_literal(&mut self, id: NodeId, value: Constant) -> TNodeId {
        if let Constant::String(ref s) = value
            && String::from_utf16(s).is_err()
        {
            return self.unsupported(id, "string with an unpaired surrogate");
        }
        let span = self.ast.span(id);
        self.build().literal(value, span)
    }
}
