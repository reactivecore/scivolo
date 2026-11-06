// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Deferred givens: `given T = deferred` in a trait, abstract, and its implementation found by
//! implicit search in each class that inherits it first (dotty's `implementDeferredGivens`).

use super::Typer;
use crate::ctx::{Ctx, Level};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::flags::{self as f, FlagSet};
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::tags::{GIVEN, HASDEFAULT};
use tasty::{Kind, SymbolId, Type};

impl Typer<'_> {
    /// Whether `rhs` of a value definition with flags `flags` in `ctx` is `deferred` of a given
    /// in a trait.
    pub fn is_deferred_given_rhs(&self, rhs: Option<NodeId>, flags: FlagSet, ctx: Ctx) -> bool {
        let named = |t: &Tree| matches!(*t, Tree::Ident { name } | Tree::Select { name, .. } if self.text(name.name) == "deferred");
        flags.is(f::GIVEN) && self.u.is_trait(ctx.owner) && rhs.is_some_and(|r| named(self.ast.tree(r)))
    }

    /// Whether `s` is a deferred given, not a parameter.
    pub fn is_deferred_given(&self, s: SymbolId) -> bool {
        let sym = self.u.symbol(s);
        sym.kind == Kind::Val && sym.flags.has(GIVEN) && sym.flags.has(HASDEFAULT) && self.u.is_deferred(s)
    }

    /// The definitions implementing the deferred givens class `cls` inherits and its superclass
    /// does not, searched in `ctx`, outside the class, and the class's given parameters.
    pub fn deferred_given_impls(&mut self, cls: SymbolId, ctx: Ctx, cctx: Ctx, span: Span) -> Result<Vec<TNodeId>, &'static str> {
        if self.u.is_trait(cls) {
            return Ok(Vec::new());
        }
        let class = crate::defn::type_ref(self.u, cls);
        let this = self.u.intern_type(Type::This(class));
        let parents = self.u.parents(cls);
        let superclass = parents.first().and_then(|&p| self.u.class_symbol(p)).filter(|&s| !self.u.is_trait(s));
        let inherited = superclass.map(|s| self.u.linearization(s)).unwrap_or_default();
        let mut impls = Vec::new();
        for base in self.u.linearization(cls).into_iter().skip(1).filter(|b| !inherited.contains(b)) {
            for dcl in self.u.decls(base).unwrap_or_default() {
                let name = self.u.symbol(dcl).name.clone();
                if !self.is_deferred_given(dcl) || self.u.resolve(this, &name, false) != Some(dcl) {
                    continue;
                }
                impls.push(self.deferred_given_impl(cls, dcl, (ctx, cctx), span)?);
            }
        }
        Ok(impls)
    }

    fn deferred_given_impl(&mut self, cls: SymbolId, dcl: SymbolId, (ctx, cctx): (Ctx, Ctx), span: Span) -> Result<TNodeId, &'static str> {
        let class = crate::defn::type_ref(self.u, cls);
        let this = self.u.intern_type(Type::This(class));
        let owner = self.u.symbol(dcl).owner.ok_or("deferred given")?;
        let info = self.u.info(dcl);
        let target = self.u.as_seen_from(info, this, owner);
        let accessors: Vec<(String, SymbolId)> = (self.u.decls(cls).unwrap_or_default().into_iter())
            .filter(|&a| self.flags_of(a).is_all_of(f::PARAM_ACCESSOR.or(f::GIVEN)))
            .map(|a| (self.u.symbol(a).name.clone(), a))
            .collect();
        let ids: Vec<SymbolId> = accessors.iter().map(|&(_, a)| a).collect();
        let level = self.levels.push(Some(ctx.level), Level::Scope(accessors));
        let search = Ctx { level, owner: cls, ..ctx };
        let at = span.start_pos().to_synthetic();
        let rhs = self.implicit_arg(target, at, search, 0)?;
        self.anchor_params(rhs, &ids, cls, cctx);
        let name = self.u.symbol(dcl).name.clone();
        let flags = self.syms.flags.get(&dcl).ok_or("deferred given of a library trait")?;
        let flags = flags.and_not(f::HAS_DEFAULT.or(f::DEFERRED)).or(f::FINAL).or(f::OVERRIDE);
        let sym = self.syms.define(self.u, &name, Kind::Val, cls, flags);
        self.u.set_info(sym, target);
        let tpt = self.type_tree(target, at, false);
        let name = TName { name: self.name(&name), is_type: false };
        Ok(self.tpd.node(T::ValDef { name, tpt, rhs: Some(rhs) }, at, None, Some(sym)))
    }

    /// `tree` with its references to the parameter accessors `ids` of class `cls` as selections
    /// of `this` in the class's context `cctx`.
    fn anchor_params(&mut self, tree: TNodeId, ids: &[SymbolId], cls: SymbolId, cctx: Ctx) {
        if let T::Ident { name } = *self.tpd.tree(tree)
            && let Some(s) = self.tpd.sym(tree).filter(|s| ids.contains(s))
        {
            let class = crate::defn::type_ref(self.u, cls);
            let this = self.u.intern_type(Type::This(class));
            let tpe = self.u.intern_type(Type::Sym(Some(this), s));
            let span = self.tpd.span(tree);
            let select = self.this_select(span, name, tpe, s, cctx);
            let node = self.tpd.get(select);
            let (new, tpe, sym) = (node.tree.clone(), node.tpe, node.sym);
            let node = self.tpd.get_mut(tree);
            (node.tree, node.tpe, node.sym) = (new, tpe, sym);
            return;
        }
        for c in self.tpd.children(tree) {
            self.anchor_params(c, ids, cls, cctx);
        }
    }
}
