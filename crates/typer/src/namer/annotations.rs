// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/PostTyper.scala
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//! Annotations of definitions: their classes resolved when the definitions are completed
//! (dotty's `Namer.addAnnotations`), those refused, and the checks of those the typer acts on.

use crate::ctx::Ctx;
use crate::typer::Typer;
use syntax::ast::constant::Constant;
use syntax::ast::flags as f;
use syntax::ast::{NodeId, Tree};
use tasty::{Kind, SymbolId};

/// The annotation classes refused: Scala Native's code-generation annotations, and those whose
/// meaning dotty's later phases implement and the lowering does not.
const REFUSED: [&str; 23] = [
    "scala.native",
    "scala.deprecatedName",
    "scala.annotation.compileTimeOnly",
    "scala.beans.BeanProperty",
    "scala.beans.BooleanBeanProperty",
    "scala.scalanative.annotation.align",
    "scala.scalanative.annotation.alwaysinline",
    "scala.scalanative.annotation.nonExtern",
    "scala.scalanative.annotation.nooptimize",
    "scala.scalanative.annotation.nospecialize",
    "scala.scalanative.annotation.safePublish",
    "scala.scalanative.annotation.stub",
    "scala.scalanative.reflect.annotation.EnableReflectiveInstantiation",
    "scala.scalanative.runtime.struct",
    "scala.scalanative.unsafe.define",
    "scala.scalanative.unsafe.exportAccessors",
    "scala.scalanative.unsafe.exported",
    "scala.scalanative.unsafe.link",
    "scala.scalanative.unsafe.linkCppRuntime",
    "scala.scalanative.unsafe.name",
    "scala.scalanative.unsafe.package$.blocking",
    "scala.scalanative.unsafe.package$.extern",
    "scala.scalanative.unsafe.resolvedAtLinktime",
];

pub const STATIC: &str = "scala.annotation.static";

impl Typer<'_> {
    /// Records the classes of the annotations of definition `tree` on its symbol `sym` in the
    /// universe, and the annotations to type in `ctx`; a `@targetName` names `sym` at run time.
    pub fn add_annotations(&mut self, sym: SymbolId, tree: NodeId, ctx: Ctx) {
        let (Tree::ValDef { mods, .. } | Tree::DefDef { mods, .. } | Tree::TypeDef { mods, .. }) =
            *self.ast.tree(tree)
        else {
            return;
        };
        let annots = self.ast.items(self.ast.modifiers(mods).annotations).to_vec();
        if annots.is_empty() {
            return;
        }
        let ctx = self.annot_ctx(sym, ctx);
        for annot in annots {
            let Some(class) = self.annotation_class(annot, ctx) else {
                self.unsupported(annot, "annotation");
                continue;
            };
            let name = self.u.full_name(class);
            if REFUSED.contains(&name.as_str()) {
                self.unsupported(annot, &format!("annotation @{name}"));
                continue;
            }
            match name.as_str() {
                "scala.annotation.targetName" => self.enter_target_name(sym, annot),
                "scala.annotation.unroll" => self.check_unroll(sym, annot),
                "scala.annotation.implicitNotFound" if self.string_arg(annot).is_none() => {
                    self.unsupported(annot, "@implicitNotFound without a string literal");
                }
                _ => {}
            }
            self.u.annotate(sym, class);
            if self.defn.main_annot(self.u) == Ok(class) {
                self.syms.main_annotations.insert(sym, self.ast.span(annot));
            }
            if !self.syms.annotation_trees.iter().any(|&(s, a, _)| (s, a) == (sym, annot)) {
                self.syms.annotation_trees.push((sym, annot, ctx));
            }
        }
    }

    /// The class of annotation `annot`, its type tree typed in `ctx` (dotty's
    /// `typedAheadAnnotationClass`).
    fn annotation_class(&mut self, annot: NodeId, ctx: Ctx) -> Option<SymbolId> {
        let mut t = annot;
        while let Tree::Apply { fun: inner, .. } | Tree::Select { qualifier: inner, .. } = *self.ast.tree(t) {
            t = inner;
        }
        let Tree::New { tpt } = *self.ast.tree(t) else { return None };
        let typed = self.typed_type(tpt, ctx);
        let tpe = self.tpd.tpe(typed);
        self.u.class_symbol(tpe)
    }

    /// Gives method or value `sym` the run-time name of `@targetName` annotation `annot`, whose
    /// argument must be a nonempty string literal.
    fn enter_target_name(&mut self, sym: SymbolId, annot: NodeId) {
        let name = self.string_arg(annot).filter(|n| !n.is_empty());
        let s = self.u.symbol(sym);
        let in_class = s.owner.is_some_and(|o| self.u.symbol(o).kind == Kind::Class);
        let member = in_class && matches!(s.kind, Kind::Def | Kind::Val) && s.name != "<init>";
        match name {
            Some(n) if member && !self.u.is_module(sym) => self.u.set_target(sym, &n),
            _ => {
                self.unsupported(annot, "@targetName other than a string on a method or value member");
            }
        }
    }

    /// The string literal that is the only argument of annotation `annot`.
    fn string_arg(&self, annot: NodeId) -> Option<String> {
        let Tree::Apply { args, .. } = *self.ast.tree(annot) else { return None };
        let &[arg] = self.ast.items(args) else { return None };
        match self.ast.tree(arg) {
            Tree::Literal { value: Constant::String(s) } => String::from_utf16(s).ok(),
            _ => None,
        }
    }

    /// Refuses `@unroll` annotation `annot` of `sym` unless `sym` is a parameter with a default
    /// argument of a constructor or of a final member method (dotty's `PostTyper` checks).
    fn check_unroll(&mut self, sym: SymbolId, annot: NodeId) {
        let flags = self.syms.flags.get(&sym).copied().unwrap_or_default();
        if flags.is(f::PARAM_ACCESSOR) {
            return;
        }
        let has_default = flags.is(f::HAS_DEFAULT);
        let method = self.u.symbol(sym).owner.filter(|_| self.u.symbol(sym).kind == Kind::Param);
        let class =
            method.and_then(|m| self.u.symbol(m).owner).filter(|&c| self.u.symbol(c).kind == Kind::Class);
        let unrollable = method.zip(class).is_some_and(|(m, c)| {
            let final_method = self.syms.flags.get(&m).is_some_and(|fl| fl.is(f::FINAL));
            let final_class = self.u.symbol(c).flags.has(tasty::tags::FINAL) || self.u.is_module_class(c);
            self.u.symbol(m).name == "<init>" || final_method || final_class
        });
        if !(has_default && unrollable) {
            self.unsupported(annot, "@unroll other than on a defaulted parameter of a final method");
        }
    }
}
