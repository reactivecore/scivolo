// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/Splicer.scala
//! The macro implementation a splice calls: a method of a static object, possibly inherited,
//! and whether its result is a context function of the `Quotes` the splice applies.

use super::Inlining;
use crate::tpd::{List, TNodeId, Tree as T};
use tasty::tags::INLINE;
use tasty::{Kind, SymbolId, Type};

impl Inlining<'_> {
    /// The macro implementation reference `t` names and the static object it is called on: the
    /// method's owner, or an object inheriting it (dotty's `interpretedStaticMethodCall`).
    pub fn macro_method(&mut self, t: TNodeId) -> Option<(SymbolId, SymbolId)> {
        let m = self.tpd.sym(t)?;
        let s = self.u.symbol(m);
        if s.kind != Kind::Def || s.flags.has(INLINE) {
            return None;
        }
        let owner = s.owner?;
        if self.is_static_object(owner) {
            return Some((m, owner));
        }
        let prefix = match *self.u.types().get(self.tpd.tpe(t)) {
            Type::Sym(Some(p), _) | Type::TermRef(p, _) => p,
            _ => return None,
        };
        let object = match *self.u.types().get(prefix) {
            Type::Sym(_, v) if self.u.is_module(v) => self.u.module_class(v)?,
            Type::This(c) => self.u.class_symbol(c)?,
            _ => return None,
        };
        (self.is_static_object(object) && self.u.derives_from(object, owner)).then_some((m, object))
    }

    /// The function `fun` applied to the splice's `Quotes` `quotes` in `args` calls `apply` on,
    /// if it is a context function of them, not taken by name.
    pub fn context_function(&mut self, fun: TNodeId, args: List, quotes: SymbolId) -> Option<TNodeId> {
        let T::Select { qualifier, .. } = *self.tpd.tree(fun) else { return None };
        let args = self.tpd.items(args);
        let apply = self.tpd.sym(fun)?;
        let class = self.u.symbol(apply).owner?;
        let is_quotes =
            |a: &TNodeId| self.tpd.sym(*a) == Some(quotes) && matches!(self.tpd.tree(*a), T::Ident { .. });
        let context =
            self.u.symbol(apply).name == "apply" && self.u.full_name(class) == "scala.ContextFunction1";
        if !context || !matches!(args, [a] if is_quotes(a)) {
            return None;
        }
        let f = self.u.widen(self.tpd.tpe(qualifier));
        let f = self.u.dealias(f);
        let types = self.u.types();
        let by_name = matches!(types.get(f), Type::Applied(_, targs)
            if targs.first().is_some_and(|&a| matches!(types.get(a), Type::ByName(_))));
        (!by_name).then_some(qualifier)
    }

    fn is_static_object(&self, c: SymbolId) -> bool {
        self.u.is_module_class(c) && self.u.is_static_symbol(c)
    }
}
