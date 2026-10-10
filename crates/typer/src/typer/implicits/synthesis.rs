//! Implicit arguments the compiler makes instead of finding them (dotty's `Synthesizer`): a
//! `ClassTag[T]` for a class `T` without type arguments, `ClassTag.apply[T](classOf[T])` or, for
//! `Unit`, `Any` and `AnyVal`, `ClassTag.Unit`, `ClassTag.Any`, `ClassTag.AnyVal`.

use super::super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::Span;
use tasty::{Kind, Type, TypeId};

impl Typer<'_> {
    /// The synthesized argument of type `pt` at `at`, if `pt` is one the compiler makes.
    pub fn synthesized(&mut self, pt: TypeId, at: Span) -> Result<Option<TNodeId>, &'static str> {
        let Type::Applied(tycon, args) = self.u.types().get(pt).clone() else { return Ok(None) };
        let Some(class) = self.u.class_symbol(tycon) else { return Ok(None) };
        if self.u.full_name(class) != "scala.reflect.ClassTag" {
            return Ok(None);
        }
        let [arg] = args[..] else { return Ok(None) };
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
        let orig = self.fully_defined(arg)?;
        let arg = self.u.dealias(orig);
        let class = match *self.u.types().get(arg) {
            Type::Sym(..) | Type::TypeRef(..) => self.u.class_symbol(arg),
            _ => None,
        };
        let Some(c) = class else { return Err("class tag of a type that is not a class") };
        if self.u.symbol(c).kind != Kind::Class || !self.u.type_params(c).is_empty() {
            return Err("class tag of a type that is not a class");
        }
        let module = self.resolve_full_name("scala.reflect.ClassTag", false).ok_or("ClassTag")?;
        let module_tpe = crate::defn::type_ref(self.u, module);
        let module_class = self.u.module_class(module).ok_or("ClassTag")?;
        let full = self.u.full_name(c);
        if full == "scala.Nothing" || full == "scala.Null" {
            return Err("class tag of a bottom type");
        }
        if let Some(special) = ["Unit", "Any", "AnyVal"].into_iter().find(|n| full == format!("scala.{n}")) {
            let member = self.resolve_full_name(&format!("scala.reflect.ClassTag$.{special}"), false);
            let member = member.ok_or("ClassTag member")?;
            let qualifier = self.build().ident(module, module_tpe, at);
            return Ok(Some(self.build().select(qualifier, member, at)));
        }
        let apply = self
            .u
            .decl(module_class, "apply")
            .unwrap_or_default()
            .into_iter()
            .find(|&d| self.u.symbol(d).kind == Kind::Def)
            .ok_or("ClassTag.apply")?;
        let qualifier = self.build().ident(module, module_tpe, at);
        let select = self.build().select(qualifier, apply, at);
        let targ = self.type_tree(orig, at, false);
        let method = self.instantiated(select, &[orig])?;
        let fun = self.build().type_apply(select, &[targ], method, at);
        let Type::Method { result, .. } = self.u.types().get(method).clone() else {
            return Err("ClassTag.apply");
        };
        let class_tpe = self.class_of(arg).ok_or("java.lang.Class")?;
        let literal = self.tpd.node(T::ClassLiteral { tpe: arg }, at, Some(class_tpe), None);
        Ok(Some(self.build().apply(fun, &[literal], result, at)))
    }

    /// `java.lang.Class[t]`.
    fn class_of(&mut self, t: TypeId) -> Option<TypeId> {
        let class = self.resolve_full_name("java.lang.Class", true)?;
        let class = crate::defn::type_ref(self.u, class);
        Some(self.u.intern_type(Type::Applied(class, vec![t])))
    }
}
