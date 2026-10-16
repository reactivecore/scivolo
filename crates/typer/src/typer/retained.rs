//! Retained inline methods, which override a method that is not inline and so keep a body at run
//! time: each followed by its body retainer (dotty's `isRetainedInline`, `Inlines.bodyRetainer`).

use super::Typer;
use crate::ctx::Ctx;
use crate::inlining::Unsupported;
use crate::tpd::{TNodeId, Tree as T};
use rustc_hash::FxHashMap;
use syntax::ast::flags as f;
use syntax::ast::modifiers::Modifiers;
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::tags::INLINE;
use tasty::{Kind, SymbolId};

/// The suffix of the name of a body retainer (dotty's `BodyRetainerName`).
pub const RETAINED_BODY: &str = "$retainedBody";

impl Typer<'_> {
    /// Whether `sym` is a concrete inline method that overrides a method that is not inline.
    pub fn is_retained_inline(&mut self, sym: SymbolId) -> bool {
        let s = self.u.symbol(sym);
        if s.kind != Kind::Def || !s.flags.has(INLINE) || self.u.is_deferred(sym) {
            return false;
        }
        let overridden = self.overridden(sym);
        overridden.into_iter().any(|m| !self.u.symbol(m).flags.has(INLINE))
    }

    /// Class body `body`, typed in `ctx`, with the body retainer of each retained inline method
    /// after the method (dotty's `inlineExpansion`).
    pub fn with_retainers(&mut self, body: Vec<TNodeId>, ctx: Ctx) -> Vec<TNodeId> {
        let mut out = Vec::with_capacity(body.len());
        for stat in body {
            out.push(stat);
            let Some(sym) = self.tpd.sym(stat).filter(|_| matches!(self.tpd.tree(stat), T::DefDef { .. }))
            else {
                continue;
            };
            if self.is_retained_inline(sym) {
                out.extend(self.body_retainer(stat, sym, ctx));
            }
        }
        out
    }

    /// `private def f$retainedBody(ps) = f(ps)` for retained inline method `f` defined by `def`,
    /// entered and typed in `ctx`, the call to expand when the unit is inlined.
    fn body_retainer(&mut self, def: TNodeId, f: SymbolId, ctx: Ctx) -> Option<TNodeId> {
        let T::DefDef { paramss, tpt, rhs: Some(rhs), .. } = *self.tpd.tree(def) else { return None };
        let body = match *self.tpd.tree(rhs) {
            T::Typed { expr, .. } if self.tpd.span(rhs).is_synthetic() => expr,
            _ => rhs,
        };
        let span = self.tpd.span(body).start_pos();
        let flags = self.syms.flags.get(&f).copied().unwrap_or_default();
        let clauses: Vec<Vec<TNodeId>> =
            self.tpd.clause_items(paramss).iter().map(|&c| self.tpd.items(c).to_vec()).collect();
        let params: Vec<TNodeId> = clauses.iter().flatten().copied().collect();
        if params.iter().any(|&p| !matches!(self.tpd.tree(p), T::ValDef { .. })) {
            self.unsupported_at(self.tpd.span(def), "retained inline method with type parameters");
            return None;
        }
        let implicit = params
            .iter()
            .filter_map(|&p| self.tpd.sym(p))
            .any(|p| self.syms.flags.get(&p).is_some_and(|fl| fl.is_one_of(f::GIVEN_OR_IMPLICIT)));
        if implicit || flags.is(f::GIVEN_OR_IMPLICIT) {
            self.unsupported_at(self.tpd.span(def), "retained inline method with implicit parameters");
            return None;
        }
        let fun = self.member_ref(f, span);
        let mut call = self.ast.node(Tree::TypeTree);
        self.splices.insert(call, fun);
        let mut retainer_clauses = Vec::new();
        for clause in &clauses {
            let mut defs = Vec::new();
            let mut args = Vec::new();
            for &p in clause {
                let p = self.tpd.sym(p).expect("a parameter");
                let name = self.term_name_of(p);
                let info = self.u.info(p);
                let tpt = self.splice_type(info);
                let mods = self.ast.mods(Modifiers::with_flags(f::TERM_PARAM));
                defs.push(self.ast.node(Tree::ValDef { name, tpt, rhs: None, mods }));
                args.push(self.ast.node(Tree::Ident { name }));
            }
            let args = self.ast.list(&args);
            call = self.ast.node(Tree::Apply { fun: call, args });
            retainer_clauses.push(self.ast.list(&defs));
        }
        let result = self.tpd.tpe(tpt);
        let tpt = self.splice_type(result);
        let name = format!("{}{RETAINED_BODY}", self.u.symbol(f).name);
        let name = TName { name: self.name(&name), is_type: false };
        let paramss = self.ast.clauses(&retainer_clauses);
        let dropped = f::INLINE.or(f::MACRO).or(f::OVERRIDE).or(f::ABS_OVERRIDE).or(f::TRANSPARENT);
        let mods = self.ast.mods(Modifiers::with_flags(flags.and_not(dropped).or(f::PRIVATE)));
        let retainer: NodeId = self.ast.node(Tree::DefDef { name, paramss, tpt, rhs: Some(call), mods });
        self.ast.with_span(retainer, Span::at(span.start()));
        self.index(&[retainer], ctx);
        let sym = self.syms.by_tree[&retainer];
        let own = self.syms.flags.get(&sym).copied().unwrap_or_default().and_not(f::LOCAL);
        let own = if flags.is(f::TRANSPARENT) { own.or(f::TRANSPARENT) } else { own };
        self.syms.set_flags(self.u, sym, own);
        let typed = self.typed_stat(retainer, ctx);
        if let T::DefDef { rhs: Some(call), .. } = *self.tpd.tree(typed)
            && let Some(&call_ctx) = self.inline_ctxs.get(&call)
        {
            match self.expanded_retained(call, f, call_ctx) {
                Ok(inlined) => {
                    let T::DefDef { name, paramss, tpt, .. } = *self.tpd.tree(typed) else { unreachable!() };
                    self.tpd.get_mut(typed).tree = T::DefDef { name, paramss, tpt, rhs: Some(inlined) };
                }
                Err((at, what)) => self.unsupported_at(at, what),
            }
        }
        Some(typed)
    }

    /// Call `call` in `ctx` of retained inline method `f` expanded, as the typer expands
    /// transparent calls.
    fn expanded_retained(&mut self, call: TNodeId, f: SymbolId, ctx: Ctx) -> Result<TNodeId, Unsupported> {
        let span = self.tpd.span(call);
        self.define_call_types(call).map_err(|w| (span, w))?;
        let mut bodies = FxHashMap::default();
        self.collect_transparent_bodies(f, false, &mut bodies).map_err(|w| (span, w))?;
        let with_inits = self.classes_with_inits();
        self.with_inlining(true, |i| i.inline_forced((bodies, with_inits), call, ctx))
    }

    /// A reference to member `m` from inside its class: an identifier if the class is a static
    /// owner, else a selection from its `this` (dotty's `ref`).
    pub fn member_ref(&mut self, m: SymbolId, span: Span) -> TNodeId {
        let cls = self.u.symbol(m).owner.expect("a member");
        if self.is_static_owner(cls) {
            let prefix = crate::defn::owner_prefix(self.u, cls);
            let tpe = self.u.intern_type(tasty::Type::Sym(Some(prefix), m));
            return self.build().ident(m, tpe, span);
        }
        let class_type = crate::defn::type_ref(self.u, cls);
        let this = self.build().this(class_type, span);
        self.build().select(this, m, span)
    }

    /// Per value parameter of method `m`, over all its clauses, whether it is inline.
    pub fn inline_params(&mut self, m: SymbolId) -> Vec<bool> {
        self.complete(m);
        let params: Vec<SymbolId> = match self.syms.param_clauses.get(&m) {
            Some(clauses) => clauses.iter().flatten().copied().collect(),
            None => self.u.symbol(m).decls.clone(),
        };
        let params = params.into_iter().filter(|&p| self.u.symbol(p).kind == Kind::Param);
        params.map(|p| self.u.symbol(p).flags.has(INLINE)).collect()
    }

    pub fn term_name_of(&mut self, s: SymbolId) -> TName {
        let text = self.u.symbol(s).name.clone();
        TName { name: self.name(&text), is_type: false }
    }
}
