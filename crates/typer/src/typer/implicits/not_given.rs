//! `NotGiven[T]` (dotty's `isNotGiven` with `negateIfNot`): `NotGiven.value` where the search for
//! `T` finds nothing.

use super::super::Typer;
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::Span;
use tasty::{Kind, Type, TypeId};

/// The classes whose instances dotty synthesizes and this search does not.
const UNSYNTHESIZED: [&str; 8] = [
    "scala.deriving.Mirror",
    "scala.reflect.TypeTest",
    "scala.reflect.Manifest",
    "scala.reflect.OptManifest",
    "scala.ValueOf",
    "scala.util.TupledFunction",
    "scala.Singleton",
    "scala.Precise",
];

impl Typer<'_> {
    /// `T` if `pt` is `NotGiven[T]`.
    pub fn not_given_arg(&mut self, pt: TypeId) -> Option<TypeId> {
        match self.class_args(pt, "scala.util.NotGiven")?[..] {
            [t] => Some(t),
            _ => None,
        }
    }

    /// `NotGiven.value` at `at` if no implicit argument of type `t` exists in `ctx`.
    pub fn not_given(&mut self, t: TypeId, at: Span, ctx: Ctx, depth: u32) -> Result<TNodeId, &'static str> {
        if !self.is_fully_defined(t) {
            return Err("NotGiven of a type with type variables");
        }
        for part in self.type_parts(t) {
            let class = self.u.class_symbol(part).map(|c| self.u.full_name(c)).unwrap_or_default();
            if UNSYNTHESIZED.iter().any(|u| class.starts_with(u)) {
                return Err("NotGiven of a type whose instances dotty synthesizes");
            }
        }
        if self.implicit_exists(t, at, ctx, depth + 1)? {
            return Err("no implicit argument");
        }
        let module = self.resolve_full_name("scala.util.NotGiven", false).ok_or("NotGiven")?;
        let module_class = self.u.module_class(module).ok_or("NotGiven")?;
        let value = self.u.decl(module_class, "value").unwrap_or_default();
        let value =
            value.into_iter().find(|&d| self.u.symbol(d).kind == Kind::Def).ok_or("NotGiven.value")?;
        let class = crate::defn::type_ref(self.u, module_class);
        let this = self.u.intern_type(Type::This(class));
        let tpe = self.u.intern_type(Type::Sym(Some(this), value));
        Ok(self.build().ident(value, tpe, at))
    }
}
