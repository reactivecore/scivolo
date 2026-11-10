// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//   compiler/src/dotty/tools/dotc/core/Annotations.scala
//! The `@Child` annotations dotty's `Namer` gives sealed classes (`addChild`,
//! `Annotation.Child`), for their TASTy.

use super::Typer;
use crate::defn::type_ref;
use crate::tpd::{TNodeId, Tree as T};
use rustc_hash::FxHashMap;
use syntax::ast::Span;
use tasty::tags::SEALED;
use tasty::{Kind, SymbolId, Type, Universe};

const CHILD: &str = "scala.annotation.internal.Child";
const SOURCE_FILE: &str = "scala.annotation.internal.SourceFile";
const EXPERIMENTAL: &str = "scala.annotation.experimental";

impl Typer<'_> {
    /// Collects the annotation trees of each definition for its TASTy: a sealed class under
    /// `root` gets `@Child[C]()` for each child `C` after the annotations `PostTyper` adds, the
    /// children last defined first, the class itself for a child local to a method or anonymous.
    /// A sealed class with annotations of its own is not supported.
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
                let child = if is_accessible_child(self.u, c, s) { c } else { s };
                let class = self.u.module_class(child).unwrap_or(child);
                let start = self.syms.def_trees.get(&class).map(|&n| self.ast.span(n).start());
                if !children.iter().any(|&(_, k)| k == child) {
                    children.push(((start, i), child));
                }
            }
            children.sort_by_key(|&(start, _)| std::cmp::Reverse(start));
            let trees: Vec<TNodeId> =
                children.into_iter().map(|(_, c)| child_annotation(self, c, at)).collect::<Result<_, _>>()?;
            out.get_mut(&s).expect("the entry made above").extend(trees);
        }
        Ok(out)
    }
}

/// Whether child `c` of sealed class `parent` is neither anonymous nor local to a term the
/// parent is not (dotty's `isInaccessibleChildOf`).
fn is_accessible_child(u: &Universe, c: SymbolId, parent: SymbolId) -> bool {
    let is_type = |s: SymbolId| u.symbol(s).kind.is_type() && u.symbol(s).kind != Kind::Package;
    let owner = |s: SymbolId| u.symbol(s).owner;
    let mut base = owner(parent);
    while let Some(b) = base.filter(|&b| is_type(b)) {
        base = owner(b);
    }
    let mut s = owner(c);
    while let Some(o) = s {
        if Some(o) == base || u.symbol(o).kind == Kind::Package {
            return !u.symbol(c).name.starts_with("$anon");
        }
        s = owner(o).filter(|_| is_type(o));
    }
    false
}

/// `new Child[c.type]()` of object or value `c`, `new Child[c]()` of class `c`, at `at`.
fn child_annotation(t: &mut Typer, c: SymbolId, at: Span) -> Result<TNodeId, &'static str> {
    let (pkg, name) = CHILD.rsplit_once('.').expect("a qualified name");
    let package = t.u.package(pkg).ok_or("scala.annotation.internal missing")?;
    let classes = t.u.decl(package, name).unwrap_or_default();
    let class = classes.into_iter().find(|&s| t.u.is_class(s)).ok_or("Child missing")?;
    let ctor = t.u.decl(class, "<init>").unwrap_or_default().into_iter().next();
    let ctor = ctor.ok_or("Child without a constructor")?;
    let tycon = type_ref(t.u, class);
    let child = type_ref(t.u, c);
    let applied = t.u.intern_type(Type::Applied(tycon, vec![child]));
    let tpt = t.type_tree(tycon, at, false);
    let new = t.tpd.node(T::New { tpt }, at, Some(tycon), None);
    let select = t.build().select(new, ctor, at);
    let arg = t.type_tree(child, at, false);
    let targs = t.build().type_apply(select, &[arg], applied, at);
    Ok(t.build().apply(targs, &[], applied, at))
}
