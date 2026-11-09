// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//! `NotGiven[T]` (dotty's `isNotGiven` with `negateIfNot`): `NotGiven.value` where the search for
//! `T` finds nothing.

use super::super::Typer;
use super::super::type_ops::TypeOps;
use super::search::ImplicitSearch;
use crate::tpd::TNodeId;
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

/// `T` if `pt` is `NotGiven[T]`.
pub fn not_given_arg(ty: &mut TypeOps<'_>, pt: TypeId) -> Option<TypeId> {
    match ty.class_args(pt, "scala.util.NotGiven")?[..] {
        [arg] => Some(arg),
        _ => None,
    }
}

impl ImplicitSearch {
    /// `NotGiven.value` if no implicit argument of type `negated` exists.
    pub fn not_given(&self, t: &mut Typer<'_>, negated: TypeId) -> Result<TNodeId, &'static str> {
        if !t.types().is_fully_defined(negated) {
            return Err("NotGiven of a type with type variables");
        }
        for part in t.types().type_parts(negated) {
            let Some(class) = t.u.class_symbol(part) else { continue };
            let name = t.u.full_name(class);
            let inner =
                t.u.symbol(class).owner.is_some_and(|o| t.u.symbol(o).is_class() && !t.u.is_module_class(o));
            if inner || UNSEARCHED.iter().any(|u| name.starts_with(u)) {
                return Err("NotGiven of a type whose instances the search may miss");
            }
        }
        if t.implicit_exists(negated, self.at, self.ctx, self.depth + 1)? {
            return Err("no implicit argument");
        }
        let module = t.resolve_full_name("scala.util.NotGiven", false).ok_or("NotGiven")?;
        let module_class = t.u.module_class(module).ok_or("NotGiven")?;
        let value = t.u.decl(module_class, "value").unwrap_or_default();
        let value = value.into_iter().find(|&d| t.u.symbol(d).kind == Kind::Def).ok_or("NotGiven.value")?;
        let class = crate::defn::type_ref(t.u, module_class);
        let this = t.u.intern_type(Type::This(class));
        let tpe = t.u.intern_type(Type::Sym(Some(this), value));
        Ok(t.build().ident(value, tpe, self.at))
    }
}
