// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Selections that call an extension method (dotty's `tryExtensionOrConversion` with
//! `extMethodApply`): `qual.name` typed as `name(qual)` for an extension method `name` in scope.

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::symbols::Completion;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::tags::EXTENSION;
use tasty::{Kind, SymbolId};

impl Typer<'_> {
    /// Whether an extension method named `name` is visible in `ctx`.
    pub fn extension_visible(&mut self, name: &str, ctx: Ctx) -> bool {
        let found = self.find_ref(name, false, ctx);
        let Ok(Some(found)) = found else { return found.is_err() };
        found.syms.iter().any(|&s| self.u.symbol(s).flags.has(tasty::tags::EXTENSION))
    }

    /// Selection `id` of `name` from typed qualifier `qual`, which has no such member, as a call
    /// of the extension methods named `name` that `ctx` sees, if all it sees are such.
    pub fn extension_select(&mut self, id: NodeId, qual: TNodeId, name: TName, pt: Pt, ctx: Ctx) -> TNodeId {
        let text = self.text(name.name).to_owned();
        let found = match self.find_ref(&text, false, ctx) {
            Ok(Some(found)) => found.syms,
            _ => Vec::new(),
        };
        if found.is_empty() || !found.iter().all(|&s| self.u.symbol(s).flags.has(tasty::tags::EXTENSION)) {
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
        let name_span = self.select_name_span(id, qual);
        self.ast.with_span(fun, name_span);
        let args = self.ast.list(&[receiver]);
        let apply = self.ast.node(Tree::Apply { fun, args });
        self.ast.with_span(apply, Span::synthetic(span.start(), span.end()));
        let typed = self.typed_apply(apply, fun, args, pt, ctx);
        let tpe = self.tpd.tpe(typed);
        let using_next = matches!(self.u.types().get(tpe), tasty::Type::Method { implicit: true, .. });
        if using_next && !matches!(pt, Pt::Fun(_)) {
            return self.unsupported(id, "extension method with a using clause after the receiver");
        }
        let typed = match pt {
            Pt::PolyFun(_) => typed,
            _ => match self.with_type_vars(typed) {
                Ok(t) => t,
                Err(what) => return self.unsupported(id, what),
            },
        };
        let typed = self.inlined_transparent(typed, pt, ctx).unwrap_or(typed);
        self.tpd.get_mut(typed).span = span;
        typed
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
        let sibling = found
            .iter()
            .any(|&s| self.u.symbol(s).flags.has(EXTENSION) && self.lead_param(s) == Some(xparam));
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
