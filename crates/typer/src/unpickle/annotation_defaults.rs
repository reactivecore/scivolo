// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Calls of Java annotation constructors that leave out defaulted elements (dotty's
//! `isJavaAnnotConstr` in `tryDefault`).

use super::{Refusal, Unpickler};
use crate::tpd::{TNodeId, Tree as T};
use names::nme;
use tasty::{NameId, SymbolId, TypeId};

const MISSING: Refusal = "application with missing arguments in a library inline body";

impl Unpickler<'_> {
    /// Types `new A(args)` of method `meth` with parameters `params` as its class `A`, when `A` is
    /// a class file's annotation class, `args` are named arguments for some of its elements and
    /// each element they leave out has a default, as javalib's `@Deprecated` pickled against a
    /// JDK whose `Deprecated` has no elements. Refuses any other call with missing arguments.
    pub fn defaulted_java_annotation(
        &mut self,
        fun: TNodeId,
        meth: Option<SymbolId>,
        params: &[(NameId, TypeId)],
        args: &[TNodeId],
    ) -> Result<TypeId, Refusal> {
        let class =
            meth.filter(|&m| self.u.symbol(m).name == nme::CONSTRUCTOR).and_then(|m| self.u.symbol(m).owner);
        let Some(class) = class.filter(|&c| self.u.classfile_retention(c).is_some()) else {
            return Err(MISSING);
        };
        let part = self.tpd.meth_part(fun);
        let T::Select { qualifier, .. } = *self.tpd.tree(part) else { return Err(MISSING) };
        if !matches!(self.tpd.tree(qualifier), T::New { .. }) {
            return Err(MISSING);
        }

        let mut given = Vec::new();
        for &a in args {
            let T::NamedArg { name, .. } = *self.tpd.tree(a) else { return Err(MISSING) };
            given.push(name.name);
        }
        let mut elements = Vec::new();
        for &(n, _) in params {
            let text = self.u.name(n).to_owned();
            let element = self.names.intern(&text);
            if !given.contains(&element) && !self.u.has_annotation_default(class, &text) {
                return Err(MISSING);
            }
            elements.push(element);
        }
        match given.iter().all(|g| elements.contains(g)) {
            true => Ok(self.tpd.tpe(qualifier)),
            false => Err(MISSING),
        }
    }
}
