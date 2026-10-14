//! Annotations of definitions: their classes resolved when the definitions are completed
//! (dotty's `Namer.addAnnotations`), the annotations typed as `new` expressions once the unit
//! is typed (`Typer.typedAnnotation`, `Checking.checkAnnotArgs`).

use crate::ctx::Ctx;
use crate::typer::{Pt, Typer};
use syntax::ast::constant::Constant;
use syntax::ast::flags as f;
use syntax::ast::{NodeId, Tree};
use tasty::{Kind, SymbolId};

/// The annotation classes accepted: `@main`, `@targetName`, `@static` (its member kept a member
/// of the object), and those that change nothing a program run by the VM does.
const ACCEPTED: [&str; 18] = [
    "scala.main",
    "scala.annotation.targetName",
    STATIC,
    "scala.noinline",
    "scala.inline",
    "scala.annotation.tailrec",
    "scala.annotation.unused",
    "scala.annotation.switch",
    "scala.specialized",
    "scala.deprecated",
    "scala.throws",
    "scala.transient",
    "scala.volatile",
    "scala.annotation.experimental",
    "scala.annotation.publicInBinary",
    "scala.annotation.unroll",
    "scala.annotation.elidable",
    "scala.annotation.implicitNotFound",
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
        for annot in annots {
            let Some(class) = self.annotation_class(annot, ctx) else {
                self.unsupported(annot, "annotation");
                continue;
            };
            let name = self.u.full_name(class);
            if !ACCEPTED.contains(&name.as_str()) {
                self.unsupported(annot, "annotation");
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
            if class == self.defn.main_annot {
                self.syms.main_annotations.insert(sym, self.ast.span(annot));
            }
            if !self.syms.annotation_trees.iter().any(|&(_, a, _)| a == annot) {
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

    /// Types the annotations of the unit's definitions in their contexts.
    pub fn typed_annotations(&mut self) {
        let mut i = 0;
        while let Some(&(sym, annot, ctx)) = self.syms.annotation_trees.get(i) {
            i += 1;
            let typed = self.typed(annot, Pt::Any, ctx);
            let tpe = self.tpd.tpe(typed);
            if self.u.class_symbol(tpe).is_some_and(|c| self.u.full_name(c) == STATIC) {
                self.check_static(sym, annot);
            }
        }
        self.syms.annotation_trees.clear();
    }
}
