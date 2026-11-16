// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/JavaParsers.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! The annotations of the members of the Java sources typed as `new` expressions, as dotty's
//! Java parser writes them (`annotation`) and its typer types them (`typedAnnotation`).

use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use crate::typer::{Pt, Typer};
use classfile::ElementValue;
use names::nme;
use syntax::ast::constant::Constant;
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::{Kind, SourceAnnotation, SourceArg, SymbolId, Type, TypeId};

/// The name of `scala.Array`.
const ARRAY: &str = "Array";

impl Typer<'_> {
    /// Types the annotations of the fields, methods and constructors of the Java sources in
    /// `ctx` and keeps them on their symbols and their export forwarders; a member with an
    /// annotation the reader refuses, or one that does not type, keeps none, its diagnostics
    /// dropped.
    pub fn typed_java_source_annotations(&mut self, ctx: Ctx) {
        for (sym, annots) in self.u.java_source_annotations() {
            let Ok(annots) = annots else { continue };
            let typed: Option<Vec<_>> = annots.iter().map(|a| self.typed_java_annotation(a, ctx)).collect();
            if let Some(typed) = typed {
                self.syms.annotations.insert(sym, typed);
            }
        }
        self.forward_java_source_annotations();
    }

    /// Annotation `a` typed in `ctx`; none, the constraint and the implicit dictionary restored,
    /// if it reports an error or a refusal.
    fn typed_java_annotation(&mut self, a: &SourceAnnotation, ctx: Ctx) -> Option<TNodeId> {
        let (reported, unsupported) = (self.diags.items.len(), self.unsupported.clone());
        let (attempt, dictionary) = (self.constraint.attempt(), self.history.snapshot());
        let tree = self.annotation_tree(a, ctx);
        let typed = tree.map(|tree| self.typed(tree, Pt::Any, ctx));
        let errors = self.diags.items[reported..].iter().any(|d| d.level == syntax::diag::Level::Error);
        if errors || self.unsupported != unsupported || typed.is_none_or(|t| self.is_error(t)) {
            self.diags.items.truncate(reported);
            self.unsupported = unsupported;
            self.constraint.abandon(attempt);
            self.history.restore(dictionary);
            return None;
        }
        typed
    }

    /// The untyped `new C(e = v, …)` of annotation `a` with its arguments for `ctx`; none if an
    /// argument has none.
    fn annotation_tree(&mut self, a: &SourceAnnotation, ctx: Ctx) -> Option<NodeId> {
        if !self.u.is_java_defined(a.class) {
            self.u.enter_jdk_annotation(a.class);
        }
        let class = self.u.intern_type(Type::Sym(None, a.class));
        let tpt = self.splice_type(class);
        let new = self.ast.node(Tree::New { tpt });
        self.annotation_news.insert(new);
        let init = TName { name: self.name(nme::CONSTRUCTOR), is_type: false };
        let fun = self.ast.node(Tree::Select { qualifier: new, name: init });
        let mut args = Vec::new();
        for (element, value) in &a.args {
            let arg = match self.single_array_element(a.class, element, value) {
                Some(elem) => self.java_seq_literal(value, elem, ctx)?,
                None => self.annotation_arg(value, ctx)?,
            };
            let name = TName { name: self.name(element), is_type: false };
            args.push(self.ast.node(Tree::NamedArg { name, arg }));
        }
        let args = self.ast.list(&args);
        Some(self.ast.node(Tree::Apply { fun, args }))
    }

    /// The element type of element `element` of annotation class `class` if it is an array and
    /// `value` is not.
    fn single_array_element(&mut self, class: SymbolId, element: &str, value: &SourceArg) -> Option<TypeId> {
        if matches!(value, SourceArg::Array(_)) {
            return None;
        }
        let method =
            self.u.decls_named(class, element).into_iter().find(|&d| self.u.symbol(d).kind == Kind::Def)?;
        let info = self.u.info(method);
        let result = self.u.result_type(info);
        if !self.u.is_array(result) {
            return None;
        }
        let Type::Applied(_, args) = *self.u.ty(result) else { return None };
        self.u.types_of(args).first().copied()
    }

    /// The array of the single element `value` of type `elem`, typed in `ctx` (dotty's
    /// `typedNamedArg` in a Java unit, a `JavaSeqLiteral`), spliced into an untyped tree.
    fn java_seq_literal(&mut self, value: &SourceArg, elem: TypeId, ctx: Ctx) -> Option<NodeId> {
        let arg = self.annotation_arg(value, ctx)?;
        let typed = self.typed(arg, Pt::Type(elem), ctx);
        let elemtpt = self.tpd.node(T::TypeTree { inferred: false }, Span::NONE, Some(elem), None);
        let elems = self.tpd.list(&[typed]);
        let array = self.defn.array(self.u).ok()?;
        let array = self.u.intern_type(Type::Sym(None, array));
        let tpe = self.u.intern_applied(array, &[elem]);
        let seq = self.tpd.node(T::SeqLiteral { elems, elemtpt }, Span::NONE, Some(tpe), None);
        let id = self.ast.node(Tree::TypeTree);
        self.splices.insert(id, seq);
        Some(id)
    }

    /// The untyped tree of annotation argument `v` for `ctx`: a literal, `scala.Array(…)`, a
    /// reference to a static field, or a nested `new`.
    fn annotation_arg(&mut self, v: &SourceArg, ctx: Ctx) -> Option<NodeId> {
        Some(match v {
            SourceArg::Literal(e) => {
                let value = constant(e)?;
                self.ast.node(Tree::Literal { value })
            }
            SourceArg::Array(vs) => {
                let root = TName { name: self.name(nme::ROOT_PKG), is_type: false };
                let mut fun = self.ast.node(Tree::Ident { name: root });
                for part in [nme::SCALA, ARRAY] {
                    let name = TName { name: self.name(part), is_type: false };
                    fun = self.ast.node(Tree::Select { qualifier: fun, name });
                }
                let elems: Option<Vec<NodeId>> = vs.iter().map(|v| self.annotation_arg(v, ctx)).collect();
                let args = self.ast.list(&elems?);
                self.ast.node(Tree::Apply { fun, args })
            }
            &SourceArg::Static(field) => self.splice_ref(field),
            SourceArg::Annotation(a) => self.annotation_tree(a, ctx)?,
        })
    }
}

/// The constant of literal `e`; none for a value no Java literal writes.
fn constant(e: &ElementValue) -> Option<Constant> {
    Some(match *e {
        ElementValue::Boolean(b) => Constant::Boolean(b),
        ElementValue::Char(c) => Constant::Char(c),
        ElementValue::Int(i) => Constant::Int(i),
        ElementValue::Long(l) => Constant::Long(l),
        ElementValue::Float(f) => Constant::Float(f),
        ElementValue::Double(d) => Constant::Double(d),
        ElementValue::String(ref s) => Constant::String(s.encode_utf16().collect()),
        _ => return None,
    })
}
