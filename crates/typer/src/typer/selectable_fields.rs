// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Selections of the fields a `Selectable`'s `Fields` type member names (dotty's
//! `typedSelectWithAdapt` for `Fields`).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::{NodeId, TName};
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// `qual.selectDynamic("name")`, cast to the field's type unless it conforms, if `qual`'s
    /// type `pre` derives from `Selectable` and its `Fields`, a named tuple type, has a field
    /// `name`; `None` otherwise.
    pub fn selectable_field(
        &mut self,
        id: NodeId,
        (qual, pre): (TNodeId, TypeId),
        name: TName,
        ctx: Ctx,
    ) -> Option<TNodeId> {
        if name.is_type || self.dynamic_lhs == Some(id) {
            return None;
        }
        let selectable = self.resolve_full_name("scala.Selectable", true)?;
        let widened = self.u.widen(pre);
        let class = self.u.class_symbol(widened)?;
        if !self.u.derives_from(class, selectable) {
            return None;
        }
        let fields = self.type_members(pre, "Fields").into_iter().find(|&s| self.u.is_type_symbol(s))?;
        let fields = self.u.intern_type(Type::Sym(Some(pre), fields));
        let fields = self.u.dealias(fields);
        let fields = self.with_case_fields(fields);
        let fields = self.u.normalized(fields);
        let (names, values) = self.derived_named_tuple_parts(fields)?;
        let text = self.text(name.name).to_owned();
        let field = values[names.iter().position(|n| *n == text)?];
        let span = self.ast.span(id);
        let (fun, literal) = self.structural_fun(qual, "selectDynamic", &text, span);
        let (apply, _) = self.applied(fun, &[literal]);
        let call = self.typed(apply, Pt::Any, ctx);
        let tpe = self.tpd.tpe(call);
        let result = if self.is_error(call) || self.u.is_subtype(tpe, field) {
            call
        } else {
            self.cast_to_captured(call, field).unwrap_or_else(|what| self.unsupported(id, what))
        };
        self.tpd.get_mut(result).span = span;
        Some(result)
    }
}
