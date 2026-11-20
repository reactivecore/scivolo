// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Checking.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Prefixes of type selections `p.T`, which are stable paths (dotty's `checkStable`).

use super::Typer;
use super::stability::is_stable_path;
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use tasty::{Kind, Type, TypeId};

impl Typer<'_> {
    /// Reports `NotAPath` where `qual`, the prefix of a type selection, is not a stable path, as a
    /// by-name parameter `z` in `z.T` or a `var` (dotty's `checkStable` of a "type prefix").
    pub fn check_type_prefix(&mut self, qual: TNodeId, ctx: Ctx) {
        let tpe = self.tpd.tpe(qual);
        if self.tpd.sym(qual).is_some_and(|q| self.u.is_type_symbol(q))
            || self.tpd.is_type_tree(qual)
            || self.is_stable_path(qual)
            || is_stable_path(self.u, tpe)
            || self.is_capability(qual, ctx)
            || matches!(self.u.ty(tpe), Type::Error)
        {
            return;
        }

        let inline = self.tpd.sym(qual).filter(|&s| {
            let sym = self.u.symbol(s);
            sym.kind == Kind::Param && sym.flags.has(tasty::tags::INLINE)
        });
        let name = inline.map(|s| self.u.symbol(s).name.clone());
        let by_name = self.term_symbol(tpe).is_some_and(|s| {
            let info = self.u.info(s);
            self.u.by_name(info).is_some()
        });
        let found = if by_name { None } else { self.found_text(tpe) };
        let shown = found.unwrap_or_else(|| self.singleton_text(tpe));
        let span = self.tpd.span(qual);
        self.error_at(span, super::errors::not_a_path(&shown, "type prefix", name.as_deref()));
    }

    /// Singleton `t` shown with the type of the term it refers to, `(z : => Box)`, or `t` as
    /// `show_user` shows it if not a singleton.
    fn singleton_text(&mut self, t: TypeId) -> String {
        let shown = self.u.show_user(t);
        let Some(path) = shown.strip_suffix(".type") else { return shown };
        let underlying = match self.term_symbol(t) {
            Some(s) => self.u.info(s),
            None => self.u.widen(t),
        };
        format!("({path} : {})", self.u.show_user(underlying))
    }
}
