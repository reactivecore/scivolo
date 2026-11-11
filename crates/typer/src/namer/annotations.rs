// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/PostTyper.scala
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Annotations of definitions: their classes resolved when the definitions are completed
//! (dotty's `Namer.addAnnotations`), those refused, and the checks of those the typer acts on.

use crate::ctx::Ctx;
use crate::typer::Typer;
use syntax::ast::constant::Constant;
use syntax::ast::flags as f;
use syntax::ast::{NodeId, Tree};
use tasty::{Kind, SymbolId, nme};

/// The annotation classes refused: those whose meaning dotty's later phases implement and the
/// lowering does not. A backend's own annotations are its code generator's to read or refuse.
const REFUSED: [&str; 3] = ["scala.native", "scala.beans.BeanProperty", "scala.beans.BooleanBeanProperty"];

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
            let Some((class, metas)) = self.annotation_class(annot, ctx) else {
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
                _ => {}
            }
            if !self.u.annotations(sym).contains(&class) {
                metas.into_iter().for_each(|m| self.u.annotate_meta(sym, class, m));
            }
            self.u.annotate(sym, class);
            if self.defn.main_annot(self.u) == Ok(class) {
                self.syms.main_annotations.insert(sym, self.ast.span(annot));
            }
            if !self.syms.annotation_trees.iter().any(|&(s, a, _)| (s, a) == (sym, annot)) {
                self.syms.annotation_trees.push((sym, annot, ctx));
                if self.has_args(annot) {
                    let arg = self.string_arg(annot);
                    self.u.annotate_string(sym, class, arg);
                }
            }
        }
    }

    /// The class of annotation `annot`, its type tree typed in `ctx` (dotty's
    /// `typedAheadAnnotationClass`).
    fn annotation_class(&mut self, annot: NodeId, ctx: Ctx) -> Option<(SymbolId, Vec<SymbolId>)> {
        let mut t = annot;
        while let Tree::Apply { fun: inner, .. }
        | Tree::TypeApply { fun: inner, .. }
        | Tree::Select { qualifier: inner, .. } = *self.ast.tree(t)
        {
            t = inner;
        }
        let Tree::New { tpt } = *self.ast.tree(t) else { return None };
        let typed = self.typed_type(tpt, ctx);
        let mut tpe = self.tpd.tpe(typed);
        let mut metas = Vec::new();
        while let Some((inner, meta)) = self.u.annotated(tpe) {
            metas.extend(self.u.class_symbol(meta));
            tpe = inner;
        }
        let tpe = self.dealiased(tpe);
        Some((self.u.class_symbol(tpe)?, metas))
    }

    /// Gives method or value `sym` the run-time name of `@targetName` annotation `annot`, whose
    /// argument must be a nonempty string literal.
    fn enter_target_name(&mut self, sym: SymbolId, annot: NodeId) {
        let name = self.string_arg(annot).filter(|n| !n.is_empty());
        let s = self.u.symbol(sym);
        let in_class = s.owner.is_some_and(|o| self.u.symbol(o).kind == Kind::Class);
        let member = in_class && matches!(s.kind, Kind::Def | Kind::Val) && s.name != nme::CONSTRUCTOR;
        match name {
            Some(n) if member && !self.u.is_module(sym) => self.u.set_target(sym, &n),
            _ => {
                self.unsupported(annot, "@targetName other than a string on a method or value member");
            }
        }
    }

    /// Whether annotation `annot` is applied to arguments.
    fn has_args(&self, annot: NodeId) -> bool {
        let mut t = annot;
        while let Tree::Apply { fun, args } = *self.ast.tree(t) {
            if !self.ast.items(args).is_empty() {
                return true;
            }
            t = fun;
        }
        false
    }

    /// The string literal that is the only argument of annotation `annot`, named or not.
    fn string_arg(&self, annot: NodeId) -> Option<String> {
        let Tree::Apply { args, .. } = *self.ast.tree(annot) else { return None };
        let &[arg] = self.ast.items(args) else { return None };
        self.string_value(arg)
    }

    /// The string of literal argument `arg`, named or not.
    fn string_value(&self, mut arg: NodeId) -> Option<String> {
        if let Tree::NamedArg { arg: named, .. } = *self.ast.tree(arg) {
            arg = named;
        }
        match self.ast.tree(arg) {
            Tree::Literal { value: Constant::String(s) } => String::from_utf16(s).ok(),
            _ => None,
        }
    }

    /// Refuses parameter clause `params` of a method if a `@deprecatedName` repeats a name of an
    /// earlier parameter or deprecated name, or a parameter repeats an earlier deprecated name
    /// (dotty's `typedDefDef` checks).
    pub fn check_deprecated_names(&mut self, params: &[SymbolId]) {
        const DEPRECATED_NAME: &str = "scala.deprecatedName";
        if !params.iter().any(|&p| self.u.is_annotated(p, DEPRECATED_NAME)) {
            return;
        }
        let mut seen: Vec<String> = Vec::new();
        for &p in params {
            let name = self.u.symbol(p).name.clone();
            let tree = self.syms.param_trees.get(&p).copied();
            if seen.contains(&name)
                && let Some(tree) = tree
            {
                self.unsupported(tree, "parameter name equal to a deprecated name");
            }
            let annots: Vec<(NodeId, Ctx)> =
                self.syms.annotation_trees.iter().filter(|a| a.0 == p).map(|a| (a.1, a.2)).collect();
            for (annot, ctx) in annots {
                if self
                    .annotation_class(annot, ctx)
                    .is_none_or(|(c, _)| self.u.full_name(c) != DEPRECATED_NAME)
                {
                    continue;
                }
                let deprecated = match self.deprecated_name_arg(annot) {
                    Some(arg) => self.string_value(arg).unwrap_or_default(),
                    None => "<none>".to_owned(),
                };
                if seen.contains(&deprecated) {
                    self.unsupported(annot, "deprecated parameter name equal to another name");
                }
                seen.push(deprecated);
            }
            seen.push(name);
        }
    }

    /// The `name` argument of `@deprecatedName` annotation `annot`: its first positional argument
    /// or the one named `name`.
    fn deprecated_name_arg(&self, annot: NodeId) -> Option<NodeId> {
        let Tree::Apply { args, .. } = *self.ast.tree(annot) else { return None };
        let args = self.ast.items(args);
        args.iter().copied().find(|&a| match *self.ast.tree(a) {
            Tree::NamedArg { name, .. } => self.text(name.name) == "name",
            _ => a == args[0],
        })
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
            self.u.symbol(m).name == nme::CONSTRUCTOR || final_method || final_class
        });
        if !(has_default && unrollable) {
            self.unsupported(annot, "@unroll other than on a defaulted parameter of a final method");
        }
    }
}
