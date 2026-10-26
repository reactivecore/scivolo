// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenStat.scala
//   nir/src/main/scala/scala/scalanative/nir/Attrs.scala
//! The annotations Scala Native's code generator reads (`genClassAttrs`, `genMethodAttrs`,
//! `genClassFields`): the attributes they give classes, methods and fields, and those refused.

use super::Gen;
use crate::{Lower, Result, unsupported};
use nir::{Attr, StrId};
use tasty::SymbolId;
use typer::tpd::{TNodeId, Tree};

/// What an annotation class gives the definition it annotates.
#[derive(Clone, Copy)]
enum Gives {
    /// An attribute of methods.
    Method(fn() -> Attr),
    /// An attribute of methods and classes.
    Any(fn() -> Attr),
    /// `link` or `define` of its string argument, of methods and classes.
    Named(fn(StrId) -> Attr),
    /// An attribute of the fields of what it annotates, a field or a class.
    Field(fn() -> Attr),
    /// The name of an extern definition, its string argument.
    Id,
    /// `blocking` of the `extern` attribute of an extern definition.
    Blocking,
    Refused,
}

const ANNOTATIONS: [(&str, Gives); 21] = [
    ("scala.volatile", Gives::Field(|| Attr::Volatile)),
    ("scala.scalanative.annotation.safePublish", Gives::Field(|| Attr::SafePublish)),
    ("scala.inline", Gives::Method(|| Attr::InlineHint)),
    ("scala.noinline", Gives::Method(|| Attr::NoInline)),
    ("scala.scalanative.annotation.alwaysinline", Gives::Method(|| Attr::AlwaysInline)),
    ("scala.scalanative.annotation.nooptimize", Gives::Method(|| Attr::NoOpt)),
    ("scala.scalanative.annotation.nospecialize", Gives::Method(|| Attr::NoSpecialize)),
    ("scala.scalanative.annotation.stub", Gives::Any(|| Attr::Stub)),
    ("scala.scalanative.unsafe.linkCppRuntime", Gives::Any(|| Attr::LinkCppRuntime)),
    ("scala.scalanative.unsafe.link", Gives::Named(Attr::Link)),
    ("scala.scalanative.unsafe.define", Gives::Named(Attr::Define)),
    ("scala.scalanative.annotation.align", Gives::Refused),
    ("scala.scalanative.annotation.nonExtern", Gives::Refused),
    ("scala.scalanative.reflect.annotation.EnableReflectiveInstantiation", Gives::Refused),
    ("scala.scalanative.runtime.struct", Gives::Refused),
    ("scala.scalanative.unsafe.exportAccessors", Gives::Refused),
    ("scala.scalanative.unsafe.exported", Gives::Refused),
    ("scala.scalanative.unsafe.name", Gives::Id),
    ("scala.scalanative.unsafe.package$.blocking", Gives::Blocking),
    ("scala.scalanative.unsafe.package$.extern", Gives::Any(|| Attr::Extern { blocking: false })),
    ("scala.scalanative.unsafe.resolvedAtLinktime", Gives::Refused),
];

fn gives(l: &Lower, class: SymbolId) -> Option<Gives> {
    let name = l.u.full_name(class);
    ANNOTATIONS.iter().find(|(n, _)| *n == name).map(|&(_, g)| g)
}

/// Fails for a definition of unit `root` annotated with an annotation the code generator does
/// not implement, or with `link` or `define` of no string literal.
pub fn check_annotations(l: &Lower, root: TNodeId) -> Result<()> {
    l.tpd.fold(Ok(()), root, &mut |r, t| {
        r?;
        let (Tree::ValDef { .. } | Tree::DefDef { .. } | Tree::TypeDef { .. }) = l.tpd.tree(t) else {
            return Ok(());
        };
        let Some(sym) = l.tpd.sym(t) else { return Ok(()) };
        for &class in l.u.annotations(sym) {
            let what = || format!("annotation @{}", l.u.full_name(class));
            let strings = l.u.annotation_strings(sym, class);
            let literal = !strings.is_empty() && strings.iter().all(Option::is_some);
            match gives(l, class) {
                Some(Gives::Refused) => return unsupported(l.tpd.span(t), what()),
                Some(Gives::Id | Gives::Blocking) if !crate::symbols::is_extern(l.u, sym) => {
                    return unsupported(
                        l.tpd.span(t),
                        format!("{} of a definition that is not extern", what()),
                    );
                }
                Some(Gives::Named(_) | Gives::Id) if !literal => {
                    return unsupported(l.tpd.span(t), format!("{} without a string literal", what()));
                }
                _ => {}
            }
        }
        Ok(())
    })
}

