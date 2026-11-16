// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/localopt/StringInterpolatorOpt.scala
//! `f` interpolations (dotty's `StringInterpolatorOpt.transformF`): `StringContext(parts*).f(args*)`
//! becomes `new StringOps(format).format(args*)`, `format` the checked format string of the parts.

use super::format_checker::{Args, checked};
use super::format_spec::Checked;
use super::interpolators::interpolation;
use crate::definitions::type_ref;
use crate::{Lower, Result, unsupported};
use names::nme;
use syntax::ast::Span;
use syntax::ast::constant::Constant;
use typer::tpd::{FormatView, TNodeId, Target, Tree};

/// Rewrites `f` interpolation `t`, an argument converted by the view the format check picks; fails
/// for parts or arguments dotty's check rejects.
pub fn transform(l: &mut Lower, t: TNodeId) -> Result<()> {
    let span = l.tpd.span(t);
    let Some((parts, args, seq)) = interpolation(l, t) else {
        return unsupported(span, "f interpolations without a statically known `StringContext`");
    };
    let texts: Option<Vec<Vec<u16>>> = parts
        .iter()
        .map(|&p| match l.tpd.tree(p) {
            Tree::Literal { value: Constant::String(s) } => Some(s.to_vec()),
            _ => None,
        })
        .collect();
    let Some(texts) = texts else { return unsupported(span, "f interpolations of non-literal parts") };
    let texts: Vec<&[u16]> = texts.iter().map(Vec::as_slice).collect();
    let mut typed = Typed { l, args: &args, views: Vec::new() };
    let format = match checked(&texts, &mut typed) {
        Ok(format) => format,
        Err(why) => return unsupported(span, why),
    };
    if !typed.views.is_empty() {
        let mut elems = args.clone();
        for (i, view) in typed.views {
            elems[i] = view;
        }
        replace_elems(l, seq, &elems);
    }
    let string_ops = l.u.class_ref("scala.collection.StringOps");
    let class = l.u.class_symbol(string_ops).expect("`StringOps`");
    let member = |l: &mut Lower, name: &str| *l.u.decls_named(class, name).first().expect(name);
    let (init, format_method) = (member(l, nme::CONSTRUCTOR), member(l, nme::FORMAT));
    let value = Constant::String(format.into());
    let tpe = typer::tpd::constant_type_of(l.u, &value);
    let literal = l.tpd.node(Tree::Literal { value }, Span::NONE, Some(tpe), None);
    let new = l.new_(class, Span::NONE);
    let ctor = l.select(new, init, Span::NONE);
    let ops_type = type_ref(l.u, class);
    let ops = l.apply(ctor, &[literal], ops_type, Span::NONE);
    let fun = l.select(ops, format_method, Span::NONE);
    let string = l.u.class_ref(tasty::defn::STRING);
    reposition(l, seq);
    let call = l.apply(fun, &[seq], string, Span::NONE);
    let call = l.positioned(call);
    l.copy_node(t, call);
    Ok(())
}

/// The arguments of an `f` interpolation with the views the typer recorded for them, and those the
/// format check picked, each with the index of its argument.
struct Typed<'l, 'a> {
    l: &'l mut Lower<'a>,
    args: &'l [TNodeId],
    views: Vec<(usize, TNodeId)>,
}

impl Args for Typed<'_, '_> {
    fn conforms(&mut self, i: usize, t: Target) -> bool {
        let Some(class) = t.class_name() else { return true };
        let target = self.l.u.class_ref(class);
        self.l.u.is_subtype(self.l.tpd.tpe(self.args[i]), target)
    }

    fn converts(&mut self, i: usize, t: Target) -> Checked<bool> {
        match self.l.tpd.format_view(self.args[i], t) {
            None => Ok(false),
            Some(FormatView::Converted(view)) => {
                self.views.push((i, view));
                Ok(true)
            }
            Some(FormatView::Unknown) => Err("f interpolation arguments of an unknown implicit view"),
        }
    }
}

/// Makes `elems` the elements of the sequence literal of argument `seq`.
fn replace_elems(l: &mut Lower, seq: TNodeId, elems: &[TNodeId]) {
    let literal = match *l.tpd.tree(seq) {
        Tree::Typed { expr, .. } => expr,
        _ => seq,
    };
    let list = l.tpd.list(elems);
    if let Tree::SeqLiteral { elems, .. } = &mut l.tpd.get_mut(literal).tree {
        *elems = list;
    }
}

/// Gives argument `seq` and the sequence literal in it the spans of their children, as dotty's
/// new `SeqLiteral` of the format arguments has.
fn reposition(l: &mut Lower, seq: TNodeId) {
    if let Tree::Typed { expr, .. } = *l.tpd.tree(seq) {
        l.tpd.get_mut(expr).span = Span::NONE;
        l.positioned(expr);
    }
    l.tpd.get_mut(seq).span = Span::NONE;
    l.positioned(seq);
}
