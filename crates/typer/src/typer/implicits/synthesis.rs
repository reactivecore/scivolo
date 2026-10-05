//! Implicit arguments the compiler makes instead of finding them (dotty's `Synthesizer`): a
//! `ClassTag[T]` for a class `T` without type arguments, `ClassTag.apply[T](classOf[T])`.

use super::super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{Span, TName};
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
        let arg = self.fully_defined(arg)?;
        let arg = self.u.dealias(arg);
        let Type::Sym(_, c) = *self.u.types().get(arg) else {
            return Err("class tag of a type that is not a class");
        };
        if self.u.symbol(c).kind != Kind::Class || !self.u.type_params(c).is_empty() {
            return Err("class tag of a type that is not a class");
        }
        let module = self.resolve_full_name("scala.reflect.ClassTag", false).ok_or("ClassTag")?;
        let module_tpe = crate::defn::type_ref(self.u, module);
        let module_class = self.u.module_class(module).ok_or("ClassTag")?;
        let apply = self
            .u
            .decl(module_class, "apply")
            .unwrap_or_default()
            .into_iter()
            .find(|&d| self.u.symbol(d).kind == Kind::Def)
            .ok_or("ClassTag.apply")?;
        let name = TName { name: self.name("ClassTag"), is_type: false };
        let qualifier = self.tpd.node(T::Ident { name }, at, Some(module_tpe), Some(module));
        let apply_tpe = self.u.intern_type(Type::Sym(Some(module_tpe), apply));
        let name = TName { name: self.name("apply"), is_type: false };
        let select = self.tpd.node(T::Select { qualifier, name }, at, Some(apply_tpe), Some(apply));
        let targ = self.type_tree(arg, at, false);
        let method = self.instantiated(select, &[arg])?;
        let targs = self.tpd.list(&[targ]);
        let fun = self.tpd.node(T::TypeApply { fun: select, args: targs }, at, Some(method), None);
        let Type::Method { result, .. } = self.u.types().get(method).clone() else {
            return Err("ClassTag.apply");
        };
        let class_tpe = self.class_of(arg);
        let literal = self.tpd.node(T::ClassLiteral { tpe: arg }, at, Some(class_tpe), None);
        let args = self.tpd.list(&[literal]);
        Ok(Some(self.tpd.node(T::Apply { fun, args }, at, Some(result), None)))
    }

    /// `java.lang.Class[t]`.
    fn class_of(&mut self, t: TypeId) -> TypeId {
        let class = self.resolve_full_name("java.lang.Class", true).expect("java.lang.Class");
        let class = crate::defn::type_ref(self.u, class);
        self.u.intern_type(Type::Applied(class, vec![t]))
    }
}
