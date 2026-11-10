// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/EtaExpansion.scala
//! One argument lifted into a local value named after its parameter (dotty's `liftArgs`).

use super::super::Typer;
use super::Lifter;
use crate::tpd::{TNodeId, Tree as T};
use tasty::TypeId;

impl Lifter {
    /// Gives argument `arg` of parameter `(name, p)` lifted unless it needs not or the parameter
    /// is by-name or inline, named after the parameter unless the name has a `$`; a named
    /// argument or a sequence argument `xs*` keeps its form around the lifted expression.
    pub fn arg(&mut self, t: &mut Typer, arg: TNodeId, (name, p): &(String, TypeId)) -> TNodeId {
        if t.u.by_name(*p).is_some() || t.types().is_inline_param_type(*p) {
            return arg;
        }
        let prefix = if name.contains('$') { "" } else { name.as_str() };
        match t.tpd.tree(arg).clone() {
            T::NamedArg { name, arg: inner } => {
                let lifted = self.lift(t, inner, prefix);
                let (span, tpe) = (t.tpd.span(arg), t.tpd.get(arg).tpe);
                if lifted == inner {
                    arg
                } else {
                    t.tpd.node(T::NamedArg { name, arg: lifted }, span, tpe, None)
                }
            }
            T::Typed { expr, tpt } if t.tpd.get(arg).tpe.is_some_and(|x| t.repeated_elem(x).is_some()) => {
                t.type_seq_literal(expr);
                let lifted = self.lift(t, expr, prefix);
                let (span, tpe) = (t.tpd.span(arg), t.tpd.get(arg).tpe);
                if lifted == expr { arg } else { t.tpd.node(T::Typed { expr: lifted, tpt }, span, tpe, None) }
            }
            _ => self.lift(t, arg, prefix),
        }
    }
}
