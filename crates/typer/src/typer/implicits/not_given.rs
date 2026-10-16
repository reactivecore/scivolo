//! `NotGiven[T]` (dotty's `isNotGiven` with `negateIfNot`): `NotGiven.value` where the search for
//! `T` finds nothing.

use super::super::Typer;
use crate::ctx::{Ctx, Level};
use crate::tpd::TNodeId;
use syntax::ast::Span;
use tasty::{Kind, Type, TypeId};

/// The classes of which dotty finds more instances than this search: those it synthesizes, and
/// functions, which every implicit method may be eta-expanded to.
const UNSEARCHED: [&str; 10] = [
    "scala.Function",
    "scala.ContextFunction",
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
            let Some(class) = self.u.class_symbol(part) else { continue };
            let name = self.u.full_name(class);
            let inner = self
                .u
                .symbol(class)
                .owner
                .is_some_and(|o| self.u.symbol(o).kind == Kind::Class && !self.u.is_module_class(o));
            if inner || UNSEARCHED.iter().any(|u| name.starts_with(u)) {
                return Err("NotGiven of a type whose instances the search may miss");
            }
        }
        if self.has_package_implicits(ctx) {
            return Err("NotGiven beside top-level implicits");
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

    /// Whether a package enclosing `ctx` has top-level implicits, which the search does not see.
    fn has_package_implicits(&mut self, ctx: Ctx) -> bool {
        let mut level = Some(ctx.level);
        while let Some(l) = level {
            if let Level::Members(p) = *self.levels.get(l)
                && self.u.symbol(p).kind == Kind::Package
            {
                let Ok(objects) = self.u.package_objects(p) else { return true };
                for d in objects {
                    if !self.implicit_members(d, false).is_empty() {
                        return true;
                    }
                }
            }
            level = self.levels.outer(l);
        }
        false
    }
}
