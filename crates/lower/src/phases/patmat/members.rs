// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/PatternMatcher.scala
//! The member selections plans make: `qual.m` typed as seen from `qual`, the selectors `_1`…
//! of a product, the test `!x.isEmpty`.

use super::Translator;
use crate::Result;
use syntax::ast::Span;
use tasty::{Kind, SymbolId, Type, TypeId};
use typer::tpd::TNodeId;

impl Translator<'_, '_> {
    /// `!scrutinee.isEmpty`.
    pub fn non_empty(&mut self, scrutinee: TNodeId) -> TNodeId {
        let tpe = self.l.tpd.tpe(scrutinee);
        let is_empty = self.member_named(tpe, "isEmpty").expect("`isEmpty`");
        let empty = self.select_member(scrutinee, is_empty);
        let boolean = self.l.tpd.tpe(empty);
        let not = self.member_named(boolean, "unary_!").expect("`unary_!`");
        self.select_member(empty, not)
    }

    /// `qual.m`, typed as `m` is seen from `qual`'s type, a parameterless method's result.
    pub fn select_member(&mut self, qual: TNodeId, m: SymbolId) -> TNodeId {
        let prefix = self.l.tpd.tpe(qual);
        let info = self.l.u.member_info(prefix, m);
        let tpe = match self.l.u.types().get(info).clone() {
            Type::Method { params, result, .. } if params.is_empty() => result,
            _ => info,
        };
        let select = self.l.select(qual, m, Span::NONE);
        self.l.tpd.get_mut(select).tpe = Some(tpe);
        self.l.positioned(select)
    }

    /// The selectors `_1`, `_2`… of type `tpe`, none unless it is a product if `product` (dotty's
    /// `productSelectors`, `productArity`).
    pub fn product_selectors(&mut self, tpe: TypeId, product: bool) -> Result<Vec<SymbolId>> {
        if product {
            let product_class = self.l.product()?;
            if !self.l.class_of(tpe).is_some_and(|c| self.l.u.derives_from(c, product_class)) {
                return Ok(Vec::new());
            }
        }
        let mut out = Vec::new();
        while let Some(s) = self.member_named(tpe, &format!("_{}", out.len() + 1)) {
            out.push(s);
        }
        Ok(out)
    }

    /// The parameterless method or the value of type `tpe`'s class or its bases named `name`.
    pub fn member_named(&mut self, tpe: TypeId, name: &str) -> Option<SymbolId> {
        let class = self.l.class_of(tpe)?;
        for base in self.l.u.linearization(class) {
            let found = self.l.u.decl(base, name).unwrap_or_default();
            let member = |m: &SymbolId| match self.l.u.symbol(*m).kind {
                Kind::Val | Kind::Param => true,
                Kind::Def => {
                    let info = self.l.u.info(*m);
                    !matches!(self.l.u.types().get(info), Type::Method { params, .. } if !params.is_empty())
                }
                _ => false,
            };
            if let Some(m) = found.into_iter().find(member) {
                return Some(m);
            }
        }
        None
    }
}
