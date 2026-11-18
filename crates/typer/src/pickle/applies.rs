// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreePickler.scala
//! Applications in the shape of dotty's trees (the `Apply` case of `pickleTree`): a
//! parameterless method without an empty application, the constructor of a generic class
//! with its type arguments, a Java annotation's without its omitted arguments, a signature
//! polymorphic call with its method type.

use super::{Refusal, TreePickler};
use crate::tpd::{List, TNodeId, Tree as T};
use names::nme;
use tasty::tags::{APPLY, APPLYsigpoly, TYPEAPPLY};
use tasty::{Kind, Type, TypeId};

impl TreePickler<'_> {
    /// Application `fun(args)`, in a file in dotty's shape.
    pub fn apply_(&mut self, fun: TNodeId, args: List) -> Result<(), Refusal> {
        let ctor = self.tpd.sym(fun).filter(|&c| self.u.is_constructor(c));
        if ctor.and_then(|c| self.u.symbol(c).owner).is_some_and(|o| self.u.classfile_retention(o).is_some())
        {
            let args: Vec<TNodeId> =
                self.tpd.items(args).iter().copied().filter(|&a| !self.is_default_placeholder(a)).collect();
            return self.with_length(APPLY, &mut |p| {
                p.tree(fun)?;
                args.iter().try_for_each(|&a| p.tree(a))
            });
        }
        if let Some(instance) =
            self.tpd.sym(fun).filter(|&s| self.u.original_signature_polymorphic(s).is_some())
        {
            let info = self.u.info(instance);
            let args = self.tpd.items(args).to_vec();
            return self.with_length(APPLYsigpoly, &mut |p| {
                p.tree(fun)?;
                let at = p.buf.buf.current_addr();
                p.new_type(info, true, at)?;
                args.iter().try_for_each(|&a| p.tree(a))
            });
        }
        if self.file.is_none() {
            return self.node(APPLY, &[fun], &[args]);
        }
        let parameterless = self.tpd.sym(fun).is_some_and(|s| {
            let info = self.u.info(s);
            self.u.symbol(s).kind == Kind::Def
                && !matches!(self.u.ty(info), Type::Method { .. } | Type::Poly { .. })
        });
        if parameterless && self.tpd.items(args).is_empty() {
            return self.tree(fun);
        }
        let Some(targs) = self.constructor_type_args(fun) else { return self.node(APPLY, &[fun], &[args]) };
        let args = self.tpd.items(args).to_vec();
        let span = self.tpd.span(fun);
        let written = self.new_tpts(fun);
        self.with_length(APPLY, &mut |p| {
            p.with_length(TYPEAPPLY, &mut |p| {
                p.tree(fun)?;
                p.with_typed_tpts(written.clone(), &mut |p| {
                    targs.iter().try_for_each(|&t| {
                        p.synthetic_here(span, None);
                        p.ty(t, false)
                    })
                })
            })?;
            args.iter().try_for_each(|&a| p.tree(a))
        })
    }

    /// Type application `fun[args]`, in a file named arguments and those inferred for the
    /// parameters not named by position (dotty's `normalizeTypeArgs`).
    pub fn type_apply(&mut self, fun: TNodeId, args: List) -> Result<(), Refusal> {
        let is_named =
            |p: &Self, l: List| p.tpd.items(l).iter().any(|&a| matches!(p.tpd.tree(a), T::NamedArg { .. }));
        let (tycon, named, others) = match *self.tpd.tree(fun) {
            T::TypeApply { fun: inner, args: named } if is_named(self, named) => (inner, named, Some(args)),
            _ if is_named(self, args) => (fun, args, None),
            _ => {
                let items = self.tpd.items(args).to_vec();
                self.mark_inferred_type_args(fun, &items);
                let written = self.new_tpts(fun);
                return self.with_typed_tpts(written, &mut |p| p.node(TYPEAPPLY, &[fun], &[args]));
            }
        };
        if self.file.is_none() {
            return Err("named type arguments");
        }
        let info = self.u.widen(self.tpd.tpe(tycon));
        let Some(params) = self.u.poly(info).map(|p| p.params.to_vec()) else {
            return Err("named type arguments of a method that is not polymorphic");
        };
        let named = self.tpd.items(named).to_vec();
        let mut others = others.map(|o| self.tpd.items(o).to_vec()).unwrap_or_default().into_iter();
        let mut ordered = Vec::new();
        for &(name, _) in &params {
            let given = named.iter().find_map(|&a| match *self.tpd.tree(a) {
                T::NamedArg { name: n, arg } if self.names.text(n.name) == self.u.name(name) => Some(arg),
                _ => None,
            });
            ordered.push(given.or_else(|| others.next()).ok_or("named type arguments missing a parameter")?);
        }
        self.mark_inferred_type_args(tycon, &ordered);
        self.with_length(TYPEAPPLY, &mut |p| {
            p.tree(tycon)?;
            ordered.iter().try_for_each(|&a| p.tree(a))
        })
    }

    /// The type arguments of `new C[Ts]` that the constructor selected by `fun` of generic class
    /// `C` lacks, the upper bounds of `C`'s type parameters where `C` is raw.
    fn constructor_type_args(&mut self, fun: TNodeId) -> Option<Vec<TypeId>> {
        let T::Select { qualifier, .. } = *self.tpd.tree(fun) else { return None };
        let T::New { tpt } = *self.tpd.tree(qualifier) else { return None };
        let ctor = self.tpd.sym(fun).filter(|&c| self.u.is_constructor(c))?;
        let info = self.u.info(ctor);
        let params = self.u.poly(info).map(|p| p.params.to_vec())?;
        let tpe = self.tpd.tpe(tpt);
        match self.u.applied(tpe) {
            Some((_, targs)) => Some(targs.to_vec()),
            None => Some(params.iter().map(|&(_, b)| self.upper_bound(b)).collect()),
        }
    }

    /// The annotated type trees of `C[Ts]` if `fun` selects a constructor of `new C[Ts]`.
    fn new_tpts(&self, fun: TNodeId) -> Vec<TNodeId> {
        match *self.tpd.tree(fun) {
            T::Select { qualifier, .. } if matches!(self.tpd.tree(qualifier), T::New { .. }) => {
                self.annotated_tpts(qualifier)
            }
            _ => Vec::new(),
        }
    }

    fn upper_bound(&self, bounds: TypeId) -> TypeId {
        match *self.u.ty(bounds) {
            Type::Bounds(_, hi) | Type::Alias(hi) => hi,
            _ => bounds,
        }
    }

    /// Whether `arg` is `name = _`, an omitted argument of a Java annotation with a default value
    /// (dotty's `pickleTree` skips it, #19951).
    fn is_default_placeholder(&self, arg: TNodeId) -> bool {
        let T::NamedArg { arg, .. } = *self.tpd.tree(arg) else { return false };
        matches!(*self.tpd.tree(arg), T::Ident { name } if self.names.text(name.name) == nme::WILDCARD)
            && self.tpd.sym(arg).is_none()
    }
}
