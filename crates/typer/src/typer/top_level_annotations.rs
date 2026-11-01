// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/PostTyper.scala
//   compiler/src/dotty/tools/dotc/typer/Checking.scala
//! The annotations dotty's `PostTyper` gives the top-level definitions of a source file:
//! `@SourceFile` to classes, `@experimental` under a top-level experimental language import
//! (`checkAndAdaptExperimentalImports`).

use super::Typer;
use crate::defn::type_ref;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::Span;
use syntax::ast::constant::Constant;
use tasty::{Kind, SymbolId};

const SOURCE_FILE: &str = "scala.annotation.internal.SourceFile";
const EXPERIMENTAL: &str = "scala.annotation.experimental";
/// The experimental features whose imports do not make the unit experimental (dotty's
/// `nonViralExperimentalFeatures` and `scala2macros`).
const NOT_VIRAL: [&str; 4] = ["captureChecking", "separationChecking", "safe", "macros"];

/// The top-level definitions of a source file, package objects' members for package objects, its
/// classes and its experimental language imports as dotty shows them.
#[derive(Default)]
struct TopLevel {
    defs: Vec<SymbolId>,
    classes: Vec<SymbolId>,
    imports: Vec<String>,
}

impl Typer<'_> {
    /// Annotates the top-level definitions of typed source file `root`, named `path`.
    pub fn top_level_annotations(&mut self, root: TNodeId, path: &str) {
        let mut top = TopLevel::default();
        self.top_level(root, &mut top);
        if let Some(import) = top.imports.first() {
            let why = format!("Added by top level {import}");
            for &s in &top.defs {
                if !self.is_experimental(s) {
                    self.prepend_annotation(s, EXPERIMENTAL, &why);
                }
            }
        }
        for s in top.classes {
            self.prepend_annotation(s, SOURCE_FILE, path);
        }
    }

    fn top_level(&mut self, t: TNodeId, top: &mut TopLevel) {
        let member = |t: &T| matches!(t, T::TypeDef { .. } | T::ValDef { .. } | T::DefDef { .. });
        match self.tpd.tree(t).clone() {
            T::PackageDef { stats, .. } => {
                for s in self.tpd.items(stats).to_vec() {
                    self.top_level(s, top);
                }
            }
            T::Import { expr, selectors, export: false } => {
                top.imports.extend(self.experimental_import(expr, selectors));
            }
            ref d if member(d) => {
                let Some(s) = self.tpd.sym(t) else { return };
                let owner = self.u.symbol(s).owner;
                if self.u.is_class(s) && owner.is_some_and(|o| self.u.symbol(o).kind == Kind::Package) {
                    top.classes.push(s);
                }
                match self.tpd.class_template(t) {
                    Some(template) if self.u.is_package_object(s) => {
                        let members =
                            self.tpd.items(template.body).iter().filter(|&&d| member(self.tpd.tree(d)));
                        top.defs.extend(members.filter_map(|&d| self.tpd.sym(d)).collect::<Vec<_>>());
                    }
                    _ => top.defs.push(s),
                }
            }
            _ => {}
        }
    }

    /// `import scala.language.experimental.f` as dotty shows it, if `f` makes the unit experimental.
    fn experimental_import(&mut self, expr: TNodeId, selectors: crate::tpd::List) -> Option<String> {
        let site = self.import_site(self.tpd.tpe(expr))?;
        if self.u.full_name(site).replace('$', "") != "scala.language.experimental" {
            return None;
        }
        let mut names = Vec::new();
        for s in self.tpd.items(selectors).to_vec() {
            let T::ImportSelector { imported, renamed, .. } = *self.tpd.tree(s) else { return None };
            let name = self.ident_text(imported)?;
            names.push(match renamed.and_then(|r| self.ident_text(r)) {
                Some(r) => format!("{name} as {r}"),
                None => name,
            });
        }
        if names.iter().all(|n| NOT_VIRAL.contains(&n.as_str())) {
            return None;
        }
        let path = self.import_path(expr)?;
        Some(match &names[..] {
            [one] if !one.contains(' ') => format!("import {path}.{one}"),
            _ => format!("import {path}.{{{}}}", names.join(", ")),
        })
    }

    fn ident_text(&self, t: TNodeId) -> Option<String> {
        let T::Ident { name } = *self.tpd.tree(t) else { return None };
        Some(self.names.text(name.name).to_owned())
    }

    /// Path `t` by its names.
    fn import_path(&self, t: TNodeId) -> Option<String> {
        match *self.tpd.tree(t) {
            T::Select { qualifier, name } => {
                Some(format!("{}.{}", self.import_path(qualifier)?, self.names.text(name.name)))
            }
            _ => self.ident_text(t),
        }
    }

    fn is_experimental(&mut self, s: SymbolId) -> bool {
        let annots = self.syms.annotations.get(&s).cloned().unwrap_or_default();
        annots.iter().any(|&a| {
            let tpe = self.tpd.tpe(a);
            self.u.class_symbol(tpe).is_some_and(|c| self.u.full_name(c) == EXPERIMENTAL)
        })
    }

    /// Puts `new class(text)` first among the annotations of `s` (dotty's `addAnnotation`).
    fn prepend_annotation(&mut self, s: SymbolId, class: &str, text: &str) {
        let Some((pkg, name)) = class.rsplit_once('.') else { return };
        let Some(package) = self.u.package(pkg) else { return };
        let classes = self.u.decl(package, name).unwrap_or_default();
        let Some(cls) = classes.into_iter().find(|&c| self.u.is_class(c)) else { return };
        let ctors = self.u.decl(cls, "<init>").unwrap_or_default();
        let Some(ctor) = ctors.into_iter().find(|&c| self.u.erased_method(c).0.len() == 1) else { return };
        let at = Span::NONE;
        let tpe = type_ref(self.u, cls);
        let tpt = self.type_tree(tpe, at, false);
        let new = self.tpd.node(T::New { tpt }, at, Some(tpe), None);
        let select = self.build().select(new, ctor, at);
        let arg = self.build().literal(Constant::String(text.encode_utf16().collect()), at);
        let annot = self.build().apply(select, &[arg], tpe, at);
        self.syms.annotations.entry(s).or_default().insert(0, annot);
    }
}