impl Gen<'_, '_> {
    /// The attributes the annotations of method `m` give, after `attrs`, in NIR's order; an
    /// extension method has those of its value class's method.
    pub fn method_attrs(&mut self, m: SymbolId, attrs: Vec<Attr>) -> Vec<Attr> {
        let m = self.l.extensions.iter().find(|&(_, &e)| e == m).map_or(m, |(&method, _)| method);
        self.annotated(m, attrs, true)
    }

    /// The attributes the annotations of class `c` give, after `attrs`, in NIR's order.
    pub fn class_attrs(&mut self, c: SymbolId, attrs: Vec<Attr>) -> Vec<Attr> {
        self.annotated(c, attrs, false)
    }

    /// `attrs` of field `f` of class `c` and those of the annotations of `f`, the value it was made
    /// for, and `c`: `volatile` of the field, `safePublish` of a final field or its class.
    pub fn field_attrs(&mut self, f: SymbolId, c: SymbolId, mut attrs: Vec<Attr>) -> Vec<Attr> {
        let field = [Some(f), self.l.fields.get(&f).copied()];
        let is_final = attrs.iter().any(|a| matches!(a, Attr::Final));
        for (sym, of_class) in field.into_iter().flatten().map(|s| (s, false)).chain([(c, true)]) {
            for &class in self.l.u.annotations(sym) {
                if let Some(Gives::Field(a)) = gives(self.l, class) {
                    match a() {
                        Attr::SafePublish if !is_final => {}
                        Attr::Volatile if of_class => {}
                        attr => attrs.push(attr),
                    }
                }
            }
        }
        canonical(attrs)
    }

    /// `attrs` and those of the annotations of `sym`; a library symbol's from its TASTy, but for
    /// `link` and `define`.
    fn annotated(&mut self, sym: SymbolId, mut attrs: Vec<Attr>, method: bool) -> Vec<Attr> {
        let found: Vec<(Gives, Option<String>)> = if self.l.u.symbol(sym).tree.is_some() {
            let annotated = ANNOTATIONS.iter().filter(|(n, _)| self.l.u.is_annotated(sym, n));
            annotated.filter(|(_, g)| !matches!(g, Gives::Named(_))).map(|&(_, g)| (g, None)).collect()
        } else {
            let mut found = Vec::new();
            for &c in self.l.u.annotations(sym) {
                match gives(self.l, c) {
                    Some(g @ Gives::Named(_)) => {
                        found.extend(self.l.u.annotation_strings(sym, c).iter().map(|s| (g, s.clone())))
                    }
                    Some(g) => found.push((g, None)),
                    None => {}
                }
            }
            found
        };
        for (g, string) in found {
            match g {
                Gives::Method(a) if method => attrs.push(a()),
                Gives::Any(a) => attrs.push(a()),
                Gives::Named(a) => {
                    attrs.push(a(StrId(self.pool.strings.intern_str(&string.unwrap_or_default()))))
                }
                _ => {}
            }
        }
        canonical(attrs)
    }
}

/// `attrs` as Scala Native's `Attrs.fromSeq(attrs).toSeq`: the last of each kind of inlining,
/// specialization and optimization, each flag once, links and definitions last.
pub fn canonical(attrs: Vec<Attr>) -> Vec<Attr> {
    let mut out: Vec<Attr> = Vec::new();
    for a in attrs {
        let r = rank(&a);
        let repeatable = matches!(a, Attr::Link(_) | Attr::Define(_));
        match out.iter().position(|o| !repeatable && rank(o) == r) {
            Some(i) => out[i] = a,
            None => out.push(a),
        }
    }
    out.retain(|a| !matches!(a, Attr::MayInline | Attr::MaySpecialize | Attr::UnOpt));
    out.sort_by_key(rank);
    out
}

fn rank(a: &Attr) -> u8 {
    match a {
        Attr::MayInline | Attr::InlineHint | Attr::NoInline | Attr::AlwaysInline => 0,
        Attr::MaySpecialize | Attr::NoSpecialize => 1,
        Attr::UnOpt | Attr::NoOpt | Attr::DidOpt | Attr::BailOpt(_) => 2,
        Attr::Align { .. } => 3,
        Attr::Extern { .. } => 4,
        Attr::Dyn => 5,
        Attr::Stub => 6,
        Attr::LinkCppRuntime => 7,
        Attr::Abstract => 8,
        Attr::Volatile => 9,
        Attr::Final => 10,
        Attr::SafePublish => 11,
        Attr::LinktimeResolved => 12,
        Attr::UsesIntrinsic => 13,
        Attr::Link(_) => 14,
        Attr::Define(_) => 15,
    }
}
