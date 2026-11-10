// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Checking.scala
//! dotty's migration warnings about refutable patterns of definitions (the messages of
//! `Checking.checkIrrefutable`).

use super::super::Typer;
use super::{RUNTIME_CHECKED, Refutable, UNCHECKED};
use crate::tpd::{TNodeId, Tree as T};
use tasty::TypeId;

/// Reports `reason` as dotty's migration warning about a refutable pattern definition of
/// selector `sel`, as `val Some(x) = opt`.
pub fn warn_refutable(t: &mut Typer<'_>, reason: Refutable, sel: TNodeId) {
    let (message, usage, span) = match reason {
        Refutable::NonConforming(p, pt) => {
            let p_tpe = t.tpd.tpe(p);
            let pt = t.without_annotation(pt, UNCHECKED);
            let pt = t.without_annotation(pt, RUNTIME_CHECKED);
            let pt = if t.is_singleton(p_tpe) { pt } else { t.u.widen(pt) };
            let problem =
                if t.u.is_subtype(p_tpe, pt) { "is more specialized than" } else { "does not match" };
            let (p_text, pt_text) = (message_type(t, p_tpe), message_type(t, pt));
            let message =
                format!("pattern's type {p_text} {problem} the right hand side expression's type {pt_text}");
            (message, "the narrowing", t.tpd.span(sel))
        }
        Refutable::Extractor(p) => {
            let message = match extractor_name(t, p) {
                Some(name) => format!("pattern binding uses refutable extractor `{name}`"),
                None => "pattern binding uses refutable extractor".to_owned(),
            };
            let span = t.tpd.span(p).union(t.tpd.span(sel));
            (message, "this usage", span)
        }
        Refutable::Unknown => return,
    };

    let rewrite = t.rewrite_notice("This patch", "3.8-migration");
    let text = format!(
        "{message}\n\nIf {usage} is intentional, this can be communicated by adding `.runtimeChecked` \
         after the expression,\nwhich may result in a MatchError at runtime.{rewrite}"
    );
    t.diags.warning_at(span.start(), span.end(), span.point(), text);
}

/// Type `tpe` as dotty's messages show it, a repeated type as `T*`.
fn message_type(t: &mut Typer<'_>, tpe: TypeId) -> String {
    if let Some(elem) = t.repeated_elem(tpe) {
        return format!("{}*", message_type(t, elem));
    }
    let text = if t.is_singleton(tpe) { t.found_text(tpe) } else { t.plain_text(tpe) };
    text.unwrap_or_else(|| t.u.show_user(tpe))
}

/// The path of the object whose extractor pattern `pat` calls, as dotty shows it.
fn extractor_name(t: &mut Typer<'_>, pat: TNodeId) -> Option<String> {
    let T::UnApply { fun, .. } = *t.tpd.tree(pat) else { return None };
    let T::Select { qualifier, .. } = *t.tpd.tree(t.tpd.meth_part(fun)) else { return None };
    path_text(t, qualifier)
}

/// Path `tree`, identifiers selected from identifiers, an identifier with the prefix of its
/// type (`Test.A` for `A`).
fn path_text(t: &mut Typer<'_>, tree: TNodeId) -> Option<String> {
    match *t.tpd.tree(tree) {
        T::Ident { name } => {
            let shown = t.u.show_user(t.tpd.tpe(tree));
            let name = t.text(name.name);
            let with_prefix = shown.strip_suffix(".type").filter(|s| s.ends_with(&format!(".{name}")));
            Some(with_prefix.unwrap_or(name).to_owned())
        }
        T::Select { qualifier, name } => {
            let prefix = path_text(t, qualifier)?;
            Some(format!("{prefix}.{}", t.text(name.name)))
        }
        _ => None,
    }
}
