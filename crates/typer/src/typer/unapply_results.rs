// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! The result types of extractors (dotty's `UnapplyArgs`, `extractorMemberType`): product
//! selectors, `get` of an `isEmpty`/`get` result, Boolean results, generic tuples.

use super::Typer;
use super::dependent_params::applied_result;
use crate::symbols::Completion;
use crate::tpd::TNodeId;
use tasty::{Binder, SymbolId, Type, TypeId, Universe};

impl Typer<'_> {
    /// Result `t` of extractor `unapp` of parameter `param` of the method type of binder
    /// `binder`, applied to a value of type `own`, an intersection `(?1 : S) & T` with the
    /// skolem of an unstable `own` as `S & T` simplified, type variables instantiated.
    pub fn applied_unapply_result(
        &mut self,
        unapp: TNodeId,
        t: TypeId,
        (binder, param): (Binder, &str),
        own: TypeId,
    ) -> TypeId {
        let method = self.tpd.sym(self.tpd.meth_part(unapp));
        let names = [param.to_owned()];
        let applied = applied_result(self.u, t, (binder, &names), method, &[own]);
        let skolem = |u: &Universe, x| match *u.types().get(x) {
            Type::Skolem(_, w) => Some(w),
            _ => None,
        };
        let result = self.u.map_type(applied, &mut |u, part| {
            let Type::And(a, b) = *u.types().get(part) else { return None };
            match (skolem(u, a), skolem(u, b)) {
                (Some(w), None) => Some(u.glb(w, b)),
                (None, Some(w)) => Some(u.glb(a, w)),
                _ => None,
            }
        });
        self.zonk(result)
    }

    /// The expected types of `n` patterns of an extractor with result type `result` (dotty's
    /// `UnapplyArgs.argTypes` for `unapply`).
    pub fn unapply_arg_types(&mut self, result: TypeId, n: usize) -> Result<Vec<TypeId>, &'static str> {
        let result = self.strip_named_tuple(result);
        let types = if n > 0
            && self.derives_from_named(result, "scala.Product")
            && self.product_selectors(result)?.len() == n
        {
            self.product_selectors(result)?
        } else if let Some(get) = self.get_match(result)? {
            let sels = if n > 1 && !self.derives_from_named(get, "scala.collection.Seq") {
                let stripped = self.strip_named_tuple(get);
                self.product_selectors(stripped)?
            } else {
                Vec::new()
            };
            if sels.len() == n { sels } else { vec![get] }
        } else if self.derives_from_named(result, "scala.Boolean") {
            Vec::new()
        } else if self.derives_from_named(result, "scala.NonEmptyTuple") {
            let widened = self.u.widen(result);
            self.tuple_elements_within(widened, usize::MAX).unwrap_or_default()
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
            Type::Bounds(..) => return Err("extractor member typed by a wildcard argument"),
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
