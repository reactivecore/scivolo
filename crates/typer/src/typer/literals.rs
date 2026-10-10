//! Literals and their types (dotty's `typedLiteral`).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::NodeId;
use syntax::ast::constant::Constant;
use syntax::ast::from_digits;
use syntax::ast::tree::NumberKind;
use syntax::config::Features;
use tasty::TypeId;

impl Typer<'_> {
    /// The type of literal `c`: `Unit` and `Null` for `()` and `null`, else a constant type.
    pub fn literal_type(&mut self, c: &Constant) -> TypeId {
        match c {
            Constant::Unit => crate::defn::type_ref(self.u, self.defn.unit),
            Constant::Null => crate::defn::type_ref(self.u, self.u.builtins().null),
            _ => self.build().constant_type(c),
        }
    }

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
        if matches!(pt, Pt::Type(_))
            && !primitive
            && self.feature_enabled(generic, "experimental.genericNumberLiterals", ctx)
        {
            return self.unsupported(id, "generic number literal");
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

    pub fn typed_literal(&mut self, id: NodeId, value: Constant) -> TNodeId {
        if let Constant::String(ref s) = value
            && String::from_utf16(s).is_err()
        {
            return self.unsupported(id, "string with an unpaired surrogate");
        }
        let span = self.ast.span(id);
        if !matches!(value, Constant::Unit | Constant::Null) {
            return self.build().literal(value, span);
        }
        let tpe = self.literal_type(&value);
        self.tpd.node(T::Literal { value }, span, Some(tpe), None)
    }
}
