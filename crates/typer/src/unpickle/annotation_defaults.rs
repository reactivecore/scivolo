// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreeUnpickler.scala
//! The arguments of Java annotation constructor calls read from TASTy (dotty's
//! `fixArgsToJavaAnnotConstructor`).

use super::Unpickler;
use crate::tpd::{TNodeId, Tree as T};
use names::nme;
use syntax::ast::TName;
use syntax::names::Name;
use tasty::TypeId;

impl Unpickler<'_> {
    /// Gives the arguments `args` of `fun(args)` as dotty reads them when `fun` is `new A` of a
    /// class file's annotation class `A`, whose elements may have changed since the pickling: all
    /// named in the order of `A`'s elements, an element left out as `name = _`. Positional
    /// arguments of older TASTy are named by their position. Arguments that do not match the
    /// elements, and the arguments of other calls, stay as they are.
    pub fn java_annotation_args(&mut self, fun: TNodeId, args: Vec<TNodeId>) -> Vec<TNodeId> {
        let part = self.tpd.meth_part(fun);
        let T::Select { qualifier, .. } = *self.tpd.tree(part) else { return args };
        let class = self
            .tpd
            .sym(part)
            .filter(|&m| self.u.symbol(m).name == nme::CONSTRUCTOR)
            .and_then(|m| self.u.symbol(m).owner);
        if !matches!(self.tpd.tree(qualifier), T::New { .. })
            || !class.is_some_and(|c| self.u.classfile_retention(c).is_some())
        {
            return args;
        }
        let (info, _) = self.method_type(fun);
        let Some(params) = self.u.method(info).map(|m| m.params.to_vec()) else { return args };
        if args.len() > params.len() {
            return args;
        }

        let elements: Vec<_> = params.iter().map(|&(n, _)| self.names.intern(self.u.name(n))).collect();
        let mut named = Vec::new();
        for (i, &a) in args.iter().enumerate() {
            match *self.tpd.tree(a) {
                T::NamedArg { name, .. } => named.push((name.name, a)),
                _ => named.push((elements[i], self.named(elements[i], a))),
            }
        }
        let positional = args.iter().any(|&a| !matches!(self.tpd.tree(a), T::NamedArg { .. }));
        if positional && named.iter().zip(&elements).any(|((n, _), e)| n != e) {
            return args;
        }

        let mut out = Vec::new();
        for (&e, &(_, tpe)) in elements.iter().zip(&params) {
            match named.iter().position(|&(n, _)| n == e) {
                Some(i) => out.push(named.remove(i).1),
                None => out.push(self.underscore(e, tpe)),
            }
        }
        if named.is_empty() { out } else { args }
    }

    /// `name = arg`.
    fn named(&mut self, name: Name, arg: TNodeId) -> TNodeId {
        let tpe = self.tpd.get(arg).tpe;
        self.bare(T::NamedArg { name: TName { name, is_type: false }, arg }, tpe, None)
    }

    /// `name = _` of type `tpe` (dotty's `Underscore`), the default of a Java annotation element.
    fn underscore(&mut self, name: Name, tpe: TypeId) -> TNodeId {
        let wildcard = TName { name: self.names.intern(nme::WILDCARD), is_type: false };
        let arg = self.bare(T::Ident { name: wildcard }, Some(tpe), None);
        self.named(name, arg)
    }
}
