//! The result types of extractors (dotty's `UnapplyArgs`, `extractorMemberType`): product
//! selectors, `get` of an `isEmpty`/`get` result, Boolean results.

use super::Typer;
use crate::symbols::Completion;
use tasty::{SymbolId, Type, TypeId};

impl Typer<'_> {
    /// The expected types of `n` patterns of an extractor with result type `result` (dotty's
    /// `UnapplyArgs.argTypes` for `unapply`).
    pub fn unapply_arg_types(&mut self, result: TypeId, n: usize) -> Result<Vec<TypeId>, &'static str> {
        let types = if n > 0
            && self.derives_from_named(result, "scala.Product")
            && self.product_selectors(result)?.len() == n
        {
            self.product_selectors(result)?
        } else if let Some(get) = self.get_match(result)? {
            let sels = if n > 1 && !self.derives_from_named(get, "scala.collection.Seq") {
                self.product_selectors(get)?
            } else {
                Vec::new()
            };
            if sels.len() == n { sels } else { vec![get] }
        } else if self.derives_from_named(result, "scala.Boolean") {
            Vec::new()
        } else {
            return Err("extractor result type");
        };
        if types.len() != n {
            return Err("wrong number of patterns");
        }
        Ok(types)
    }

    /// The types of the product selectors `_1`, `_2`, ... of `t`.
    pub fn product_selectors(&mut self, t: TypeId) -> Result<Vec<TypeId>, &'static str> {
        let mut out = Vec::new();
        while let Some(s) = self.extractor_member(t, &format!("_{}", out.len() + 1))? {
            out.push(s);
        }
        Ok(out)
    }

    /// The type of `get` if `t` has it and a `Boolean` member `isEmpty`.
    pub fn get_match(&mut self, t: TypeId) -> Result<Option<TypeId>, &'static str> {
        let empty = self.extractor_member(t, "isEmpty")?;
        if !empty.is_some_and(|e| self.derives_from_named(e, "scala.Boolean")) {
            return Ok(None);
        }
        self.extractor_member(t, "get")
    }

    /// The type of parameterless member `name` of `t` (dotty's `extractorMemberType`).
    pub fn extractor_member(&mut self, t: TypeId, name: &str) -> Result<Option<TypeId>, &'static str> {
        let Some(found) = self.members_found(t, name, false) else { return Ok(None) };
        let [sym] = found.syms[..] else { return Err("overloaded extractor member") };
        let tpe = self.ref_type(Some(t), sym);
        let info = self.method_info(tpe).unwrap_or(tpe);
        Ok(match *self.u.types().get(info) {
            Type::Method { .. } | Type::Poly { .. } => None,
            Type::ByName(r) => Some(r),
            _ => Some(info),
        })
    }

    pub fn derives_from_named(&mut self, t: TypeId, class: &str) -> bool {
        let base = self.resolve_full_name(class, true);
        let t = self.u.widen(t);
        match (self.u.class_symbol(t), base) {
            (Some(c), Some(b)) => {
                self.complete_bases(c);
                self.u.derives_from(c, b)
            }
            _ => false,
        }
    }

    /// Completes class `c` and the unit's classes among its base classes.
    fn complete_bases(&mut self, c: SymbolId) {
        loop {
            let pending: Vec<SymbolId> = std::iter::once(c)
                .chain(self.u.linearization(c))
                .filter(|s| matches!(self.syms.completions.get(s), Some(Completion::Pending { .. })))
                .collect();
            if pending.is_empty() {
                return;
            }
            pending.into_iter().for_each(|s| self.complete(s));
        }
    }
}
