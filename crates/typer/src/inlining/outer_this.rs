// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/tpd.scala
//   compiler/src/dotty/tools/dotc/inlines/Inliner.scala
//! Proxies of the `this` of the classes enclosing an inline method's class that its body refers
//! to, bound to the paths the call's prefix makes them (dotty's `computeThisBindings`).

use super::Unsupported;
use super::expand::Expansion;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{Span, TName};
use tasty::tags::INLINEPROXY;
use tasty::{Flags, Kind, Symbol, SymbolId, Type, TypeId};

/// The number of outer accessors a selection named `name` stands for, if it is an outer
/// selection `n_<outer>` (dotty's `OuterSelectName`).
pub fn outer_hops(name: &str) -> Option<usize> {
    name.strip_suffix("_<outer>")?.parse().ok()
}

impl Expansion<'_, '_> {
    /// `C.this` for the prefix of identifier `fun` if it is the `this` of `cls` (dotty's
    /// `desugarIdentPrefix`).
    pub fn prefix_this(&mut self, fun: TNodeId, cls: SymbolId) -> Result<TNodeId, Unsupported> {
        let refused = self.err("inline body referring to an outer this");
        let tpe = self.i.tpd.tpe(fun);
        let Type::Sym(Some(p), _) = *self.i.u.types().get(tpe) else { return Err(refused) };
        let this = match *self.i.u.types().get(p) {
            Type::This(c) => Some(c),
            _ => None,
        };
        let is_cls = this.is_some_and(|c| self.i.u.class_symbol(c) == Some(cls));
        let point = Span::at(self.i.tpd.span(self.call).point());
        is_cls.then(|| self.i.this_tree(p, point)).flatten().ok_or(refused)
    }

    /// The prefix of identifier `fun` as a tree if a path other than a `this` (dotty's
    /// `desugarIdentPrefix`).
    pub fn ident_prefix(&mut self, fun: TNodeId) -> Option<TNodeId> {
        let Type::Sym(Some(p), _) = *self.i.u.types().get(self.i.tpd.get(fun).tpe?) else { return None };
        let path = matches!(self.i.u.types().get(p), Type::Sym(_, s) if self.i.u.symbol(*s).kind.is_term());
        let point = Span::at(self.i.tpd.span(fun).start());
        path.then(|| self.i.singleton(p, point)).flatten()
    }

    /// The classes `outers`, enclosing class `cls`, with the proxies of their `this` for call
    /// prefix `q` of type `qt`, innermost first, bound after `last`, the proxy of the `this` of
    /// `cls` if any: each to the stable path the prefix makes it if `pure` or after another proxy
    /// (dotty's `ref(info)`), else to the static object it is, else to an outer selection from the
    /// last proxy or the prefix.
    pub fn bind_outer_this(
        &mut self,
        (cls, q, qt): (SymbolId, TNodeId, TypeId),
        outers: &[SymbolId],
        last: Option<SymbolId>,
        pure: bool,
    ) -> Result<Vec<(SymbolId, SymbolId)>, Unsupported> {
        let mut sorted = outers.to_vec();
        sorted.sort_by_key(|&c| std::cmp::Reverse(self.nesting_level(c)));
        let point = Span::at(self.i.tpd.span(self.call).point());
        let at = usize::from(last.is_some());
        let mut last = last.map(|p| (cls, p, self.nesting_level(cls)));
        let mut out = Vec::new();
        for c in sorted {
            let level = self.nesting_level(c);
            let class_type = crate::defn::type_ref(self.i.u, c);
            let this = self.i.u.intern_type(Type::This(class_type));
            let seen = self.i.u.as_seen_from(this, qt, cls);
            let stable = match *self.i.u.types().get(seen) {
                Type::Sym(_, s) => self.i.is_stable_member(s) && self.i.is_singleton_path(seen),
                _ => false,
            };
            let proxy = self.this_proxy_symbol(c, seen);
            let path = match stable && (last.is_some() || pure) {
                true => Some(seen),
                false => self.static_module_ref(seen),
            };
            let rhs = match (path.and_then(|p| self.i.singleton(p, point)), last) {
                (Some(r), _) => r,
                (None, Some((last_cls, last_proxy, last_level))) => {
                    let proxy_ref = self.i.u.intern_type(Type::Sym(None, last_proxy));
                    let start = self.i.singleton(proxy_ref, point).expect("a proxy reference");
                    self.outer_select((start, last_cls), (cls, qt), last_level - level, seen)
                }
                (None, None) => {
                    let mut m = super::copy::TreeTypeMap {
                        old_owner: Some(self.owner),
                        new_owner: Some(proxy),
                        ..Default::default()
                    };
                    let pre = self.i.copy(&mut m, q, &mut |_, _, _| None)?;
                    let pre_level = self.nesting_level(cls);
                    match pre_level > level {
                        true => self.outer_select((pre, cls), (cls, qt), pre_level - level, seen),
                        false => pre,
                    }
                }
            };
            self.bind_proxy(proxy, rhs, point, at + out.len());
            out.push((c, proxy));
            last = Some((c, proxy, level));
        }
        Ok(out)
    }

