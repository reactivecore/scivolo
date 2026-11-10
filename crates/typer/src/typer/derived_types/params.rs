// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/Desugar.scala
//! Copies of constructor and enum type parameters (dotty's `DerivedFromParamTree` and
//! `derivedTypeParamWithVariance`).

use super::super::Typer;
use super::DerivedTypeTree;
use crate::ctx::Level;
use crate::defn::type_ref;
use syntax::ast::flags::{PARAM, PARAM_ACCESSOR};
use tasty::{Kind, SymbolId, Type, TypeId};

impl DerivedTypeTree {
    /// The info of parameter `name` of the source constructor, a type parameter if `is_type`,
    /// its references to the constructor's other parameters made to the same-named parameters or
    /// accessors visible here (dotty's `DerivedFromParamTree.derivedTree`): the `A` and `x: A`
    /// that `copy[A](x: A = this.x)` of `case class C[A](x: A)` copies.
    pub fn param_type(&self, t: &mut Typer<'_>, name: &str, is_type: bool) -> Option<TypeId> {
        let ctor = self.source_constructor(t)?;
        match is_type {
            true => t.enter_leading_type_params(ctor),
            false => t.complete(ctor),
        }
        let decls = t.u.symbol(ctor).decls.clone();
        let original =
            decls.into_iter().find(|&p| t.u.symbol(p).name == name && t.u.is_type_symbol(p) == is_type)?;
        self.relocated_info(t, original, ctor)
    }

    /// The info of type parameter `name` of the enum class whose case class is being typed,
    /// relocated like `param_type`'s: the `T` of `case C(x: T)` in `enum E[T]`.
    pub fn enum_param_type(&self, t: &mut Typer<'_>, name: &str) -> Option<TypeId> {
        let case = t.enclosing_class(self.ctx.owner)?;
        let module = t.u.symbol(case).owner?;
        let enum_class = t.u.companion_class(module)?;
        t.complete(enum_class);
        let tparams = t.u.type_params(enum_class);
        let original = tparams.into_iter().find(|&p| t.u.symbol(p).name == name)?;
        self.relocated_info(t, original, enum_class)
    }

    /// The constructor whose parameters the class members and companion methods here copy: the
    /// enclosing class's primary one, or, in an object, its companion class's.
    fn source_constructor(&self, t: &mut Typer<'_>) -> Option<SymbolId> {
        let cls = t.enclosing_class(self.ctx.owner)?;
        if let Some(&ctor) = t.syms.primary_ctors.get(&cls)
            && !t.u.symbol(cls).flags.has(tasty::tags::OBJECT)
        {
            return Some(ctor);
        }
        t.companion_constructor(cls)
    }

    /// The info of `original`, its references to other symbols of `owner` made to the same-named
    /// parameters or accessors visible here.
    fn relocated_info(&self, t: &mut Typer<'_>, original: SymbolId, owner: SymbolId) -> Option<TypeId> {
        let info = match t.syms.repeated_params.get(&original) {
            Some(&written) => written,
            None => t.u.info(original),
        };
        let mut siblings = Vec::new();
        t.u.map_type(info, &mut |u, p| {
            if let Type::Sym(_, s) = *u.types().get(p)
                && u.symbol(s).owner == Some(owner)
            {
                siblings.push(s);
            }
            None
        });

        let mut replacements = Vec::new();
        for s in siblings {
            let (n, ty) = (t.u.symbol(s).name.clone(), t.u.is_type_symbol(s));
            let local = self.param_or_accessor(t, &n, ty)?;
            replacements.push((s, local));
        }
        Some(t.u.map_type(info, &mut |u, p| match *u.types().get(p) {
            Type::Sym(_, s) => replacements.iter().find(|&&(o, _)| o == s).map(|&(_, l)| l),
            _ => None,
        }))
    }

    /// A reference to the parameter or parameter accessor named `name` visible here.
    fn param_or_accessor(&self, t: &mut Typer<'_>, name: &str, is_type: bool) -> Option<TypeId> {
        let mut level = Some(self.ctx.level);
        while let Some(l) = level {
            let candidates: Vec<SymbolId> = match t.levels.get(l) {
                Level::Scope(entries) => entries.iter().filter(|(n, _)| n == name).map(|&(_, s)| s).collect(),
                &Level::Members(owner) if t.u.symbol(owner).kind == Kind::Class => {
                    t.u.symbol(owner).decls.iter().copied().filter(|&d| t.u.symbol(d).name == name).collect()
                }
                _ => Vec::new(),
            };
            let found = candidates.into_iter().find(|&s| {
                t.u.is_type_symbol(s) == is_type
                    && t.syms.flags.get(&s).is_some_and(|f| f.is(PARAM) || f.is(PARAM_ACCESSOR))
            });
            if let Some(s) = found {
                let owner = t.u.symbol(s).owner.filter(|&o| t.u.symbol(o).kind == Kind::Class);
                let prefix = owner.map(|o| {
                    let class = type_ref(t.u, o);
                    t.u.intern_type(Type::This(class))
                });
                return Some(t.u.intern_type(Type::Sym(prefix, s)));
            }
            level = t.levels.outer(l);
        }
        None
    }
}
