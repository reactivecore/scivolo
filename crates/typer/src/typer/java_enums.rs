// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Definitions.scala
//   compiler/src/dotty/tools/dotc/typer/Checking.scala
//! Enums extending `java.lang.Enum`: its constructor called without the name and ordinal the
//! lowering passes (dotty's `JavaEnumClass` with its primary constructor's parameters removed),
//! and the definitions dotty rejects refused; enum classes with their own `ordinal`.

use super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{Span, TName};
use tasty::{SymbolId, Type};

impl Typer<'_> {
    /// Whether `c` is `java.lang.Enum`.
    pub fn is_java_enum_class(&self, c: SymbolId) -> bool {
        self.u.full_name(c) == "java.lang.Enum"
    }

    /// Whether class `c` derives from `java.lang.Enum`.
    pub fn derives_from_java_enum(&mut self, c: SymbolId) -> bool {
        self.u.linearization(c).into_iter().any(|b| self.is_java_enum_class(b))
    }

    /// `new java.lang.Enum[E]()` for parent type tree `parent` at `span`.
    pub fn java_enum_constr_call(&mut self, parent: TNodeId, psym: SymbolId, span: Span) -> Option<TNodeId> {
        let tpe = self.tpd.tpe(parent);
        let ctor = self.u.decl(psym, "<init>").ok()?.into_iter().next()?;
        let Type::Applied(_, args) = self.u.types().get(tpe).clone() else { return None };
        let new = self.tpd.node(T::New { tpt: parent }, span, Some(tpe), None);
        let name = TName { name: self.name("<init>"), is_type: false };
        let ctor_tpe = self.ref_type(Some(tpe), ctor);
        let select = self.tpd.node(T::Select { qualifier: new, name }, span, Some(ctor_tpe), Some(ctor));
        let targs: Vec<TNodeId> = args.iter().map(|&a| self.type_tree(a, span.start_pos(), false)).collect();
        let binder = self.syms.fresh_binder();
        let method =
            Type::Method { binder, params: Vec::new(), result: tpe, implicit: false, contextual: false };
        let method = self.u.intern_type(method);
        let fun = self.build().type_apply(select, &targs, method, span);
        Some(self.build().apply(fun, &[], tpe, span))
    }

    /// Whether enum class `c` declares or inherits a concrete `ordinal` other than `Enum`'s or
    /// `java.lang.Enum`'s.
    pub fn defines_enum_ordinal(&mut self, c: SymbolId) -> bool {
        let flags = self.u.symbol(c).flags;
        if !flags.has(tasty::tags::ENUM) || flags.has(tasty::tags::CASE) {
            return false;
        }
        self.class_members(c, "ordinal").into_iter().any(|m| {
            let owner = self.u.symbol(m).owner.map(|o| self.u.full_name(o));
            !self.u.symbol(m).flags.has(tasty::tags::SYNTHETIC)
                && owner.as_deref() != Some("scala.reflect.Enum")
                && owner.as_deref() != Some("java.lang.Enum")
                && self.is_concrete(m)
        })
    }

    /// What dotty rejects in class `cls` deriving from `java.lang.Enum`, or the subset leaves out:
    /// a class case, an enum outside a static scope, an explicit call of its constructor;
    /// nothing in javalib's sources, whose classes extend their `_Enum`.
    pub fn java_enum_problem(&mut self, cls: SymbolId, parents: &[TNodeId]) -> Option<&'static str> {
        use tasty::tags::{CASE, ENUM, TRAIT};
        if !self.derives_from_java_enum(cls) || self.u.javalib_sources() {
            return None;
        }
        let flags = self.u.symbol(cls).flags;
        if !flags.has(ENUM) && !flags.has(TRAIT) && !self.u.symbol(cls).name.starts_with("$anon") {
            return Some("class extending java.lang.Enum");
        }
        if flags.has(ENUM) && flags.has(CASE) {
            return Some("parameterized case of an enum extending java.lang.Enum");
        }
        if flags.has(ENUM) && !self.u.is_static_symbol(cls) {
            return Some("enum extending java.lang.Enum outside a static scope");
        }
        if flags.has(ENUM) && !flags.has(CASE) {
            let this = crate::defn::type_ref(self.u, cls);
            let base = self.u.linearization(cls).into_iter().find(|&b| self.is_java_enum_class(b));
            let arg = base.and_then(|b| self.u.base_type(this, b)).map(|t| self.u.types().get(t).clone());
            let own = matches!(arg, Some(Type::Applied(_, ref args)) if args.len() == 1
                && self.u.class_symbol(args[0]) == Some(cls));
            if !own || !self.u.type_params(cls).is_empty() {
                return Some("enum extending java.lang.Enum of another type or with type parameters");
            }
        }
        let explicit = parents.iter().any(|&p| {
            let called = self.tpd.sym(self.tpd.meth_part(p)).and_then(|c| self.u.symbol(c).owner);
            called.is_some_and(|c| self.is_java_enum_class(c))
                && matches!(*self.tpd.tree(p), T::Apply { args, .. } if !self.tpd.items(args).is_empty())
        });
        explicit.then_some("explicit call of the constructor of java.lang.Enum")
    }
}
