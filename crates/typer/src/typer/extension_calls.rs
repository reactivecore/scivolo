// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Selections that call an extension method (dotty's `tryExtensionOrConversion` with
//! `extMethodApply`): `qual.name` typed as `name(qual)` for an extension method `name` in scope.

use super::{Pt, Typer};
use crate::ctx::{Ctx, Mode};
use crate::symbols::Completion;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::tags::EXTENSION;
use tasty::{Kind, SymbolId};

/// `ctx` looking up extension methods only.
pub fn extension_ctx(ctx: Ctx) -> Ctx {
    ctx.with_mode(ctx.mode.with(Mode::EXTENSION_METHOD))
}

impl Typer<'_> {
    /// Whether an extension method named `name` is visible in `ctx`.
    pub fn extension_visible(&mut self, name: &str, ctx: Ctx) -> bool {
        let found = self.find_ref(name, false, extension_ctx(ctx));
        let Ok(Some(found)) = found else { return found.is_err() };
        !found.syms.is_empty()
    }

    /// Selection `id` of `name` from typed qualifier `qual`, which has no such member, as a call
    /// of the extension methods named `name` that `ctx` sees.
    pub fn extension_select(&mut self, id: NodeId, qual: TNodeId, name: TName, pt: Pt, ctx: Ctx) -> TNodeId {
        let text = self.text(name.name).to_owned();
        let ctx = extension_ctx(ctx);
        let found = match self.find_ref(&text, false, ctx) {
            Ok(Some(found)) => found.syms,
            _ => Vec::new(),
        };
        if found.is_empty() {
            return self.unsupported(id, "extension method among other definitions");
        }
        if found.iter().any(|&s| self.u.symbol(s).name != text) {
            return self.unsupported(id, "extension method through a renaming import");
        }
        if self.tpd.sym(qual).is_some_and(|s| self.u.symbol(s).kind == tasty::Kind::Package) {
            return self.unsupported(id, "extension method on a package");
        }
        let java_statics = self.tpd.sym(qual).is_some_and(|s| {
            let full = self.u.full_name(s);
            self.u.symbol(s).flags.has(tasty::tags::OBJECT)
                && (full.starts_with("java.") || full.starts_with("javax."))
        });
        if java_statics {
            return self.unsupported(id, "extension method on the statics of a Java class");
        }
        let fun = self.ast.node(Tree::Ident { name });
        self.ext_method_apply(id, fun, qual, pt, ctx)
    }

    /// Selection `id` of an extension method from `qual` as `fun(qual)` (dotty's `extMethodApply`),
    /// `fun` an untyped reference to the method placed at the name.
    pub fn ext_method_apply(&mut self, id: NodeId, fun: NodeId, qual: TNodeId, pt: Pt, ctx: Ctx) -> TNodeId {
        let span = self.ast.span(id);
        let receiver = self.spliced(qual);
        self.extension_receivers.insert(receiver);
        let name_span = self.select_name_span(id, qual);
        self.ast.with_span(fun, name_span);
        let args = self.ast.list(&[receiver]);
        let apply = self.ast.node(Tree::Apply { fun, args });
        let inner = match self.tpd.span(qual) {
            q if q.exists() => q.union(name_span),
            _ => span,
        };
        self.ast.with_span(apply, Span::synthetic(inner.start(), inner.end()));
        let typed = self.typed_apply(apply, fun, args, pt, ctx);
        let ctx = ctx.with_mode(ctx.mode.without(Mode::EXTENSION_METHOD));
        let typed = match pt {
            Pt::PolyFun(_) => Ok(typed),
            Pt::Fun(proto) if self.protos[proto].using => self.with_type_vars(typed),
            _ => self.with_type_vars(typed).and_then(|t| self.using_applied(t, pt, ctx)),
        };
        let typed = match typed {
            Ok(t) => t,
            Err(what) => return self.unsupported(id, what),
        };
        let typed = self.inlined_transparent(typed, pt, ctx).unwrap_or(typed);
        self.tpd.get_mut(typed).span = span;
        typed
    }

    /// Whether `pt` applies a function to the receiver of an extension method call (dotty's
    /// `isExtensionApplyProto`).
    pub fn is_extension_apply(&self, pt: Pt) -> bool {
        match pt {
            Pt::PolyFun(p) => self.is_extension_apply(self.poly_protos[p].pt),
            Pt::Fun(i) => matches!(self.protos[i].args[..], [a] if self.extension_receivers.contains(&a)),
            _ => false,
        }
    }

    /// `f` applied to the arguments of its leading `using` clauses, its result first constrained
    /// by `pt` (dotty's `adaptNoArgsImplicitMethod`).
    fn using_applied(&mut self, f: TNodeId, pt: Pt, ctx: Ctx) -> Result<TNodeId, &'static str> {
        if self.takes_using(f) {
            let tpe = self.tpd.tpe(f);
            let info = self.method_info(tpe).unwrap_or(tpe);
            let meth = self.tpd.sym(self.tpd.meth_part(f));
            let _ = self.constrain_applied_result(meth, info, pt);
        }
        self.with_using_args(f, false, ctx)
    }

    /// The span of the name of selection `id` from `qual` (dotty's `Select.nameSpan`).
    fn select_name_span(&self, id: NodeId, qual: TNodeId) -> Span {
        let span = self.ast.span(id);
        let qual_span = self.tpd.span(qual);
        match *self.ast.tree(id) {
            Tree::Select { name, .. } if qual_span.exists() && qual_span.start() > span.point() => {
                let len = self.text(name.name).len() as u32;
                Span::new(span.start(), span.start() + len, span.point())
            }
            _ => Span::new(span.point(), span.end(), span.point()),
        }
    }

    /// Identifier `id` naming a sibling of the enclosing extension method, typed as a selection
    /// from their shared receiver (dotty's `localExtensionSelection`).
    pub fn local_extension_selection(
        &mut self,
        id: NodeId,
        name: TName,
        found: &[SymbolId],
        pt: Pt,
        ctx: Ctx,
    ) -> Option<TNodeId> {
        if let Pt::Fun(i) = pt
            && let [a] = self.protos[i].args[..]
            && self.splices.contains_key(&a)
        {
            return None;
        }
        let mut owner = Some(ctx.owner);
        let xmethod = loop {
            let sym = self.u.symbol(owner?);
            if sym.kind == Kind::Def && sym.flags.has(EXTENSION) {
                break owner?;
            }
            owner = sym.owner;
        };
        let xparam = self.lead_param(xmethod)?;
        let span_of = |t: &Self, p: SymbolId| t.syms.param_trees.get(&p).map(|&d| t.ast.span(d));
        let xspan = span_of(self, xparam);
        let sibling = found.iter().any(|&s| {
            self.u.symbol(s).flags.has(EXTENSION)
                && self
                    .lead_param(s)
                    .is_some_and(|p| p == xparam || xspan.is_some() && span_of(self, p) == xspan)
        });
        if !sibling {
            return None;
        }
        let span = self.ast.span(id);
        let xname = self.build().term_name(xparam);
        let tpe = self.ref_type(None, xparam);
        let at = Span::synthetic(span.start(), span.start());
        let receiver = self.tpd.node(T::Ident { name: xname }, at, Some(tpe), Some(xparam));
        let qualifier = self.spliced(receiver);
        let select = self.ast.node(Tree::Select { qualifier, name });
        self.ast.with_span(select, span);
        Some(self.typed(select, pt, ctx))
    }

    /// The receiver parameter of script extension method `m` (dotty's `leadParamOf`).
    fn lead_param(&mut self, m: SymbolId) -> Option<SymbolId> {
        if matches!(self.syms.completions.get(&m), Some(Completion::Pending { .. })) {
            self.complete(m);
        }
        let clauses = self.syms.param_clauses.get(&m)?;
        let right = self.u.symbol(m).name.ends_with(':');
        let [p] = clauses.get(usize::from(right))?[..] else { return None };
        Some(p)
    }

    /// An untyped node standing for typed tree `t`, at its span.
    pub fn spliced(&mut self, t: TNodeId) -> NodeId {
        let node = self.ast.node(Tree::TypeTree);
        self.ast.with_span(node, self.tpd.span(t));
        self.splices.insert(node, t);
        node
    }
}
