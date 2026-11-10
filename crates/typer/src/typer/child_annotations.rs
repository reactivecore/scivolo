// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//   compiler/src/dotty/tools/dotc/core/Annotations.scala
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//! The children of sealed classes (dotty's `children`) and the `@Child` annotations dotty's
//! `Namer` gives them (`addChild`, `Annotation.Child`), for their TASTy.

use super::Typer;
use crate::defn::type_ref;
use crate::sealed;
use crate::tpd::{TNodeId, Tree as T};
use rustc_hash::FxHashMap;
use syntax::ast::Span;
use tasty::tags::{ENUM, SEALED};
use tasty::{Kind, SymbolId, Type};

const CHILD: &str = "scala.annotation.internal.Child";
const SOURCE_FILE: &str = "scala.annotation.internal.SourceFile";
const EXPERIMENTAL: &str = "scala.annotation.experimental";

impl Typer<'_> {
    /// The annotation trees of each definition, a sealed class under `root` with `@Child[C]()`
    /// for each child `C` after the annotations `PostTyper` adds, the children last defined
    /// first, the class itself for those not accessible.
    pub fn with_child_annotations(
        &mut self,
        root: TNodeId,
    ) -> Result<FxHashMap<SymbolId, Vec<TNodeId>>, &'static str> {
        let mut out = self.syms.annotations.clone();
        let mut todo = vec![root];
        while let Some(t) = todo.pop() {
            todo.extend(self.tpd.children(t).iter().rev());
            let Some(s) = self.tpd.sym(t).filter(|_| matches!(self.tpd.tree(t), T::TypeDef { .. })) else {
                continue;
            };
            if !self.u.is_class(s) || !self.u.symbol(s).flags.has(SEALED) {
                continue;
            }
            let annots = out.entry(s).or_insert_with(|| self.tpd.annotations(s).to_vec());
            let added_later = annots.iter().all(|&a| {
                let class = self.u.class_symbol(self.tpd.tpe(a)).map(|c| self.u.full_name(c));
                class.as_deref() == Some(SOURCE_FILE)
                    || class.as_deref() == Some(EXPERIMENTAL) && !self.tpd.span(a).exists()
            });
            if !added_later {
                return Err("an annotated sealed class");
            }
            let at = Span::at(self.tpd.span(t).point());
            let mut children = Vec::new();
            for (i, c) in self.sealed_children(s)?.into_iter().enumerate() {
                let child = if self.is_accessible_child(c, s) { c } else { s };
                let class = self.u.module_class(child).unwrap_or(child);
                let start = self.syms.def_trees.get(&class).map(|&n| self.ast.span(n).start());
                if !children.iter().any(|&(_, k)| k == child) {
                    children.push(((start, i), child));
                }
            }
            children.sort_by_key(|&(start, _)| std::cmp::Reverse(start));
            let trees: Vec<TNodeId> =
                children.into_iter().map(|(_, c)| self.child_annotation(c, at)).collect::<Result<_, _>>()?;
            out.get_mut(&s).expect("the entry made above").extend(trees);
        }
        Ok(out)
    }

    /// Lists the direct subclasses of sealed class or trait `s` in the order of their
    /// definitions, an object by its value; for an enum, its cases. For a sealed class of the
    /// library, the children its TASTy names.
    pub fn sealed_children(&mut self, s: SymbolId) -> Result<Vec<SymbolId>, &'static str> {
        if self.u.symbol(s).flags.has(ENUM) {
            return Ok(sealed::enum_cases(self.u, s));
        }
        let Some(&def) = self.syms.def_trees.get(&s) else {
            let types = self.u.decoded_children(s).ok_or("sum Mirror of a sealed class without TASTy")?;
            return types
                .into_iter()
                .map(|t| self.referenced(t).ok_or("sealed child outside the subset"))
                .collect();
        };
        let file = self.file_of(self.ast.span(def));
        let defs: Vec<(SymbolId, syntax::ast::NodeId)> =
            self.syms.def_trees.iter().map(|(&k, &v)| (k, v)).collect();
        let mut classes = Vec::new();
        for (c, node) in defs {
            let span = self.ast.span(node);
            if self.u.symbol(c).kind == Kind::Class && self.file_of(span) == file {
                self.complete(c);
                classes.push((span.start(), c));
            }
        }
        Ok(sealed::direct_children(self.u, s, &classes))
    }

    /// Whether child `c` of sealed class `parent` is neither anonymous nor local to a term the
    /// parent is not (dotty's `isInaccessibleChildOf`).
    fn is_accessible_child(&self, c: SymbolId, parent: SymbolId) -> bool {
        let is_type = |s: SymbolId| self.u.symbol(s).kind.is_type() && self.u.symbol(s).kind != Kind::Package;
        let owner = |s: SymbolId| self.u.symbol(s).owner;
        let mut base = owner(parent);
        while let Some(b) = base.filter(|&b| is_type(b)) {
            base = owner(b);
        }
        let mut s = owner(c);
        while let Some(o) = s {
            if Some(o) == base || self.u.symbol(o).kind == Kind::Package {
                return !self.u.symbol(c).name.starts_with("$anon");
            }
            s = owner(o).filter(|_| is_type(o));
        }
        false
    }

    /// `new Child[c.type]()` of object or value `c`, `new Child[c]()` of class `c`, at `at`.
    fn child_annotation(&mut self, c: SymbolId, at: Span) -> Result<TNodeId, &'static str> {
        let (pkg, name) = CHILD.rsplit_once('.').expect("a qualified name");
        let package = self.u.package(pkg).ok_or("scala.annotation.internal missing")?;
        let classes = self.u.decl(package, name).unwrap_or_default();
        let class = classes.into_iter().find(|&s| self.u.is_class(s)).ok_or("Child missing")?;
        let ctor = self.u.decl(class, "<init>").unwrap_or_default().into_iter().next();
        let ctor = ctor.ok_or("Child without a constructor")?;
        let tycon = type_ref(self.u, class);
        let child = type_ref(self.u, c);
        let applied = self.u.intern_type(Type::Applied(tycon, vec![child]));
        let tpt = self.type_tree(tycon, at, false);
        let new = self.tpd.node(T::New { tpt }, at, Some(tycon), None);
        let select = self.build().select(new, ctor, at);
        let arg = self.type_tree(child, at, false);
        let targs = self.build().type_apply(select, &[arg], applied, at);
        Ok(self.build().apply(targs, &[], applied, at))
    }
}
