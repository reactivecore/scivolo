//! Proxies of the `this` of the classes enclosing an inline method's class that its body refers
//! to, bound to the paths the call's prefix makes them (dotty's `computeThisBindings`).

use super::Unsupported;
use super::expand::Expansion;
use syntax::ast::Span;
use tasty::{Kind, SymbolId, Type, TypeId};

impl Expansion<'_, '_> {
    /// The classes `outers`, enclosing class `cls`, with the proxies of their `this` for a call
    /// prefix of type `qt`, innermost first, bound from index `at` on: each to the stable path the
    /// prefix makes it if the prefix is `pure` (dotty's `ref(info)`); a path through outer
    /// fields (`outerSelect`) is refused.
    pub fn bind_outer_this(
        &mut self,
        cls: SymbolId,
        qt: TypeId,
        outers: &[SymbolId],
        (pure, at): (bool, usize),
    ) -> Result<Vec<(SymbolId, SymbolId)>, Unsupported> {
        let mut sorted = outers.to_vec();
        sorted.sort_by_key(|&c| std::cmp::Reverse(self.nesting_level(c)));
        let point = Span::at(self.i.tpd.span(self.call).point());
        let mut out = Vec::new();
        for c in sorted {
            let class_type = crate::defn::type_ref(self.i.u, c);
            let this = self.i.u.intern_type(Type::This(class_type));
            let seen = self.i.u.as_seen_from(this, qt, cls);
            let stable = match *self.i.u.types().get(seen) {
                Type::Sym(_, s) => self.i.is_stable_member(s) && self.i.is_singleton_path(seen),
                _ => false,
            };
            if !stable || !pure && out.is_empty() {
                return Err(self.err("inline body referring to an outer this"));
            }
            let proxy = self.this_proxy_symbol(c, seen);
            let rhs =
                self.i.singleton(seen, point).ok_or(self.err("inline body referring to an outer this"))?;
            self.bind_proxy(proxy, rhs, point, at + out.len());
            out.push((c, proxy));
        }
        Ok(out)
    }

    /// How many classes enclose class `c`, itself included.
    fn nesting_level(&self, c: SymbolId) -> usize {
        let mut level = 0;
        let mut o = Some(c);
        while let Some(x) = o {
            level += usize::from(self.i.u.symbol(x).kind == Kind::Class);
            o = self.i.u.symbol(x).owner;
        }
        level
    }
}
