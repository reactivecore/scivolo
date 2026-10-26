// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/InlineReducer.scala
//! Selections of parameters of new instances in macro arguments reduced to the arguments, as
//! dotty's inliner reduces them in an inline argument before the macro sees it
//! (`InlineReducer.reduceProjection` from `InlineTyper.typedSelect`).

use super::{Inlining, Unsupported};
use crate::tpd::{TNodeId, Tree as T, map_children};
use tasty::tags::{CASE, PARAMsetter, SYNTHETIC, TRAIT};
use tasty::{Kind, SymbolId};

impl Inlining<'_> {
    /// `t` with each selection `new C(args).x` or `C(args).x` of parameter `x` of a class
    /// without initialization code replaced by its argument when all arguments are elideable;
    /// other such selections refused. The trees on the paths are copied.
    pub fn reduced_projections(&mut self, t: TNodeId) -> Result<TNodeId, Unsupported> {
        if let Some(arg) = self.projected(t)? {
            return Ok(arg);
        }
        let kids = self.tpd.children(t).to_vec();
        let mut new = Vec::with_capacity(kids.len());
        for &c in &kids {
            new.push(self.reduced_projections(c)?);
        }
        if new == kids {
            return Ok(t);
        }
        let node = self.tpd.get(t);
        let (tree, span, tpe, sym) = (self.tpd.tree(t).clone(), node.span, node.tpe, node.sym);
        let n = self.tpd.node(tree, span, tpe, sym);
        let mut new = new.into_iter();
        map_children(self.tpd, n, &mut |_, c| new.next().unwrap_or(c));
        Ok(n)
    }

    /// The argument selection `t` of a parameter of a new instance reduces to, if it is one.
    fn projected(&mut self, t: TNodeId) -> Result<Option<TNodeId>, Unsupported> {
        let T::Select { qualifier, .. } = *self.tpd.tree(t) else { return Ok(None) };
        let Some((cls, args)) = self.instance(qualifier) else { return Ok(None) };
        let Some(x) = self.tpd.sym(t) else { return Ok(None) };
        let params: Vec<SymbolId> = (self.u.decls(cls).unwrap_or_default().into_iter())
            .filter(|&d| {
                let s = self.u.symbol(d);
                s.kind == Kind::Param || s.kind == Kind::Val && s.flags.has(PARAMsetter)
            })
            .collect();
        let Some(k) = params.iter().position(|&p| p == x).filter(|&k| k < args.len()) else {
            return Ok(None);
        };
        let refused = (self.tpd.span(t), "a macro argument selecting a parameter of a new instance");
        if self.u.symbol(cls).flags.has(TRAIT) || !self.is_no_inits_class(cls) {
            return Err(refused);
        }
        if !args.iter().all(|&a| self.is_elideable(a)) {
            return Err(refused);
        }
        Ok(Some(args[k]))
    }

    /// The class `t` creates an instance of and the arguments: `new C(args)`, or `C(args)` of a
    /// case class `C`.
    fn instance(&mut self, t: TNodeId) -> Option<(SymbolId, Vec<TNodeId>)> {
        let T::Apply { fun, args } = *self.tpd.tree(t) else { return None };
        let args = self.tpd.items(args).to_vec();
        let callee = match *self.tpd.tree(fun) {
            T::TypeApply { fun, .. } => fun,
            _ => fun,
        };
        if let T::Select { qualifier, .. } = *self.tpd.tree(callee)
            && let T::New { tpt } = *self.tpd.tree(qualifier)
        {
            return Some((self.u.class_symbol(self.tpd.tpe(tpt))?, args));
        }
        let s = self.u.symbol(self.tpd.sym(callee)?);
        if s.name != "apply" || !s.flags.has(SYNTHETIC) {
            return None;
        }
        let class = s.owner.and_then(|o| self.u.companion_class(o))?;
        self.u.symbol(class).flags.has(CASE).then_some((class, args))
    }
}
