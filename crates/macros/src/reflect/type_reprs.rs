// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The operations over `TypeRepr`s: names, printing, construction and member types.

use super::Fallible;
use crate::host::Host;
use tasty::{Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree as T};

impl Host {
    pub fn show_type(&mut self, t: TypeId) -> Fallible<String> {
        self.u.show_source(t)
    }

    pub fn show_type_structure(&mut self, t: TypeId) -> Fallible<String> {
        self.type_structure(t)
    }

    pub fn type_name(&mut self, t: TypeId) -> Fallible<String> {
        match self.u.types().get(t).clone() {
            Type::Sym(_, s) | Type::Package(s) => Ok(self.u.symbol(s).name.clone()),
            Type::TypeRef(_, n) | Type::TermRef(_, n) | Type::Refined(_, n, _) => Ok(n),
            _ => Err("the name of an unnamed type"),
        }
    }

    /// `tycon[targs]`, `tycon` for no `targs`.
    pub fn applied_to(&mut self, tycon: TypeId, targs: Vec<TypeId>) -> TypeId {
        match targs.is_empty() {
            true => tycon,
            false => self.u.intern_type(Type::Applied(tycon, targs)),
        }
    }

    pub fn dealias(&mut self, t: TypeId) -> TypeId {
        self.u.dealias(t)
    }

    /// The method type of parameter types and the result type `types`, the parameters named by
    /// `names` separated by `\0`.
    pub fn method_type(&mut self, types: Vec<TypeId>, names: String) -> Fallible<TypeId> {
        let (&result, params) = types.split_last().ok_or("MethodType.apply without its result type")?;
        let names: Vec<String> = names.split('\0').filter(|n| !n.is_empty()).map(str::to_owned).collect();
        if names.len() != params.len() {
            return Err("MethodType.apply with another number of names than types");
        }
        let binder = self.u.fresh_binder();
        let params = names.into_iter().zip(params.iter().copied()).collect();
        let t = Type::Method { binder, params, result, implicit: false, contextual: false };
        Ok(self.u.intern_type(t))
    }

    pub fn by_name(&mut self, t: TypeId) -> TypeId {
        self.u.intern_type(Type::ByName(t))
    }

    pub fn is_singleton(&mut self, t: TypeId) -> bool {
        match *self.u.types().get(t) {
            Type::Sym(_, s) => self.u.symbol(s).kind.is_term(),
            Type::TermRef(..) | Type::This(_) | Type::Super(..) | Type::Skolem(..) => true,
            Type::Constant(..) | Type::Str(_) => true,
            _ => false,
        }
    }

    pub fn base_classes(&mut self, t: TypeId) -> Fallible<Vec<SymbolId>> {
        let w = self.u.widen(t);
        let c = self.u.class_symbol(w).ok_or("baseClasses of a type without a class")?;
        Ok(self.u.linearization(c))
    }

    pub fn member_type(&mut self, t: TypeId, s: SymbolId) -> Fallible<TypeId> {
        Ok(self.repeated_param(s)?.unwrap_or_else(|| self.u.term_type(Some(t), s)))
    }

    pub fn term_ref(&mut self, qual: TypeId, name: String) -> Fallible<TypeId> {
        self.reflect().term_ref(qual, &name)
    }

    /// The annotation of annotated type `t` as a `new` of its type.
    pub fn annotation(&mut self, t: TypeId) -> Fallible<TNodeId> {
        let Type::Annotated(_, annot) = *self.u.types().get(t) else {
            return Err("AnnotatedType.annotation of another type");
        };
        let span = self.expansion;
        let tpt = self.tpd.node(T::TypeTree { inferred: false }, span, Some(annot), None);
        Ok(self.tpd.node(T::New { tpt }, span, Some(annot), None))
    }

    pub fn symbol_ref(&mut self, s: SymbolId) -> TypeId {
        self.reflect().symbol_ref(s)
    }

    /// The qualifier of a reference to `s` without a prefix (dotty's `owner.thisType`): the `this`
    /// of its package or static owner, no prefix for a package's own type or a local; `Err` for a
    /// member of another class.
    pub fn qualifier(&mut self, s: SymbolId) -> Result<TypeId, &'static str> {
        let package = |h: &Host, s| h.u.symbol(s).kind == Kind::Package;
        let t = match self.u.symbol(s).owner {
            _ if package(self, s) => Type::NoPrefix,
            Some(o) if package(self, o) => return Ok(self.package_this_of(o)),
            Some(o) if self.u.is_class(o) && self.u.is_static_symbol(s) => {
                Type::This(self.u.intern_type(Type::Sym(None, o)))
            }
            Some(o) if self.u.is_class(o) => {
                return Err("the qualifier of a class member without its prefix");
            }
            _ => Type::NoPrefix,
        };
        Ok(self.u.intern_type(t))
    }

    /// The type `<repeated>[T]` of `s` if it is a repeated parameter of a method, as its
    /// method's type states it (dotty's info of a repeated parameter); `Err` for one of a
    /// polymorphic method.
    fn repeated_param(&mut self, s: SymbolId) -> Result<Option<TypeId>, &'static str> {
        let sym = self.u.symbol(s);
        let (name, Some(owner)) = (sym.name.clone(), sym.owner) else { return Ok(None) };
        if sym.kind != Kind::Param || self.u.symbol(owner).kind != Kind::Def {
            return Ok(None);
        }
        let (mut t, mut poly) = (self.u.info(owner), false);
        while let Type::Poly { result, .. } | Type::Method { result, .. } = self.u.types().get(t).clone() {
            if let Type::Method { params, .. } = self.u.types().get(t).clone()
                && let Some(&(_, p)) = params.iter().find(|(n, _)| *n == name)
            {
                let repeated = self.u.class_symbol(p).is_some_and(|c| self.u.symbol(c).name == "<repeated>");
                return match (repeated, poly) {
                    (true, true) => Err("memberType of a repeated parameter of a polymorphic method"),
                    _ => Ok(repeated.then_some(p)),
                };
            }
            poly |= matches!(self.u.types().get(t), Type::Poly { .. });
            t = result;
        }
        Ok(None)
    }
}
