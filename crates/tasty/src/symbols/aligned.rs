// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Denotations.scala
//! The result types of two matching members with their type parameters aligned.

use super::{SymbolId, Universe};
use crate::types::{Type, TypeId};

impl Universe {
    /// The result types `a` and `b` of members `sa` and `sb`, `sb`'s type parameters, symbols or
    /// those of its leading polymorphic clause, replaced by `sa`'s.
    pub fn aligned_results(
        &mut self,
        (sa, a): (SymbolId, TypeId),
        (sb, b): (SymbolId, TypeId),
    ) -> (TypeId, TypeId) {
        let (ra, mut rb) = (self.result_type(a), self.result_type(b));
        let mut own = None;
        if let (Type::Poly { binder, params, .. }, Type::Poly { binder: b2, params: p2, .. }) =
            (self.ty(a).clone(), self.ty(b).clone())
            && self.params_of(params).len() == self.params_of(p2).len()
        {
            let p2 = self.params_of(p2).to_vec();
            rb = self.with_binder(rb, (b2, &p2), binder);
            own = Some(binder);
        }
        let (ta, tb) = (self.type_params(sa), self.type_params(sb));
        if ta.len() == tb.len() && ta != tb {
            let mut by_sym = false;
            self.map_type(ra, &mut |u, t| {
                by_sym |= matches!(*u.ty(t), Type::Sym(None, s) if ta.contains(&s));
                None
            });
            let own = own.filter(|_| by_sym);
            rb = self.map_type(rb, &mut |u, t| {
                let i = match *u.ty(t) {
                    Type::Sym(None, s) => tb.iter().position(|&p| p == s)?,
                    Type::Param(b, i) if Some(b) == own => i as usize,
                    _ => return None,
                };
                ta.get(i).map(|&p| u.intern_type(Type::Sym(None, p)))
            });
        }
        (ra, rb)
    }
}
