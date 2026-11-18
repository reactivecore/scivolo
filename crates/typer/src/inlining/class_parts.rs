// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Contexts.scala
//! The scopes of the parts of a local class that do not see its members.

use super::Inlining;
use super::local_implicits::Scope;
use crate::tpd::{TNodeId, Tree as T};
use names::nme;
use tasty::tags::PARAMsetter;
use tasty::{Kind, Symbol, SymbolId};

impl Inlining<'_> {
    /// Runs `retype` on child `c` of tree `t`, in the scopes of a parent of class body `t` or of
    /// the self-constructor call `t` if it is one (dotty's `superCallContext`,
    /// `thisCallArgContext`).
    pub fn in_class_part<R>(&mut self, (t, c): (TNodeId, TNodeId), retype: impl FnOnce(&mut Self) -> R) -> R {
        let scopes = match *self.tpd.tree(t) {
            T::Template { constr, parents, .. } if self.tpd.items(parents).contains(&c) => {
                self.super_call_scopes(constr)
            }
            T::Apply { .. } => self.this_call_scopes(t),
            _ => None,
        };
        let Some(scopes) = scopes else { return retype(self) };

        let outer = std::mem::replace(&mut self.scopes, scopes);
        let result = retype(self);
        self.scopes = outer;
        result
    }

    /// The scopes of a parent of the class whose body is being retyped, with primary constructor
    /// `constr`: its members give way to its implicit parameters.
    fn super_call_scopes(&mut self, constr: TNodeId) -> Option<Vec<Scope>> {
        let &Scope::Class(cls) = self.scopes.last()? else { return None };
        let param = |s: &Symbol| s.kind == Kind::Param || s.kind == Kind::Val && s.flags.has(PARAMsetter);
        let decls = self.u.decls(cls).unwrap_or_default().into_iter();
        let params = decls.filter(|&d| param(self.u.symbol(d)) && self.u.symbol(d).is_implicit()).collect();
        let constr = self.tpd.sym(constr).unwrap_or(cls);

        let mut scopes = self.scopes[..self.scopes.len() - 1].to_vec();
        scopes.push(Scope::SuperCall { constr, params });
        Some(scopes)
    }

    /// The scopes of the arguments of `t` if it is a `this(...)` call of the secondary constructor
    /// being retyped: those outside its class, then the ones of the constructor's body. The
    /// members of the class and the imports of its body are out of scope.
    fn this_call_scopes(&self, t: TNodeId) -> Option<Vec<Scope>> {
        let constr = self.owner.filter(|&o| self.u.is_constructor(o))?;
        if !self.is_this_constructor_call(t) {
            return None;
        }
        let cls = self.enclosing_class?;
        let at = self.scopes.iter().rposition(|s| is_class(s, cls))?;

        let mut scopes = self.scopes[..at].to_vec();
        scopes.push(Scope::SuperCall { constr, params: Vec::new() });
        scopes.extend(self.scopes[at + 1..].iter().filter(|s| !matches!(s, Scope::Import(_))).cloned());
        Some(scopes)
    }

    fn is_this_constructor_call(&self, t: TNodeId) -> bool {
        let callee = self.tpd.meth_part(t);
        matches!(*self.tpd.tree(callee), T::Select { qualifier, .. }
            if matches!(self.tpd.tree(qualifier), T::This { .. }))
            && self.tpd.sym(callee).is_some_and(|s| self.u.symbol(s).name == nme::CONSTRUCTOR)
    }
}

fn is_class(scope: &Scope, cls: SymbolId) -> bool {
    matches!(scope, Scope::Class(c) if *c == cls)
}
