// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Checking.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! The annotations of definitions typed as `new` expressions once their package's statements
//! are typed (dotty's `annotContext`, `typedAnnotation`, `Checking.checkAnnotClass`).

use crate::ctx::{Ctx, Level};
use crate::defn::{STATIC_ANNOT, type_ref};
use crate::tpd::{TNodeId, Tree as T};
use crate::typer::{Pt, Typer};
use rustc_hash::FxHashMap;
use syntax::ast::{NodeId, Tree as S};
use syntax::desugar::witness_names;
use tasty::{Kind, SymbolId, Type};

impl Typer<'_> {
    /// The context of the annotations of `sym`, defined in `ctx` (dotty's `annotContext`): for a
    /// parameter the context outside its method, with the method's type parameters only; owned by
    /// a local dummy of `sym`'s owner where that context's owner is a class or package.
    pub fn annot_ctx(&mut self, sym: SymbolId, ctx: Ctx) -> Ctx {
        let Some(owner) = self.u.symbol(sym).owner else { return ctx };
        let is_param = matches!(self.u.symbol(sym).kind, Kind::Param | Kind::TypeParam) && ctx.owner == owner;
        let (outer, tparams) = match self.levels.outer(ctx.level).filter(|_| is_param) {
            Some(level) => {
                let tparams: Vec<(String, SymbolId)> = (self.levels.scope_symbols(ctx.level).into_iter())
                    .filter(|&p| self.u.is_type_symbol(p))
                    .map(|p| (self.u.symbol(p).name.clone(), p))
                    .collect();
                let outer_owner = self.u.symbol(owner).owner.unwrap_or(owner);
                (Ctx { owner: outer_owner, level, ..ctx }, tparams)
            }
            None => (ctx, Vec::new()),
        };
        let in_members = matches!(self.u.symbol(outer.owner).kind, Kind::Class | Kind::Package);
        if tparams.is_empty() && !in_members {
            return outer;
        }
        let dummy = self.local_dummy(owner);
        let level = if tparams.is_empty() {
            outer.level
        } else {
            self.levels.push(Some(outer.level), Level::Scope(tparams))
        };
        Ctx { owner: dummy, level, ..outer }
    }

    /// Types the annotations of the unit's definitions not typed yet in their contexts, an
    /// annotation two definitions share once, in the context of the first one queued, and keeps
    /// them on their symbols.
    pub fn typed_annotations(&mut self) {
        let mut typed_trees: FxHashMap<NodeId, Option<TNodeId>> = FxHashMap::default();
        let mut i = 0;
        while let Some(&(sym, annot, ctx)) = self.syms.annotation_trees.get(i) {
            i += 1;
            let typed = match typed_trees.get(&annot) {
                Some(&t) => t,
                None => {
                    let typed = self.in_file_of(annot, |t| t.typed_annotation(annot, ctx));
                    let t = (!self.is_error(typed) && self.check_annot_class(annot, typed)).then_some(typed);
                    if t.is_some() {
                        self.check_annot_args(typed);
                        self.witness_names_literal(annot, typed);
                    }
                    *typed_trees.entry(annot).or_insert(t)
                }
            };
            let Some(typed) = typed else { continue };
            let tpe = self.tpd.tpe(typed);
            if self.u.is_class_named(tpe, STATIC_ANNOT) {
                self.check_static(sym, annot);
            }
            self.syms.annotations.entry(sym).or_default().push(typed);
            self.tpd.annotate(sym, typed);
        }
        self.syms.annotation_trees.clear();
    }

    /// Passes the names of `@WitnessNames` annotation `typed`, of tree `annot`, as the bare
    /// sequence of `java.lang.String`s dotty's `WitnessNamesAnnot` builds.
    fn witness_names_literal(&mut self, annot: NodeId, typed: TNodeId) {
        if witness_names(self.ast, self.names, annot).is_none() {
            return;
        }
        let T::Apply { fun, args } = *self.tpd.tree(typed) else { return };
        let &[arg] = self.tpd.items(args) else { return };
        let T::Typed { expr, .. } = *self.tpd.tree(arg) else { return };
        let T::SeqLiteral { elemtpt, .. } = *self.tpd.tree(expr) else { return };
        let Some(string) = self.u.class_symbol(self.tpd.tpe(elemtpt)) else { return };
        self.tpd.get_mut(elemtpt).tpe = Some(type_ref(self.u, string));
        self.tpd.get_mut(expr).span = self.tpd.span(typed);
        let args = self.tpd.list(&[expr]);
        self.tpd.get_mut(typed).tree = T::Apply { fun, args };
    }

    /// Types annotation tree `annot`, the `new`s within a Java annotation unchecked.
    pub fn typed_annotation(&mut self, annot: NodeId, ctx: Ctx) -> TNodeId {
        let new = self.annotation_new(annot);
        self.annotation_news.insert(new, annot);
        self.typed(annot, Pt::Any, ctx)
    }

    /// Marks the `new`s in annotation tree `annot` of a Java annotation class unchecked
    /// (dotty's `PostTyper.inJavaAnnot`).
    pub fn uncheck_java_annotation_news(&mut self, annot: NodeId) {
        let news = self.ast.subtrees(annot, &|a, n| matches!(a.tree(n), S::New { .. }));
        self.java_annotation_news.extend(news);
    }

    /// The `new` of annotation tree `annot`, the tree itself if it has none.
    fn annotation_new(&self, annot: NodeId) -> NodeId {
        let mut t = annot;
        loop {
            match *self.ast.tree(t) {
                S::Apply { fun, .. } | S::TypeApply { fun, .. } => t = fun,
                S::Select { qualifier, .. } => t = qualifier,
                _ => return t,
            }
        }
    }

    /// Whether typed annotation `typed` of tree `annot` is of a class that extends
    /// `scala.annotation.Annotation` or of a Java annotation class; reports it if not.
    fn check_annot_class(&mut self, annot: NodeId, typed: TNodeId) -> bool {
        let tpe = self.tpd.tpe(typed);
        let java = self.u.class_symbol(tpe).is_some_and(|c| self.u.classfile_retention(c).is_some());
        if java || self.derives_from_named(tpe, "scala.annotation.Annotation") {
            return true;
        }
        let name = self.u.class_symbol(tpe).map(|c| self.u.symbol(c).name.clone()).unwrap_or_default();
        let msg = format!(
            "class {name} is not a valid Scala annotation: it does not extend `scala.annotation.Annotation`"
        );
        self.error(annot, msg);
        false
    }

    /// Reports the argument of typed annotation `typed` if the annotation is an `@implicitNotFound`
    /// or `@implicitAmbiguous` and the argument is not constant (dotty's `checkAnnotArgs`).
    fn check_annot_args(&mut self, typed: TNodeId) {
        let tpe = self.tpd.tpe(typed);
        let Some(class) = self.u.class_symbol(tpe) else { return };
        let name = self.u.full_name(class);
        let Some(short) = name.strip_prefix("scala.annotation.") else { return };
        if !matches!(short, "implicitNotFound" | "implicitAmbiguous") {
            return;
        }
        let T::Apply { args, .. } = self.tpd.tree(typed) else { return };
        let &[arg] = self.tpd.items(*args) else { return };
        let mut t = self.tpd.tpe(arg);
        loop {
            let term = match *self.u.ty(t) {
                Type::TermRef(..) => true,
                Type::Sym(_, s) => self.u.symbol(s).kind.is_term(),
                _ => false,
            };
            match term.then(|| self.u.widen_once(t)).flatten() {
                Some(w) if w != t => t = w,
                _ => break,
            }
        }
        if !matches!(self.u.ty(t), Type::Constant(..) | Type::Str(_)) {
            self.error_at(
                self.tpd.span(arg),
                format!("@{short} requires constant expressions as a parameter"),
            );
        }
    }
}
