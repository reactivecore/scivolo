// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/PostTyper.scala
//   compiler/src/dotty/tools/dotc/typer/Checking.scala
//! The annotations dotty's `PostTyper` gives the top-level definitions of a source file:
//! `@SourceFile` to classes, `@experimental` under a top-level experimental language import or
//! setting (`checkAndAdaptExperimentalImports`).

use super::Typer;
use crate::defn::type_ref;
use crate::tpd::{List, TNodeId, Tree as T};
use syntax::ast::Span;
use syntax::ast::constant::Constant;
use tasty::{Kind, SymbolId};

const SOURCE_FILE: &str = "scala.annotation.internal.SourceFile";
const EXPERIMENTAL: &str = "scala.annotation.experimental";
/// The experimental features whose imports do not make the unit experimental (dotty's
/// `nonViralExperimentalFeatures` and `scala2macros`).
const NOT_VIRAL: [&str; 4] = ["captureChecking", "separationChecking", "safe", "macros"];

/// The top-level definitions of one source file as dotty annotates them: each gets
/// `@experimental` under a top-level `import scala.language.experimental.namedTypeArguments` or
/// the `-experimental` setting, each top-level class `@SourceFile("path")`. The members of a
/// package object count as top-level definitions.
#[derive(Default)]
pub struct TopLevel {
    pub defs: Vec<SymbolId>,
    /// Each class with the span of its definition.
    pub classes: Vec<(SymbolId, Span)>,
    /// The experimental language imports, as dotty shows them.
    pub imports: Vec<String>,
}

impl Typer<'_> {
    /// Annotates the top-level definitions of typed source files `files`, named by their index
    /// in `paths`.
    pub fn top_level_annotations(&mut self, files: &[TNodeId], paths: &[String]) {
        for &f in files {
            if let Some(path) = paths.get(self.file_of(self.tpd.span(f))) {
                TopLevel::of(self, f).annotate(self, path);
            }
        }
    }
}

impl TopLevel {
    /// Collects the top-level definitions of typed source file `root`.
    pub fn of(t: &Typer, root: TNodeId) -> TopLevel {
        let mut top = TopLevel::default();
        top.collect(t, root);
        top
    }

    /// Annotates the definitions, of a source file named `path`.
    pub fn annotate(self, t: &mut Typer, path: &str) {
        let why = match self.imports.first() {
            Some(import) => Some(format!("Added by top level {import}")),
            None => t.experimental.map(|setting| format!("Added by {setting}")),
        };
        if let Some(why) = why {
            for &s in &self.defs {
                if !is_experimental(t, s) {
                    prepend_annotation(t, s, EXPERIMENTAL, &why, Span::NONE);
                }
            }
        }
        for (s, at) in self.classes {
            prepend_annotation(t, s, SOURCE_FILE, path, at);
        }
    }

    fn collect(&mut self, t: &Typer, tree: TNodeId) {
        let member = |d: &T| matches!(d, T::TypeDef { .. } | T::ValDef { .. } | T::DefDef { .. });
        match *t.tpd.tree(tree) {
            T::PackageDef { stats, .. } => {
                for &s in t.tpd.items(stats) {
                    self.collect(t, s);
                }
            }
            T::Import { expr, selectors, export: false } => {
                self.imports.extend(experimental_import(t, expr, selectors));
            }
            ref d if member(d) => {
                let Some(s) = t.tpd.sym(tree) else { return };
                let owner = t.u.symbol(s).owner;
                if t.u.is_class(s) && owner.is_some_and(|o| t.u.symbol(o).kind == Kind::Package) {
                    self.classes.push((s, t.tpd.span(tree)));
                }
                match t.tpd.class_template(tree) {
                    Some(template) if t.u.is_package_object(s) => {
                        let members = t.tpd.items(template.body).iter().filter(|&&d| member(t.tpd.tree(d)));
                        self.defs.extend(members.filter_map(|&d| t.tpd.sym(d)));
                    }
                    _ => self.defs.push(s),
                }
            }
            _ => {}
        }
    }
}

/// `import scala.language.experimental.f` as dotty shows it, if `f` makes the unit experimental.
fn experimental_import(t: &Typer, expr: TNodeId, selectors: List) -> Option<String> {
    let site = t.import_site(t.tpd.tpe(expr))?;
    if t.u.full_name(site).replace('$', "") != "scala.language.experimental" {
        return None;
    }
    let (mut names, mut viral) = (Vec::new(), false);
    for &s in t.tpd.items(selectors) {
        let T::ImportSelector { imported, renamed, .. } = *t.tpd.tree(s) else { return None };
        let name = ident_text(t, imported)?;
        viral |= !NOT_VIRAL.contains(&name.as_str());
        names.push(match renamed.and_then(|r| ident_text(t, r)) {
            Some(r) => format!("{name} as {r}"),
            None => name,
        });
    }
    if !viral {
        return None;
    }
    let path = import_path(t, expr)?;
    Some(match &names[..] {
        [one] if !one.contains(' ') => format!("import {path}.{one}"),
        _ => format!("import {path}.{{{}}}", names.join(", ")),
    })
}

fn ident_text(t: &Typer, id: TNodeId) -> Option<String> {
    let T::Ident { name } = *t.tpd.tree(id) else { return None };
    Some(t.names.text(name.name).to_owned())
}

/// Path `p` by its names.
fn import_path(t: &Typer, p: TNodeId) -> Option<String> {
    match *t.tpd.tree(p) {
        T::Select { qualifier, name } => {
            Some(format!("{}.{}", import_path(t, qualifier)?, t.names.text(name.name)))
        }
        _ => ident_text(t, p),
    }
}

/// Whether `s` or its owner, a class, is annotated `@experimental` (dotty's `isExperimental`).
fn is_experimental(t: &mut Typer, s: SymbolId) -> bool {
    let owner = t.u.symbol(s).owner.filter(|&o| t.u.is_class(o));
    is_annotated_experimental(t, s) || owner.is_some_and(|o| is_annotated_experimental(t, o))
}

fn is_annotated_experimental(t: &mut Typer, s: SymbolId) -> bool {
    let annots = t.syms.annotations.get(&s).cloned().unwrap_or_default();
    annots.iter().any(|&a| {
        let tpe = t.tpd.tpe(a);
        t.u.class_symbol(tpe).is_some_and(|c| t.u.full_name(c) == EXPERIMENTAL)
    })
}

/// Puts `new class(text)` of span `span` first among the annotations of `s`, its parts at the
/// start of `span` (dotty's `addAnnotation`).
fn prepend_annotation(t: &mut Typer, s: SymbolId, class: &str, text: &str, span: Span) {
    let Some((pkg, name)) = class.rsplit_once('.') else { return };
    let Some(package) = t.u.package(pkg) else { return };
    let classes = t.u.decl(package, name).unwrap_or_default();
    let Some(cls) = classes.into_iter().find(|&c| t.u.is_class(c)) else { return };
    let ctors = t.u.decl(cls, "<init>").unwrap_or_default();
    let Some(ctor) = ctors.into_iter().find(|&c| t.u.erased_method(c).0.len() == 1) else { return };
    let at = if span.exists() { span.start_pos() } else { span };
    let tpe = type_ref(t.u, cls);
    let tpt = t.type_tree(tpe, at, false);
    let new = t.tpd.node(T::New { tpt }, at, Some(tpe), None);
    let select = t.build().select(new, ctor, at);
    let arg = t.build().literal(Constant::String(text.encode_utf16().collect()), at);
    let annot = t.build().apply(select, &[arg], tpe, span);
    t.syms.annotations.entry(s).or_default().insert(0, annot);
}
