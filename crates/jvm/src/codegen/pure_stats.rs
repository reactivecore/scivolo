// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/PureStats.scala
//   compiler/src/dotty/tools/dotc/ast/TreeInfo.scala
//! The statements dotty's `PureStats` drops, as far as `isPureExpr` finds them pure paths.

use super::Gen;
use super::body::Body;
use lower::Result;
use names::nme;
use tasty::tags::{LAZY, MUTABLE, STATIC};
use tasty::{Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

impl Gen<'_, '_> {
    /// Generates statement `t` of a block and drops its value; generates nothing for a pure `t`.
    pub fn block_stat(&mut self, b: &mut Body, t: TNodeId) -> Result<()> {
        match self.is_pure_stat(t) {
            true => Ok(()),
            false => self.stat(b, t),
        }
    }

    /// Determines whether `m` is a primary constructor, whose statements are a class body's.
    pub fn is_primary_constructor(&mut self, m: Option<SymbolId>) -> bool {
        let Some(m) = m.filter(|&m| self.l.u.is_constructor(m)) else { return false };
        let Some(class) = self.l.u.symbol(m).owner else { return false };
        self.l.u.decls_named(class, nme::CONSTRUCTOR).first() == Some(&m)
    }

    /// Whether statement `t` has no effect: a literal, `this`, `super`, a parameter or an
    /// immutable non-lazy local value, a final Java instance field of such, possibly typed, or a
    /// block of such.
    fn is_pure_stat(&self, t: TNodeId) -> bool {
        match *self.l.tpd.tree(t) {
            Tree::Literal { .. } | Tree::This { .. } | Tree::Super { .. } => true,
            Tree::Ident { .. } => {
                self.l.tpd.sym(t).is_some_and(|s| self.is_pure_local(s) || self.is_stable_field(s))
            }
            Tree::Select { qualifier, .. } => {
                let field = |s| self.is_final_java_field(s) || self.is_stable_field(s);
                self.l.tpd.sym(t).is_some_and(field) && self.is_pure_stat(qualifier)
            }
            Tree::Typed { expr, .. } => self.is_pure_stat(expr),
            Tree::Block { stats, expr } => {
                self.l.tpd.items(stats).iter().all(|&s| self.is_pure_stat(s)) && self.is_pure_stat(expr)
            }
            _ => false,
        }
    }

    /// Whether `s` is a parameter or an immutable non-lazy value local to a term, not by name.
    fn is_pure_local(&self, s: SymbolId) -> bool {
        let sym = self.l.u.symbol(s);
        let local = sym.owner.is_some_and(|o| self.l.u.symbol(o).kind.is_term());
        let by_name = self.l.u.known_info(s).is_some_and(|t| self.l.u.by_name(t).is_some());
        let value = sym.kind == Kind::Param || sym.kind == Kind::Val && !sym.flags.has(LAZY);
        local && value && !sym.flags.has(MUTABLE) && !by_name
    }

    /// Whether `s` is an immutable non-lazy field without a getter.
    fn is_stable_field(&self, s: SymbolId) -> bool {
        let flags = self.l.u.symbol(s).flags;
        self.l.is_getterless_field(s) && !flags.has(MUTABLE) && !flags.has(LAZY)
    }

    /// Whether `s` is a final instance field of a Java class.
    fn is_final_java_field(&self, s: SymbolId) -> bool {
        let u = &self.l.u;
        let sym = u.symbol(s);
        let in_statics = sym.owner.is_some_and(|o| u.is_java_statics(o));
        let instance = !in_statics && !sym.flags.has(STATIC);
        let java = u.is_java_defined(s) || u.is_java_field(s);
        sym.kind == Kind::Val && !u.is_module(s) && !sym.flags.has(MUTABLE) && java && instance
    }
}
