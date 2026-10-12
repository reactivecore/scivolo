//! Arguments the call's type refers to by skolems: their proxies typed by those skolems and the
//! expansion cast to the call's type (dotty's `callValueSkolemss` and the sealing in
//! `InlineCall.expand`).

use super::{Inlining, Unsupported};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{Span, TName};
use tasty::{Binder, SymbolId, Type, TypeId, Universe};

/// The skolem, if any, that type `actual` of an application has where the result type `result`
/// of its method type of binder `binder` refers to parameter `index`, also as symbol `param`;
/// `None` if `result` does not refer to it.
pub fn call_skolem(
    u: &mut Universe,
    (result, actual): (TypeId, TypeId),
    binder: Binder,
    (index, param): (usize, SymbolId),
) -> Option<Option<TypeId>> {
    let method = u.symbol(param).owner;
    let param_index = |u: &Universe, t: TypeId| match *u.types().get(t) {
        Type::Param(b, i) if b == binder => Some(i as usize),
        Type::Sym(None, s) if s == param => Some(index),
        Type::Sym(None, s) if u.symbol(s).kind == tasty::Kind::Param && u.symbol(s).owner == method => {
            Some(usize::MAX)
        }
        _ => None,
    };
    let mut pattern = Vec::new();
    u.map_type(result, &mut |u, part| {
        let p = param_index(u, part);
        pattern.push(p);
        p.map(|_| part)
    });
    if !pattern.contains(&Some(index)) {
        return None;
    }
    let (mut k, mut found) = (0, None);
    u.map_type(actual, &mut |u, part| {
        let p = *pattern.get(k)?;
        k += 1;
        if p == Some(index) && matches!(u.types().get(part), Type::Skolem(..)) {
            found = Some(part);
        }
        p.map(|_| part)
    });
    Some(found)
}

impl Inlining<'_> {
    /// `expr.$asInstanceOf$[to]`, at `expr`'s span made synthetic (dotty's `cast`).
    pub fn cast(&mut self, expr: TNodeId, to: TypeId) -> Result<TNodeId, Unsupported> {
        let span = self.tpd.span(expr).to_synthetic();
        let any = self.u.builtins().any;
        let found = self.u.decl(any, "$asInstanceOf$").ok().and_then(|d| d.first().copied());
        let m = found.ok_or((span, "a cast without Any.$asInstanceOf$"))?;
        let prefix = self.tpd.tpe(expr);
        let select_tpe = self.u.intern_type(Type::Sym(Some(prefix), m));
        let name = TName { name: self.names.intern("$asInstanceOf$"), is_type: false };
        let fun = self.tpd.node(T::Select { qualifier: expr, name }, span, Some(select_tpe), Some(m));
        let tpt = self.tpd.node(T::TypeTree { inferred: false }, span, Some(to), None);
        let args = self.tpd.list(&[tpt]);
        Ok(self.tpd.node(T::TypeApply { fun, args }, span, Some(to), None))
    }

    /// `expansion` cast to `tpe` if `tpe` refers to a skolem the type of one of `bindings` is.
    pub fn sealed(
        &mut self,
        bindings: &[SymbolId],
        expansion: TNodeId,
        tpe: TypeId,
    ) -> Result<TNodeId, Unsupported> {
        let mut skolems = Vec::new();
        for &b in bindings {
            let info = self.u.info(b);
            let info = match *self.u.types().get(info) {
                Type::ByName(r) => r,
                _ => info,
            };
            if let Type::Skolem(binder, _) = *self.u.types().get(info) {
                skolems.push(binder);
            }
        }
        let mut mentioned = false;
        self.u.map_type(tpe, &mut |u, part| {
            mentioned |= matches!(*u.types().get(part), Type::Skolem(b, _) if skolems.contains(&b));
            None
        });
        if !mentioned {
            return Ok(expansion);
        }
        let span: Span = self.tpd.span(expansion);
        let cast = self.cast(expansion, tpe)?;
        self.tpd.get_mut(cast).span = span;
        Ok(cast)
    }
}
