//! dotty's migration warnings about refutable patterns of definitions (the messages of
//! `Checking.checkIrrefutable`).

use super::Typer;
use super::irrefutable::{RUNTIME_CHECKED, Refutable, UNCHECKED};
use crate::tpd::{TNodeId, Tree as T};
use syntax::diag::rewrite_notice;
use tasty::TypeId;

impl Typer<'_> {
    /// Reports `reason` as dotty's migration warning about a refutable pattern definition of
    /// selector `sel`.
    pub fn warn_refutable(&mut self, reason: Refutable, sel: TNodeId) {
        let (message, usage, span) = match reason {
            Refutable::NonConforming(p, pt) => {
                let p_tpe = self.tpd.tpe(p);
                let pt = self.without_annotation(pt, UNCHECKED);
                let pt = self.without_annotation(pt, RUNTIME_CHECKED);
                let pt = if self.is_singleton(p_tpe) { pt } else { self.u.widen(pt) };
                let problem =
                    if self.u.is_subtype(p_tpe, pt) { "is more specialized than" } else { "does not match" };
                let (p_text, pt_text) = (self.message_type(p_tpe), self.message_type(pt));
                let message = format!(
                    "pattern's type {p_text} {problem} the right hand side expression's type {pt_text}"
                );
                (message, "the narrowing", self.tpd.span(sel))
            }
            Refutable::Extractor(p) => {
                let message = match self.extractor_name(p) {
                    Some(name) => format!("pattern binding uses refutable extractor `{name}`"),
                    None => "pattern binding uses refutable extractor".to_owned(),
                };
                let span = self.tpd.span(p).union(self.tpd.span(sel));
                (message, "this usage", span)
            }
            Refutable::Unknown => return,
        };
        let rewrite = rewrite_notice("This patch", "3.8-migration", "");
        let text = format!(
            "{message}\n\nIf {usage} is intentional, this can be communicated by adding `.runtimeChecked` \
             after the expression,\nwhich may result in a MatchError at runtime.{rewrite}"
        );
        self.diags.warning_at(span.start(), span.end(), span.point(), text);
    }

    /// Type `t` as dotty's messages show it, a repeated type as `T*`.
    fn message_type(&mut self, t: TypeId) -> String {
        if let Some(elem) = self.repeated_elem(t) {
            return format!("{}*", self.message_type(elem));
        }
        let text = if self.is_singleton(t) { self.found_text(t) } else { self.plain_text(t) };
        text.unwrap_or_else(|| self.u.show_user(t))
    }

    /// The path of the object whose extractor pattern `pat` calls, as dotty shows it.
    fn extractor_name(&mut self, pat: TNodeId) -> Option<String> {
        let T::UnApply { fun, .. } = *self.tpd.tree(pat) else { return None };
        let T::Select { qualifier, .. } = *self.tpd.tree(self.tpd.meth_part(fun)) else { return None };
        self.path_text(qualifier)
    }

    /// Path `t`, identifiers selected from identifiers, an identifier with the prefix of its
    /// type (`Test.A` for `A`).
    fn path_text(&mut self, t: TNodeId) -> Option<String> {
        match *self.tpd.tree(t) {
            T::Ident { name } => {
                let shown = self.u.show_user(self.tpd.tpe(t));
                let name = self.text(name.name);
                let with_prefix = shown.strip_suffix(".type").filter(|s| s.ends_with(&format!(".{name}")));
                Some(with_prefix.unwrap_or(name).to_owned())
            }
            T::Select { qualifier, name } => {
                let prefix = self.path_text(qualifier)?;
                Some(format!("{prefix}.{}", self.text(name.name)))
            }
            _ => None,
        }
    }
}
