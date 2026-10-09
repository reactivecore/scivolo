//! Selections that call an extension method (dotty's `tryExtensionOrConversion` with
//! `extMethodApply`): `qual.name` typed as `name(qual)` for an extension method `name` in scope.

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::{NodeId, Span, TName, Tree};

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
        let span = self.ast.span(id);
        let receiver = self.ast.node(Tree::TypeTree);
        self.ast.with_span(receiver, self.tpd.span(qual));
        self.splices.insert(receiver, qual);
        let fun = self.ast.node(Tree::Ident { name });
        self.ast.with_span(fun, Span::new(span.point(), span.end(), span.point()));
        let args = self.ast.list(&[receiver]);
        let apply = self.ast.node(Tree::Apply { fun, args });
        self.ast.with_span(apply, Span::synthetic(span.start(), span.end()));
        let typed = self.typed_apply(apply, fun, args, pt, ctx);
        let tpe = self.tpd.tpe(typed);
        let using_next = matches!(self.u.types().get(tpe), tasty::Type::Method { implicit: true, .. });
        if using_next && !matches!(pt, Pt::Fun(_)) {
            return self.unsupported(id, "extension method with a using clause after the receiver");
        }
        let typed = match self.with_type_vars(typed) {
            Ok(t) => t,
            Err(what) => return self.unsupported(id, what),
        };
        self.tpd.get_mut(typed).span = span;
        typed
    }
}
