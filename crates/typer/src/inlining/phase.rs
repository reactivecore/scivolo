// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/Inlines.scala
//   compiler/src/dotty/tools/dotc/transform/Inlining.scala
//! dotty's `Inlining` walk: the calls that expand and the definitions of the methods they call.

use super::{Inlining, Unsupported};
use crate::tpd::{TNodeId, Tree as T, map_children};
use tasty::tags::{INLINE, TRANSPARENT};
use tasty::{Kind, SymbolId, Type};

impl Inlining<'_> {
    /// Expands the inline calls of unit `root` in place.
    pub fn run(&mut self, root: TNodeId) -> Result<(), Unsupported> {
        self.collect_bodies(root);
        let owner = self.u.root();
        let new_root = self.transform(root, owner)?;
        debug_assert_eq!(new_root, root);
        Ok(())
    }

    /// `t`, owned by `owner`, with the inline calls in it expanded; inline definitions stay.
    pub fn transform(&mut self, t: TNodeId, owner: SymbolId) -> Result<TNodeId, Unsupported> {
        let inner = match self.tpd.tree(t) {
            T::ValDef { .. } | T::DefDef { .. } | T::TypeDef { .. } | T::Template { .. } => {
                match self.tpd.sym(t) {
                    Some(s) if self.u.symbol(s).flags.has(INLINE) => return Ok(t),
                    Some(s) => s,
                    None => owner,
                }
            }
            _ => owner,
        };
        self.expand_macro_annotations(t)?;
        let typer_ctx = self.typer_inlined_ctx(t);
        let call = self.traced_call(t);
        let (outer, outer_ctx) = (self.typer_inlined, self.typer_ctx);
        if call.is_some() {
            self.typer_inlined = outer.or(Some(self.tpd.span(t)));
            self.typer_ctx = if outer.is_some() { outer_ctx } else { typer_ctx };
        }
        let level = self.staging_level(t);
        self.staging += level;
        let (parents, constr) = match *self.tpd.tree(t) {
            T::Template { constr, parents, .. } => (self.tpd.items(parents).to_vec(), self.tpd.sym(constr)),
            _ => (Vec::new(), None),
        };
        let mut failed = None;
        let changed = map_children(self, t, &mut |i, c| match Some(c) == call {
            true => c,
            false => {
                i.transform(c, constr.filter(|_| parents.contains(&c)).unwrap_or(inner)).unwrap_or_else(|e| {
                    failed.get_or_insert(e);
                    c
                })
            }
        });
        (self.typer_inlined, self.typer_ctx) = (outer, outer_ctx);
        self.staging -= level;
        if let Some(e) = failed {
            return Err(e);
        }
        self.fold_constants(t);
        if changed {
            self.retype_inlined(t);
        }
        if let T::UnApply { fun, implicits, patterns } = *self.tpd.tree(t) {
            let fun = self.inlined_unapply_fun(fun, owner)?;
            self.tpd.get_mut(t).tree = T::UnApply { fun, implicits, patterns };
        }
        match self.inline_method(t)? {
            Some(def) => self.inline_call(t, def, owner),
            None => Ok(t),
        }
    }

    /// Records the definition of each inline method with a body in `t`.
    pub fn collect_bodies(&mut self, t: TNodeId) {
        if let T::DefDef { rhs: Some(_), .. } = self.tpd.tree(t)
            && let Some(s) = self.tpd.sym(t)
            && self.u.symbol(s).flags.has(INLINE)
        {
            self.bodies.insert(s, t);
        }
        self.note_inits(t);
        for c in self.tpd.children(t) {
            self.collect_bodies(c);
        }
    }

    /// The definition of the inline method `t` calls with all its arguments, if it is one
    /// (dotty's `needsInlining`).
    pub fn inline_method(&mut self, t: TNodeId) -> Result<Option<TNodeId>, Unsupported> {
        let Some(tpe) = self.tpd.get(t).tpe else { return Ok(None) };
        if self.staging > 0 {
            return Ok(None);
        }
        let widened = self.u.widen(tpe);
        if self.tpd.is_type_tree(t) || matches!(self.u.ty(widened), Type::Method { .. } | Type::Poly { .. }) {
            return Ok(None);
        }
        let Some(meth) = self.method_part(t).and_then(|f| self.tpd.sym(f)) else { return Ok(None) };
        let sym = self.u.symbol(meth);
        if sym.kind != Kind::Def
            || !sym.flags.has(INLINE)
            || self.transparent_only && !self.force_inline && !sym.flags.has(TRANSPARENT)
        {
            return Ok(None);
        }
        if let Some(&def) = self.bodies.get(&meth) {
            return Ok(Some(def));
        }
        if self.u.is_deferred(meth) {
            return Err((self.tpd.span(t), "call of a deferred inline method"));
        }
        let lent = (&mut *self.u, &mut *self.tpd, &mut *self.names, &mut *self.fresh);
        let def = match self.host.inline_def(lent, meth).map_err(|what| (self.tpd.span(t), what))? {
            Some(def) => def,
            None if self.u.symbol(meth).tree.is_none() => {
                return Err((self.tpd.span(t), "call of an inline method defined elsewhere"));
            }
            None => crate::unpickle::inline_def(self.u, self.tpd, self.names, meth)
                .map_err(|what| (self.tpd.span(t), what))?,
        };
        self.bodies.insert(meth, def);
        Ok(Some(def))
    }

    /// The reference `t` applies, through its applications and type applications.
    pub fn method_part(&self, t: TNodeId) -> Option<TNodeId> {
        match *self.tpd.tree(t) {
            T::Apply { fun, .. } | T::TypeApply { fun, .. } => self.method_part(fun),
            T::Ident { .. } | T::Select { .. } => Some(t),
            _ => None,
        }
    }
}
