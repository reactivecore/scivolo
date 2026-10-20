// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Whether an alternative applies to named arguments, reordered by its parameter names (dotty's
//! `ApplicableToTypes` with `reorder`).

use super::Typer;
use crate::ctx::Ctx;
use tasty::{SymbolId, Type, TypeId};

impl Typer<'_> {
    /// `applicable` of `alt` to the arguments of `proto` of types `arg_types`, named ones at the
    /// parameters of their names; `Some(false)` if the names do not fit, `None` for more arguments
    /// than parameters of a repeated one.
    pub fn applicable_to_proto(
        &mut self,
        alt: SymbolId,
        prefix: Option<TypeId>,
        (proto, arg_types): (usize, &[TypeId]),
        views: bool,
        ctx: Ctx,
    ) -> Option<bool> {
        let args = self.protos[proto].args.clone();
        if args.iter().all(|&a| self.arg_name(a).is_none()) {
            return self.applicable(alt, prefix, arg_types, views, ctx);
        }
        let tpe = self.u.intern_type(Type::Sym(prefix, alt));
        let mut info = self.method_info(tpe)?;
        if let Type::Poly { result, .. } = *self.u.types().get(info) {
            info = result;
        }
        let Type::Method { params, .. } = self.u.types().get(info).clone() else { return Some(false) };
        let repeated = params.last().is_some_and(|&(_, p)| self.repeated_param_elem(p).is_some());
        if repeated && args.len() > params.len() {
            return None;
        }
        let names: Vec<String> = params.into_iter().map(|(n, _)| n).collect();
        let Ok(aligned) = self.ordered_args(&args, &names) else { return Some(false) };
        let slots: Vec<Option<TypeId>> = aligned
            .iter()
            .map(|slot| slot.and_then(|a| args.iter().position(|&b| b == a)).map(|i| arg_types[i]))
            .collect();
        self.applicable_slots(alt, prefix, &slots, views, ctx)
    }
}
