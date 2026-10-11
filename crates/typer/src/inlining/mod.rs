//! dotty's `inlining` phase: each call of an inline method that is not itself in an inline method,
//! replaced by an `Inlined` tree of the method's body with the arguments bound (`Inlining`,
//! `Inlines.inlineCall`).

mod constants;
mod copy;
mod expand;
mod proxies;
mod purity;
mod retype;
mod unused;

use crate::tpd::{TNodeId, Tpd, Tree as T, map_children};
use rustc_hash::FxHashMap;
use syntax::ast::Span;
use syntax::names::{FreshNames, Names};
use tasty::tags::INLINE;
use tasty::{Kind, SymbolId, Type, Universe};

/// How many inline calls may expand one inside another (dotty's `-Xmax-inlines`).
const MAX_INLINES: u32 = 32;

/// Where and why a unit's inlining is outside the subset.
pub type Unsupported = (Span, &'static str);

pub struct Inlining<'a> {
    pub u: &'a mut Universe,
    pub tpd: &'a mut Tpd,
    pub names: &'a mut Names,
    pub fresh: &'a mut FreshNames,
    /// The definition of each inline method of the unit with a body.
    bodies: FxHashMap<SymbolId, TNodeId>,
    /// How many inline calls are being expanded, one inside another.
    depth: u32,
}

impl AsMut<Tpd> for Inlining<'_> {
    fn as_mut(&mut self) -> &mut Tpd {
        self.tpd
    }
}

/// Expands the inline calls of unit `root`, typed into `tpd`, in place.
pub fn run(
    u: &mut Universe,
    tpd: &mut Tpd,
    names: &mut Names,
    fresh: &mut FreshNames,
    root: TNodeId,
) -> Result<(), Unsupported> {
    let mut i = Inlining { u, tpd, names, fresh, bodies: FxHashMap::default(), depth: 0 };
    i.collect_bodies(root);
    let owner = i.u.root();
    let new_root = i.transform(root, owner)?;
    debug_assert_eq!(new_root, root);
    Ok(())
}

impl Inlining<'_> {
    fn collect_bodies(&mut self, t: TNodeId) {
        if let T::DefDef { rhs: Some(_), .. } = self.tpd.tree(t)
            && let Some(s) = self.tpd.sym(t)
            && self.u.symbol(s).flags.has(INLINE)
        {
            self.bodies.insert(s, t);
        }
        for c in self.tpd.children(t) {
            self.collect_bodies(c);
        }
    }

    /// `t`, owned by `owner`, with the inline calls in it expanded; inline definitions stay.
    fn transform(&mut self, t: TNodeId, owner: SymbolId) -> Result<TNodeId, Unsupported> {
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
        let mut failed = None;
        map_children(self, t, &mut |i, c| match i.transform(c, inner) {
            Ok(n) => n,
            Err(e) => {
                failed.get_or_insert(e);
                c
            }
        });
        if let Some(e) = failed {
            return Err(e);
        }
        match self.inline_method(t)? {
            Some(def) => self.inline_call(t, def, owner),
            None => Ok(t),
        }
    }

    /// The definition of the inline method `t` calls with all its arguments, if it is one
    /// (dotty's `needsInlining`).
    fn inline_method(&mut self, t: TNodeId) -> Result<Option<TNodeId>, Unsupported> {
        let Some(tpe) = self.tpd.get(t).tpe else { return Ok(None) };
        let widened = self.u.widen(tpe);
        if self.is_type_tree(t)
            || matches!(self.u.types().get(widened), Type::Method { .. } | Type::Poly { .. })
        {
            return Ok(None);
        }
        let Some(meth) = self.method_part(t).and_then(|f| self.tpd.sym(f)) else { return Ok(None) };
        let sym = self.u.symbol(meth);
        if sym.kind != Kind::Def || !sym.flags.has(INLINE) {
            return Ok(None);
        }
        match self.bodies.get(&meth) {
            Some(&def) => Ok(Some(def)),
            None => Err((self.tpd.span(t), "call of an inline method defined elsewhere")),
        }
    }

    /// The reference `t` applies, through its applications and type applications.
    pub fn method_part(&self, t: TNodeId) -> Option<TNodeId> {
        match *self.tpd.tree(t) {
            T::Apply { fun, .. } | T::TypeApply { fun, .. } => self.method_part(fun),
            T::Ident { .. } | T::Select { .. } => Some(t),
            _ => None,
        }
    }

    fn is_type_tree(&self, t: TNodeId) -> bool {
        matches!(
            self.tpd.tree(t),
            T::TypeTree { .. }
                | T::SingletonTypeTree { .. }
                | T::ByNameTypeTree { .. }
                | T::AppliedTypeTree { .. }
                | T::TypeBoundsTree { .. }
                | T::LambdaTypeTree { .. }
                | T::Annotated { .. }
        ) || matches!(*self.tpd.tree(t), T::Ident { name } | T::Select { name, .. } if name.is_type)
    }

    /// Call `call`, owned by `owner`, of the inline method defined by `def`, expanded.
    fn inline_call(&mut self, call: TNodeId, def: TNodeId, owner: SymbolId) -> Result<TNodeId, Unsupported> {
        let span = self.tpd.span(call);
        if self.depth >= MAX_INLINES {
            return Err((span, "maximal number of successive inlines exceeded"));
        }
        self.depth += 1;
        let expanded = expand::Expansion::new(self, call, owner).inlined(def);
        self.depth -= 1;
        let (bindings, expansion) = expanded?;
        if let Some(inner) = self.inline_ref_in(expansion) {
            return Err((self.tpd.span(inner), "inline call in an inline expansion"));
        }
        let tpe = self.tpd.tpe(expansion);
        if bindings.iter().any(|&b| self.tpd.sym(b).is_some_and(|s| self.mentions(tpe, s))) {
            return Err((span, "inline expansion of a type referring to a binding"));
        }
        let bindings = self.tpd.list(&bindings);
        Ok(self.tpd.node(T::Inlined { call: Some(call), bindings, expansion }, span, Some(tpe), None))
    }

    /// A reference to an inline method in `t`, outside the arguments inlined from outside.
    fn inline_ref_in(&self, t: TNodeId) -> Option<TNodeId> {
        match self.tpd.tree(t) {
            T::Inlined { call: None, .. } => return None,
            T::Ident { .. } | T::Select { .. } => {
                let inline = self.tpd.sym(t).is_some_and(|s| {
                    let s = self.u.symbol(s);
                    s.kind == Kind::Def && s.flags.has(INLINE)
                });
                if inline {
                    return Some(t);
                }
            }
            _ => {}
        }
        self.tpd.children(t).into_iter().find_map(|c| self.inline_ref_in(c))
    }

    /// Whether type `t` refers to symbol `s`.
    pub fn mentions(&mut self, t: tasty::TypeId, s: SymbolId) -> bool {
        let mut found = false;
        self.u.map_type(t, &mut |u, part| {
            found |= matches!(*u.types().get(part), Type::Sym(_, x) if x == s);
            None
        });
        found
    }
}
