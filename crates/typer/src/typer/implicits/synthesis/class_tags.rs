// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeErasure.scala
//   compiler/src/dotty/tools/dotc/typer/Synthesizer.scala
//! Synthesized `ClassTag[T]` (dotty's `synthesizedClassTag`): `ClassTag[E].wrap` for an array
//! `T = Array[E]`; `ClassTag.Unit`, `ClassTag.Any` or `ClassTag.AnyVal`; else
//! `ClassTag.apply[T](classOf[C])` for the class `C` that `T` erases to, an opaque type seen
//! from outside among them.

use super::Synthesizer;
use super::class_tag_erasure::{erased_class_type, has_stable_erasure};
use crate::defn::{ARRAY, CLASS_TAG};
use crate::tpd::{TNodeId, Tree as T};
use crate::typer::Typer;
use crate::typer::type_ops::TypeOps;
use tasty::{Kind, Type, TypeId, TypeVars, Universe};

/// How many type variables instantiating the argument of a class tag passes at most.
const MAX_INSTANTIATIONS: u32 = 8;

impl Synthesizer {
    /// Synthesizes `ClassTag[arg]`, inferring the type arguments still open in `arg`; `None` where
    /// `arg` has no single runtime class, e.g. an abstract type or an unbounded type parameter.
    pub fn class_tag(self, t: &mut Typer<'_>, arg: TypeId) -> Result<Option<TNodeId>, &'static str> {
        let at = self.at;
        let Some(arg) = class_tag_arg(t, arg)? else { return Ok(None) };
        let dealiased = t.u.dealias(arg);
        let array_elem = match t.is_opaque_ref(dealiased) {
            true => None,
            false => t.class_args(dealiased, ARRAY),
        };
        if let Some(&[elem]) = array_elem.as_deref() {
            let class_tag = t.resolve_full_name(CLASS_TAG, true).ok_or("ClassTag")?;
            let elem_tag = crate::defn::applied(t.u, class_tag, vec![elem]);
            let elem_tag = t.implicit_arg(elem_tag, at, self.ctx, self.depth + 1)?;
            let wrap =
                t.u.decl(class_tag, "wrap").ok().and_then(|d| d.first().copied()).ok_or("ClassTag.wrap")?;
            let prefix = t.tpd.tpe(elem_tag);
            let prefix = t.widen_if_unstable(prefix);
            let tpe = t.u.intern_type(Type::Sym(Some(prefix), wrap));
            let name = t.build().term_name(wrap);
            return Ok(Some(t.tpd.node(T::Select { qualifier: elem_tag, name }, at, Some(tpe), Some(wrap))));
        }
        if !has_stable_erasure(t, arg) {
            return Ok(None);
        }
        if is_bottom(t.u, arg) {
            return Err("class tag of a bottom type");
        }
        let module = t.resolve_full_name(CLASS_TAG, false).ok_or("ClassTag")?;
        let module_tpe = crate::defn::type_ref(t.u, module);
        let class = match t.u.types().get(dealiased).clone() {
            Type::And(..) | Type::Or(..) => None,
            Type::Applied(tycon, _)
                if t.referenced(tycon).is_some_and(|s| t.u.symbol(s).kind != Kind::Class) =>
            {
                None
            }
            _ => t.u.class_symbol(dealiased),
        };
        let b = t.u.builtins();
        let special = class.and_then(|c| match c {
            _ if c == t.defn.unit => Some("Unit"),
            _ if c == b.any => Some("Any"),
            _ if Some(c) == b.any_val => Some("AnyVal"),
            _ => None,
        });
        if let Some(special) = special {
            if !matches!(t.u.types().get(arg), Type::Sym(..) | Type::TypeRef(..))
                || t.referenced(arg).is_none_or(|s| t.u.symbol(s).kind != Kind::Class)
            {
                return Err("class tag of an alias of a special class");
            }
            let member = t.resolve_full_name(&format!("scala.reflect.ClassTag$.{special}"), false);
            let member = member.ok_or("ClassTag member")?;
            let qualifier = t.build().ident(module, module_tpe, at);
            return Ok(Some(t.build().select(qualifier, member, at)));
        }
        let erased = erased_class_type(t, arg).ok_or("class tag of a type erasing to no class")?;
        let module_class = t.u.module_class(module).ok_or("ClassTag")?;
        let apply =
            t.u.decl(module_class, "apply")
                .unwrap_or_default()
                .into_iter()
                .find(|&d| t.u.symbol(d).kind == Kind::Def)
                .ok_or("ClassTag.apply")?;
        let qualifier = t.build().ident(module, module_tpe, at);
        let select = t.build().select(qualifier, apply, at);
        let targ = t.type_tree(arg, at, false);
        let method = t.instantiated(select, &[arg])?;
        let fun = t.build().type_apply(select, &[targ], method, at);
        let Type::Method { result, .. } = *t.u.types().get(method) else {
            return Err("ClassTag.apply");
        };
        let class = t.resolve_full_name("java.lang.Class", true).ok_or("java.lang.Class")?;
        let class_tpe = crate::defn::applied(t.u, class, vec![erased]);
        let literal = t.tpd.node(T::ClassLiteral { tpe: erased }, at, Some(class_tpe), None);
        Ok(Some(t.build().apply(fun, &[literal], result, at)))
    }
}

/// The type argument of a class tag (dotty's `instArg`): an intersection or union of equal
/// sides reduced to one; an uninstantiated type variable instantiated to its lower bound, else
/// its upper bound, if that bound has no type variable on top, else fully defined; `None` for
/// one without bounds or that cannot be fully defined.
fn class_tag_arg(t: &mut Typer<'_>, arg: TypeId) -> Result<Option<TypeId>, &'static str> {
    for _ in 0..MAX_INSTANTIATIONS {
        let arg = t.zonk(arg);
        let dealiased = t.u.dealias(arg);
        let tpe = t.u.types().get(dealiased).clone();
        let v = match tpe {
            Type::And(a, b) | Type::Or(a, b, _) if a == b => return class_tag_arg(t, a),
            Type::And(..) | Type::Or(..) if !t.open_vars(dealiased).is_empty() => {
                return Err("class tag of an intersection or union with a type variable");
            }
            Type::Var(v) => v,
            _ => return Ok(Some(arg)),
        };
        let (lo, hi) = t.constraint.bounds(v);
        let nothing = t.u.builtins().nothing;
        let (bound, from_below) = match lo != tasty::NOTHING && t.u.class_symbol(lo) != Some(nothing) {
            true => (lo, true),
            false if !t.is_exactly_any(hi) => (hi, false),
            false => return Ok(None),
        };
        if is_ground(&mut t.types(), bound) {
            t.instantiate_var(v, from_below)?;
        } else if t.fully_defined_as(arg, true).is_err() {
            return Ok(None);
        }
    }
    Err("class tag of an undetermined type")
}

/// Whether `t` is no uninstantiated type variable, nor an intersection or union with one
/// (dotty's `isGroundConstr`).
fn is_ground(ty: &mut TypeOps<'_>, t: TypeId) -> bool {
    let t = ty.zonk(t);
    let t = ty.u.dealias(t);
    match *ty.u.types().get(t) {
        Type::Var(_) => false,
        Type::And(a, b) | Type::Or(a, b, _) => is_ground(ty, a) && is_ground(ty, b),
        _ => true,
    }
}

/// Determines whether `t` is `Nothing` or `Null`, an intersection with such a side, or a union of
/// such sides.
fn is_bottom(u: &mut Universe, t: TypeId) -> bool {
    let t = u.dealias(t);
    match *u.types().get(t) {
        Type::And(a, b) => is_bottom(u, a) || is_bottom(u, b),
        Type::Or(a, b, _) => is_bottom(u, a) && is_bottom(u, b),
        _ => {
            let b = u.builtins();
            u.class_symbol(t).is_some_and(|c| c == b.nothing || c == b.null)
        }
    }
}
