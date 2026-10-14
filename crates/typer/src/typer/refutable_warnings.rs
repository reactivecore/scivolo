//! dotty's migration warnings about refutable patterns of definitions and generators (the
//! messages of `Checking.checkIrrefutable`).

use super::Typer;
use super::irrefutable::Refutable;
use crate::tpd::{TNodeId, Tree as T};
use syntax::diag::rewrite_notice;
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// Reports `reason` as dotty's migration warning about a refutable pattern `pat` of selector
    /// `sel`, in a definition if `pat_def`, else in a generator.
    pub fn warn_refutable(
        &mut self,
        reason: Refutable,
        sel: TNodeId,
        pat: TNodeId,
        pat_def: bool,
    ) -> Result<(), &'static str> {
        let (message, usage, span) = match reason {
            Refutable::NonConforming(p, pt) => {
                let p_tpe = self.tpd.tpe(p);
                let pt = self.without_annotations(pt);
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
            Refutable::Unknown => return Err("refutable pattern definition"),
        };
        let span = if pat_def { span } else { self.tpd.span(pat) };
        let (fix, addendum, version) = if pat_def {
            (
                "adding `.runtimeChecked` after the expression",
                "may result in a MatchError at runtime",
                "3.8-migration",
            )
        } else {
            (
                "adding the `case` keyword before the full pattern",
                "will result in a filtering for expression (using `withFilter`)",
                "3.2-migration",
            )
        };
        let rewrite = rewrite_notice("This patch", version, "");
        let text = format!(
            "{message}\n\nIf {usage} is intentional, this can be communicated by {fix},\nwhich {addendum}.{rewrite}"
        );
        self.diags.warning_at(span.start(), span.end(), span.point(), text);
        Ok(())
    }

    /// `t` without its annotations.
    fn without_annotations(&mut self, t: TypeId) -> TypeId {
        match *self.u.types().get(t) {
            Type::Annotated(inner, _) => self.without_annotations(inner),
            _ => t,
        }
    }

    /// Type `t` as dotty's messages show it, a repeated type as `T*`.
    fn message_type(&mut self, t: TypeId) -> String {
        if let Some(elem) = self.repeated_elem(t) {
            return format!("{}*", self.message_type(elem));
        }
        let text = if self.is_singleton(t) { self.found_text(t) } else { self.plain_text(t) };
        text.unwrap_or_else(|| self.u.show_user(t))
    }

    /// The name of the object whose extractor pattern `pat` calls, as dotty shows it.
    fn extractor_name(&mut self, pat: TNodeId) -> Option<String> {
        let T::UnApply { fun, .. } = *self.tpd.tree(pat) else { return None };
        let T::Select { qualifier, .. } = *self.tpd.tree(self.tpd.meth_part(fun)) else { return None };
        let T::Ident { name } = *self.tpd.tree(qualifier) else { return None };
        Some(self.text(name.name).to_owned())
    }
}
