// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreePickler.scala
//! Applications in the shape of dotty's trees (the `Apply` case of `pickleTree`): a
//! parameterless method without an empty application, the constructor of a generic class
//! with its type arguments.

use super::{Refusal, TreePickler};
use crate::tpd::{List, TNodeId, Tree as T};
use tasty::tags::{APPLY, TYPEAPPLY};
use tasty::{Kind, Type, TypeId};

impl TreePickler<'_> {
    /// Application `fun(args)`, in a file in dotty's shape.
    pub fn apply_(&mut self, fun: TNodeId, args: List) -> Result<(), Refusal> {
        if self.file.is_none() {
            return self.node(APPLY, &[fun], &[args]);
        }
        let parameterless = self.tpd.sym(fun).is_some_and(|s| {
            let info = self.u.info(s);
            self.u.symbol(s).kind == Kind::Def
                && !matches!(self.u.types().get(info), Type::Method { .. } | Type::Poly { .. })
        });
        if parameterless && self.tpd.items(args).is_empty() {
            return self.tree(fun);
        }
        let Some(targs) = self.constructor_type_args(fun) else { return self.node(APPLY, &[fun], &[args]) };
        let args = self.tpd.items(args).to_vec();
        self.with_length(APPLY, &mut |p| {
            p.with_length(TYPEAPPLY, &mut |p| {
                p.tree(fun)?;
                targs.iter().try_for_each(|&t| p.ty(t, false))
            })?;
            args.iter().try_for_each(|&a| p.tree(a))
        })
    }

    /// Type application `fun[args]`, in a file without named arguments, which dotty's trees
    /// have by position.
    pub fn type_apply(&mut self, fun: TNodeId, args: List) -> Result<(), Refusal> {
        let named = self.tpd.items(args).iter().any(|&a| matches!(self.tpd.tree(a), T::NamedArg { .. }));
        if named && self.file.is_some() {
            return Err("named type arguments");
        }
        self.node(TYPEAPPLY, &[fun], &[args])
    }

    /// The type arguments of `new C[Ts]` that the constructor selected by `fun` of generic class
    /// `C` lacks, the upper bounds of `C`'s type parameters where `C` is raw.
    fn constructor_type_args(&mut self, fun: TNodeId) -> Option<Vec<TypeId>> {
        let T::Select { qualifier, .. } = *self.tpd.tree(fun) else { return None };
        let T::New { tpt } = *self.tpd.tree(qualifier) else { return None };
        let ctor = self.tpd.sym(fun).filter(|&c| self.u.is_constructor(c))?;
        let info = self.u.info(ctor);
        let Type::Poly { params, .. } = self.u.types().get(info).clone() else { return None };
        let tpe = self.tpd.tpe(tpt);
        match self.u.types().get(tpe) {
            Type::Applied(_, targs) => Some(targs.clone()),
            _ => Some(params.iter().map(|&(_, b)| self.upper_bound(b)).collect()),
        }
    }

    fn upper_bound(&self, bounds: TypeId) -> TypeId {
        match *self.u.types().get(bounds) {
            Type::Bounds(_, hi) | Type::Alias(hi) => hi,
            _ => bounds,
        }
    }
}
