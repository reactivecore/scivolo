//! Extractors over typed trees (dotty's `TreeInfo`).

use super::{TNodeId, Tpd, Tree};

impl Tpd {
    /// `f` of `f[T](a)(b)`: `t` without its applications and type applications (dotty's
    /// `methPart`, not looking into blocks).
    pub fn meth_part(&self, mut t: TNodeId) -> TNodeId {
        while let Tree::Apply { fun, .. } | Tree::TypeApply { fun, .. } = *self.tree(t) {
            t = fun;
        }
        t
    }

    /// `f[T]` of `f[T](a)(b)`: `t` without its applications (dotty's `stripApply`).
    pub fn strip_apply(&self, mut t: TNodeId) -> TNodeId {
        while let Tree::Apply { fun, .. } = *self.tree(t) {
            t = fun;
        }
        t
    }

    /// `e` of `e: T: U` (dotty's `stripTyped`).
    pub fn strip_typed(&self, mut t: TNodeId) -> TNodeId {
        while let Tree::Typed { expr, .. } = *self.tree(t) {
            t = expr;
        }
        t
    }
}
