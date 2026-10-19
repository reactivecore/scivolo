//! Synthesized `ClassTag[T]` (dotty's `synthesizedClassTag`): `ClassTag[E].wrap` for an array
//! `T = Array[E]`; `ClassTag.Unit`, `ClassTag.Any` or `ClassTag.AnyVal`; else
//! `ClassTag.apply[T](classOf[C])` for the class `C` that `T` erases to.

use super::super::Typer;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::Span;
use tasty::{Erased, Kind, SymbolId, Type, TypeId};

const EMPTY_TUPLE: &str = "scala.Tuple$package$.EmptyTuple$";

impl Typer<'_> {
    pub fn synthesized_class_tag(
        &mut self,
        arg: TypeId,
        at: Span,
        ctx: Ctx,
        depth: u32,
    ) -> Result<TNodeId, &'static str> {
        let arg = self.class_tag_arg(arg)?;
        let dealiased = self.u.dealias(arg);
        if let Some(&[elem]) = self.class_args(dealiased, "scala.Array").as_deref() {
            let class_tag = self.resolve_full_name("scala.reflect.ClassTag", true).ok_or("ClassTag")?;
            let elem_tag = crate::defn::applied(self.u, class_tag, vec![elem]);
            let elem_tag = self.implicit_arg(elem_tag, at, ctx, depth + 1)?;
            let wrap = self
                .u
                .decl(class_tag, "wrap")
                .ok()
                .and_then(|d| d.first().copied())
                .ok_or("ClassTag.wrap")?;
            let prefix = self.tpd.tpe(elem_tag);
            let prefix = self.widen_if_unstable(prefix);
            let tpe = self.u.intern_type(Type::Sym(Some(prefix), wrap));
            let name = self.build().term_name(wrap);
            return Ok(self.tpd.node(T::Select { qualifier: elem_tag, name }, at, Some(tpe), Some(wrap)));
        }
        if !self.has_stable_erasure(arg) {
            return Err("class tag of a type without a stable erasure");
        }
        if self.is_bottom(arg) {
            return Err("class tag of a bottom type");
        }
        let module = self.resolve_full_name("scala.reflect.ClassTag", false).ok_or("ClassTag")?;
        let module_tpe = crate::defn::type_ref(self.u, module);
        let class = match self.u.types().get(dealiased) {
            Type::And(..) | Type::Or(..) => None,
            _ => self.u.class_symbol(dealiased),
        };
        let b = self.u.builtins();
        let special = class.and_then(|c| match c {
            _ if c == self.defn.unit => Some("Unit"),
            _ if c == b.any => Some("Any"),
            _ if Some(c) == b.any_val => Some("AnyVal"),
            _ => None,
        });
        if let Some(special) = special {
            if !matches!(self.u.types().get(arg), Type::Sym(..) | Type::TypeRef(..))
                || self.referenced(arg).is_none_or(|s| self.u.symbol(s).kind != Kind::Class)
            {
                return Err("class tag of an alias of a special class");
            }
            let member = self.resolve_full_name(&format!("scala.reflect.ClassTag$.{special}"), false);
            let member = member.ok_or("ClassTag member")?;
            let qualifier = self.build().ident(module, module_tpe, at);
            return Ok(self.build().select(qualifier, member, at));
        }
        let erased = self.erased_class_type(arg).ok_or("class tag of a type erasing to no class")?;
        let module_class = self.u.module_class(module).ok_or("ClassTag")?;
        let apply = self
            .u
            .decl(module_class, "apply")
            .unwrap_or_default()
            .into_iter()
            .find(|&d| self.u.symbol(d).kind == Kind::Def)
            .ok_or("ClassTag.apply")?;
        let qualifier = self.build().ident(module, module_tpe, at);
        let select = self.build().select(qualifier, apply, at);
        let targ = self.type_tree(arg, at, false);
        let method = self.instantiated(select, &[arg])?;
        let fun = self.build().type_apply(select, &[targ], method, at);
        let Type::Method { result, .. } = self.u.types().get(method).clone() else {
            return Err("ClassTag.apply");
        };
        let class = self.resolve_full_name("java.lang.Class", true).ok_or("java.lang.Class")?;
        let class_tpe = crate::defn::applied(self.u, class, vec![erased]);
        let literal = self.tpd.node(T::ClassLiteral { tpe: erased }, at, Some(class_tpe), None);
        Ok(self.build().apply(fun, &[literal], result, at))
    }

    /// The type argument of a class tag fully defined, an intersection or union of equal sides
    /// reduced to one; `Err` if a type variable in it has no lower bound.
    fn class_tag_arg(&mut self, arg: TypeId) -> Result<TypeId, &'static str> {
        let arg = self.zonk(arg);
        let mut unbounded = false;
        self.u.map_type(arg, &mut |u, part| {
            if let Type::Var(v) = *u.types().get(part) {
                unbounded |= tasty::TypeVars::bounds(&self.constraint, v).0 == tasty::NOTHING;
            }
            None
        });
        if unbounded {
            return Err("class tag of an undetermined type");
        }
        let mut arg = self.fully_defined_as(arg, true)?;
        loop {
            let dealiased = self.u.dealias(arg);
            match *self.u.types().get(dealiased) {
                Type::And(a, b) | Type::Or(a, b, _) if a == b => arg = a,
                _ => return Ok(arg),
            }
        }
    }

    /// Whether every instance of `t` erases alike (dotty's `hasStableErasure`).
    fn has_stable_erasure(&mut self, t: TypeId) -> bool {
        let t = self.u.dealias(t);
        match self.u.types().get(t).clone() {
            Type::And(a, b) | Type::Or(a, b, _) => self.has_stable_erasure(a) && self.has_stable_erasure(b),
            Type::Applied(inner, _) | Type::Refined(inner, ..) | Type::Annotated(inner, _) => {
                self.has_stable_erasure(inner)
            }
            Type::Constant(..) | Type::Str(_) => true,
            Type::Sym(..) | Type::TypeRef(..) | Type::TermRef(..) => match self.referenced(t) {
                Some(s) if self.u.symbol(s).kind == Kind::Class => true,
                Some(s) if self.u.symbol(s).kind.is_term() => {
                    let widened = self.u.widen(t);
                    widened != t && self.has_stable_erasure(widened)
                }
                _ => false,
            },
            _ => false,
        }
    }

    /// Whether `t` is `Nothing` or `Null`, an intersection with such a side, or a union of such
    /// sides.
    fn is_bottom(&mut self, t: TypeId) -> bool {
        let t = self.u.dealias(t);
        match *self.u.types().get(t) {
            Type::And(a, b) => self.is_bottom(a) || self.is_bottom(b),
            Type::Or(a, b, _) => self.is_bottom(a) && self.is_bottom(b),
            _ => {
                let b = self.u.builtins();
                self.u.class_symbol(t).is_some_and(|c| c == b.nothing || c == b.null)
            }
        }
    }

    /// A reference to the class `t` erases to: a value class itself, else the class among those
    /// of the sides of `t`, their base classes and `Object` that erases as `t` does; `None` for a
    /// generic tuple.
    fn erased_class_type(&mut self, t: TypeId) -> Option<TypeId> {
        let mut classes = Vec::new();
        self.side_classes(t, &mut classes);
        if let [c] = classes[..]
            && self.u.underlying(c).is_some()
        {
            return Some(crate::defn::type_ref(self.u, c));
        }
        let tuple = |u: &mut tasty::Universe, c| matches!(u.full_name(c).as_str(), "scala.*:" | EMPTY_TUPLE);
        if classes.is_empty() || classes.iter().any(|&c| tuple(self.u, c)) {
            return None;
        }
        let erased = self.u.erase(t);
        let mut bases: Vec<SymbolId> = classes.iter().flat_map(|&c| self.u.linearization(c)).collect();
        bases.extend(self.resolve_full_name("java.lang.Object", true));
        let found = bases.into_iter().find(|&c| {
            let class = crate::defn::type_ref(self.u, c);
            self.u.underlying(c).is_none()
                && match &erased {
                    Erased::Class(name) => self.u.binary_name(c) == *name,
                    _ => self.u.erase(class) == erased,
                }
        })?;
        Some(crate::defn::type_ref(self.u, found))
    }

    /// Adds the classes of the sides of intersection or union `t`, or of `t`, to `out`.
    fn side_classes(&mut self, t: TypeId, out: &mut Vec<SymbolId>) {
        let t = self.u.dealias(t);
        match *self.u.types().get(t) {
            Type::And(a, b) | Type::Or(a, b, _) => {
                self.side_classes(a, out);
                self.side_classes(b, out);
            }
            _ => {
                let widened = self.u.widen(t);
                let widened = self.u.dealias(widened);
                out.extend(self.u.class_symbol(widened));
            }
        }
    }
}
