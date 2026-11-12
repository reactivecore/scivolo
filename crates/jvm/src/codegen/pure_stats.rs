// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/PureStats.scala
//   compiler/src/dotty/tools/dotc/ast/TreeInfo.scala
//! The statements dotty's `PureStats` drops, as far as `isPureExpr` finds them pure paths.

use super::Gen;
use tasty::Kind;
use tasty::tags::{LAZY, MUTABLE, STATIC};
use typer::tpd::{TNodeId, Tree};

impl Gen<'_, '_> {
    /// Determines whether statement `t` has no effect: a literal, `this`, `super`, a parameter
    /// or an immutable non-lazy local value, a final Java instance field of such, possibly
    /// typed, or a block of such.
    pub fn is_pure_stat(&self, t: TNodeId) -> bool {
        match *self.l.tpd.tree(t) {
            Tree::Literal { .. } | Tree::This { .. } | Tree::Super { .. } => true,
            Tree::Ident { .. } => self.l.tpd.sym(t).is_some_and(|s| self.is_pure_local(s)),
            Tree::Select { qualifier, .. } => {
                self.l.tpd.sym(t).is_some_and(|s| self.is_final_java_field(s)) && self.is_pure_stat(qualifier)
            }
            Tree::Typed { expr, .. } => self.is_pure_stat(expr),
            Tree::Block { stats, expr } => {
                self.l.tpd.items(stats).iter().all(|&s| self.is_pure_stat(s)) && self.is_pure_stat(expr)
            }
            _ => false,
        }
    }

    /// Whether `s` is a parameter or an immutable non-lazy value local to a term, not by name.
    fn is_pure_local(&self, s: tasty::SymbolId) -> bool {
        let sym = self.l.u.symbol(s);
        let local = sym.owner.is_some_and(|o| self.l.u.symbol(o).kind.is_term());
        let by_name = self.l.u.known_info(s).is_some_and(|t| self.l.u.by_name(t).is_some());
        let value = sym.kind == Kind::Param || sym.kind == Kind::Val && !sym.flags.has(LAZY);
        local && value && !sym.flags.has(MUTABLE) && !by_name
    }

    /// Whether `s` is a final instance field of a Java class.
    fn is_final_java_field(&self, s: tasty::SymbolId) -> bool {
        let u = &self.l.u;
        let sym = u.symbol(s);
        let in_statics = sym.owner.is_some_and(|o| u.is_java_statics(o));
        let instance = !in_statics && !sym.flags.has(STATIC);
        let java = u.is_java_defined(s) || u.is_java_field(s);
        sym.kind == Kind::Val && !u.is_module(s) && !sym.flags.has(MUTABLE) && java && instance
    }
}
