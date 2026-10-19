// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/InlineReducer.scala
//! Selections of a parameter of a new instance reduced to the argument (dotty's
//! `InlineReducer.NewInstance`, `reduceProjection` and `normalizeBinding`).

use super::Reducer;
use crate::inlining::Unsupported;
use crate::inlining::copy::TreeTypeMap;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::TName;
use tasty::tags::{CASE, PARAMsetter, SYNTHETIC, TRAIT};
use tasty::{Kind, SymbolId};

/// The class of an instance creation, its arguments, the bindings around it and whether it is
/// evaluated before the selection.
struct NewInstance {
    cls: SymbolId,
    args: Vec<TNodeId>,
    prefix: Vec<TNodeId>,
    precomputed: bool,
}

impl Reducer<'_, '_> {
    fn new_instance(&mut self, t: TNodeId) -> Option<NewInstance> {
        let i = &mut *self.i;
        match i.tpd.tree(t).clone() {
            T::Apply { fun, args } => {
                let args = i.tpd.items(args).to_vec();
                let callee = match *i.tpd.tree(fun) {
                    T::TypeApply { fun, .. } => fun,
                    _ => fun,
                };
                if let T::Select { qualifier, .. } = *i.tpd.tree(callee)
                    && let T::New { tpt } = *i.tpd.tree(qualifier)
                    && i.tpd.sym(callee).is_some_and(|s| i.u.symbol(s).name == "<init>")
                {
                    let tpe = i.tpd.tpe(tpt);
                    let cls = i.u.class_symbol(tpe)?;
                    return Some(NewInstance { cls, args, prefix: Vec::new(), precomputed: false });
                }
                let meth = i.tpd.sym(callee)?;
                let s = i.u.symbol(meth);
                if s.name != "apply" || !s.flags.has(SYNTHETIC) {
                    return None;
                }
                let cls = s
                    .owner
                    .and_then(|o| i.u.companion_class(o))
                    .filter(|&c| i.u.symbol(c).flags.has(CASE))?;
                Some(NewInstance { cls, args, prefix: Vec::new(), precomputed: false })
            }
            T::Typed { expr, .. } => self.new_instance(expr),
            T::Ident { .. } => {
                let s = self.i.tpd.sym(t)?;
                let &rhs = self.rhs_of.get(&s)?;
                self.new_instance(rhs)
            }
            T::Inlined { bindings, expansion, .. } => {
                let bindings = i.tpd.items(bindings).to_vec();
                self.new_instance_let(bindings, expansion)
            }
            T::Block { stats, expr } if i.is_elideable(t) => {
                let stats = i.tpd.items(stats).to_vec();
                self.new_instance_let(stats, expr)
            }
            _ => None,
        }
    }

    fn new_instance_let(&mut self, bindings: Vec<TNodeId>, expr: TNodeId) -> Option<NewInstance> {
        let mut n = self.new_instance(expr)?;
        n.prefix = [bindings, n.prefix].concat();
        Some(n)
    }

    /// `t` as the argument it selects if it is `new C(args).x` for a class `C` without
    /// initialization code and parameter `x`, else `t`.
    pub fn reduce_projection(&mut self, t: TNodeId) -> Result<TNodeId, Unsupported> {
        match *self.i.tpd.tree(t) {
            T::Select { qualifier, .. } => {
                let Some(n) = self.new_instance(qualifier) else { return Ok(t) };
                if self.i.u.symbol(n.cls).flags.has(TRAIT) || !self.i.is_no_inits_class(n.cls) {
                    return Ok(t);
                }
                let field = self.i.tpd.sym(t);
                let idx = self.param_accessors(n.cls).iter().position(|&p| Some(p) == field);
                let Some(k) = idx.filter(|&k| k < n.args.len()) else { return Ok(t) };
                let arg = n.args[k];
                if n.precomputed {
                    return if self.i.is_elideable(arg) { self.fresh_copy(arg) } else { Ok(t) };
                }
                let impure = |r: &mut Self, range: std::ops::Range<usize>| {
                    let args: Vec<TNodeId> = n.args[range].to_vec();
                    args.into_iter().filter(|&a| !r.i.is_elideable(a)).collect::<Vec<_>>()
                };
                if !impure(self, k + 1..n.args.len()).is_empty() {
                    return Err((self.i.tpd.span(t), "projection with impure arguments after it"));
                }
                let stats = [n.prefix, impure(self, 0..k)].concat();
                let span = stats.iter().fold(self.i.tpd.span(arg), |s, &x| s.union(self.i.tpd.span(x)));
                let tree = self.i.seq(&stats, arg)?;
                self.i.tpd.get_mut(tree).span = span;
                self.fresh_copy(tree)
            }
            T::Block { stats, expr } => {
                let items = self.i.tpd.items(stats).to_vec();
                if !items.into_iter().all(|s| self.i.is_pure_binding(s)) {
                    return Ok(t);
                }
                let e = self.reduce_projection(expr)?;
                let (span, tpe) = (self.i.tpd.span(t), self.i.tpd.get(t).tpe);
                Ok(self.i.tpd.node(T::Block { stats, expr: e }, span, tpe, None))
            }
            _ => Ok(t),
        }
    }

    /// The fields of class `cls` its constructor parameters define, in order: parameters of a
    /// class read from TASTy, values of one typed here.
    pub fn param_accessors(&mut self, cls: SymbolId) -> Vec<SymbolId> {
        let decls = self.i.u.decls(cls).unwrap_or_default();
        decls
            .into_iter()
            .filter(|&d| {
                let s = self.i.u.symbol(d);
                s.kind == Kind::Param || s.kind == Kind::Val && s.flags.has(PARAMsetter)
            })
            .collect()
    }

    fn fresh_copy(&mut self, t: TNodeId) -> Result<TNodeId, Unsupported> {
        self.i.copy(&mut TreeTypeMap::default(), t, &mut |_, _, _| None)
    }

    /// `val s = rhs` at the call, its right-hand side a reduced projection, `s` typed by it if
    /// that changed it.
    pub fn normalize_binding(&mut self, s: SymbolId, rhs: TNodeId) -> Result<TNodeId, Unsupported> {
        let reduced = self.reduce_projection(rhs)?;
        if reduced != rhs {
            let tpe = self.i.tpd.tpe(reduced);
            self.i.u.set_info(s, tpe);
        }
        self.rhs_of.insert(s, reduced);
        let span = self.i.call_span;
        Ok(self.val_def(s, reduced, span))
    }

    /// `val s = rhs` at `span`, typed by the info of `s` at the start of `rhs`.
    pub fn val_def(&mut self, s: SymbolId, rhs: TNodeId, span: syntax::ast::Span) -> TNodeId {
        let info = self.i.u.info(s);
        let at = self.i.tpd.span(rhs).start_pos();
        let tpt = self.i.tpd.node(T::TypeTree { inferred: false }, at, Some(info), None);
        let name = TName { name: self.i.names.intern(&self.i.u.symbol(s).name.clone()), is_type: false };
        self.i.tpd.node(T::ValDef { name, tpt, rhs: Some(rhs) }, span, None, Some(s))
    }
}