    /// `(prefix: C).n_<outer>` for `prefix` of class `C` seen from call prefix type `qt` of
    /// the inline method's class `cls`, of the skolem of `info` (dotty's `outerSelect`).
    fn outer_select(
        &mut self,
        (prefix, prefix_cls): (TNodeId, SymbolId),
        (cls, qt): (SymbolId, TypeId),
        hops: usize,
        info: TypeId,
    ) -> TNodeId {
        let span = self.i.tpd.span(prefix);
        let class = crate::defn::type_ref(self.i.u, prefix_cls);
        let this = self.i.u.intern_type(Type::This(class));
        let params = self.i.u.type_params(prefix_cls);
        let args: Vec<TypeId> =
            params.into_iter().map(|p| self.i.u.intern_type(Type::Sym(Some(this), p))).collect();
        let applied = match args.is_empty() {
            true => class,
            false => self.i.u.intern_type(Type::Applied(class, args)),
        };
        let tpe = self.i.u.as_seen_from(applied, qt, cls);
        let tpt = self.i.tpd.node(T::TypeTree { inferred: false }, span, Some(tpe), None);
        let typed = self.i.tpd.node(T::Typed { expr: prefix, tpt }, span, Some(tpe), None);
        let name = TName { name: self.i.names.intern(&format!("{hops}_<outer>")), is_type: false };
        let skolem = self.i.u.fresh_skolem(info);
        self.i.tpd.node(T::Select { qualifier: typed, name }, span, Some(skolem), None)
    }

    /// The reference to the static object whose class `t` widens to, if it does (dotty's
    /// `ref(rhsClsSym.sourceModule)`).
    fn static_module_ref(&mut self, t: TypeId) -> Option<TypeId> {
        let widened = self.i.u.widen(t);
        let c = self.i.u.class_symbol(widened)?;
        if !self.i.u.is_module_class(c) || !self.i.is_static_owner(c) {
            return None;
        }
        let m = self.i.u.source_module(c)?;
        Some(crate::defn::type_ref(self.i.u, m))
    }

    /// A proxy of the `this` of class `cls`, of type `info`, owned by the call's owner.
    pub fn this_proxy_symbol(&mut self, cls: SymbolId, info: TypeId) -> SymbolId {
        let proxy = self.i.u.define(Symbol {
            name: format!("{}_this", self.i.u.symbol(cls).name),
            kind: Kind::Val,
            owner: Some(self.owner),
            flags: Flags::default().with(INLINEPROXY),
            target: None,
            is_extern: false,
            derived: false,
            decls: Vec::new(),
            tree: None,
        });
        self.i.u.set_info(proxy, info);
        proxy
    }

    /// Puts the definition of `proxy` as `rhs`, its type tree at `tpt_span`, at index `at` of the
    /// bindings.
    pub fn bind_proxy(&mut self, proxy: SymbolId, rhs: TNodeId, tpt_span: Span, at: usize) {
        let info = self.i.u.info(proxy);
        let tpt = self.i.tpd.node(T::TypeTree { inferred: true }, tpt_span, Some(info), None);
        let name = TName { name: self.i.names.intern(&self.i.u.symbol(proxy).name.clone()), is_type: false };
        let call_span = self.i.tpd.span(self.call);
        let def = self.i.tpd.node(T::ValDef { name, tpt, rhs: Some(rhs) }, call_span, None, Some(proxy));
        self.bindings.insert(at, def);
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
