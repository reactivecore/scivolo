// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/staging/HealType.scala
//! The given `Type[T]` of each type `T` of level 0 a quote refers to (dotty's `tryHeal`), which
//! the lowering's `PickleQuotes` makes the quote's tags.

use super::Typer;
use crate::ctx::Ctx;
use crate::defn::type_ref;
use crate::tpd::{TNodeId, Tree as T};
use rustc_hash::FxHashSet;
use tasty::{Kind, SymbolId, Type};

impl Typer<'_> {
    /// Records the tags of quote `quote` of body `body`, typed at level 0 in `ctx`: for each abstract
    /// type local to a term and defined outside the quote that a type in the body outside its
    /// splices refers to, the stable path a search for its `Type[T]` finds.
    pub fn record_quote_tags(&mut self, quote: TNodeId, body: TNodeId, ctx: Ctx) {
        let (mut todo, mut defined, mut used) = (vec![body], FxHashSet::default(), Vec::new());
        while let Some(t) = todo.pop() {
            let node = self.tpd.get(t);
            if matches!(node.tree, T::ValDef { .. } | T::DefDef { .. } | T::TypeDef { .. } | T::Bind { .. }) {
                defined.extend(node.sym);
            }
            if let Some(tpe) = node.tpe {
                self.u.map_type(tpe, &mut |u, x| {
                    if let Type::Sym(None, s) = *u.types().get(x)
                        && !used.contains(&s)
                    {
                        used.push(s);
                    }
                    None
                });
            }
            if !self.is_splice(t) {
                todo.extend(self.tpd.children(t).iter().rev());
            }
        }
        let Some(class) = self.resolve_full_name("scala.quoted.Type", true) else { return };
        let mut tags = Vec::new();
        for s in used {
            if defined.contains(&s) || !self.is_level_zero_type(s) {
                continue;
            }
            let arg = self.u.intern_type(Type::Sym(None, s));
            let tycon = type_ref(self.u, class);
            let pt = self.u.intern_type(Type::Applied(tycon, vec![arg]));
            let Ok(tag) = self.implicit_arg(pt, self.tpd.span(body), ctx, 0) else { continue };
            let path = self.tpd.tpe(tag);
            if matches!(*self.u.types().get(path), Type::Sym(_, t) if self.u.symbol(t).kind.is_term()) {
                tags.push((s, path));
            }
        }
        if !tags.is_empty() {
            self.tpd.set_quote_tags(quote, tags);
        }
    }

    /// The body of `quote(body).apply(quotes)`.
    pub fn quote_body(&self, applied: TNodeId) -> Option<TNodeId> {
        let T::Apply { fun, .. } = *self.tpd.tree(applied) else { return None };
        let T::Select { qualifier, .. } = *self.tpd.tree(fun) else { return None };
        let T::Apply { args, .. } = *self.tpd.tree(qualifier) else { return None };
        self.tpd.items(args).first().copied()
    }

    /// Whether `t` calls `scala.quoted.runtime.Expr.splice`.
    fn is_splice(&self, t: TNodeId) -> bool {
        let T::Apply { fun, .. } = *self.tpd.tree(t) else { return false };
        let m = self.tpd.meth_part(fun);
        self.tpd.sym(m).is_some_and(|s| self.is_staging_method(s) && self.u.symbol(s).name == "splice")
    }

    /// Whether `s` is an abstract type local to a term.
    fn is_level_zero_type(&mut self, s: SymbolId) -> bool {
        let sym = self.u.symbol(s);
        matches!(sym.kind, Kind::Type | Kind::TypeParam)
            && sym.owner.is_some_and(|o| self.u.symbol(o).kind.is_term())
            && self.u.alias_of(s).is_none()
    }
}
