// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeErasure.scala
//   compiler/src/dotty/tools/dotc/typer/Synthesizer.scala
//! Synthesized `ClassTag[T]` (dotty's `synthesizedClassTag`): `ClassTag[E].wrap` for an array
//! `T = Array[E]`; `ClassTag.Unit`, `ClassTag.Any` or `ClassTag.AnyVal`; else
//! `ClassTag.apply[T](classOf[C])` for the class `C` that `T` erases to.

use super::super::Typer;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::Span;
use tasty::{Kind, Type, TypeId};

impl Typer<'_> {
    pub fn synthesized_class_tag(
        &mut self,
        arg: TypeId,
        at: Span,
        ctx: Ctx,
        depth: u32,
    ) -> Result<Option<TNodeId>, &'static str> {
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
            return Ok(Some(self.tpd.node(
                T::Select { qualifier: elem_tag, name },
                at,
                Some(tpe),
                Some(wrap),
            )));
        }
        if !self.has_stable_erasure(arg) {
            return Ok(None);
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
            return Ok(Some(self.build().select(qualifier, member, at)));
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
        Ok(Some(self.build().apply(fun, &[literal], result, at)))
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
}
