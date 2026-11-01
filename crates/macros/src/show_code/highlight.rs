// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/printers/SourceCode.scala
//! The syntax highlighting of `Printer.TreeAnsiCode`, and annotations.

use super::{Code, REFUSED, Shown};
use tasty::{Highlight, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree as T};

impl Code<'_> {
    pub fn keyword(&mut self, s: &str) {
        Highlight::Keyword.write(&mut self.out, s, self.ansi);
    }

    pub fn highlight_type(&mut self, s: &str) {
        Highlight::Type.write(&mut self.out, s, self.ansi);
    }

    pub fn literal(&mut self, s: &str) {
        Highlight::Literal.write(&mut self.out, s, self.ansi);
    }

    pub fn definition_name(&mut self, s: &str) {
        Highlight::Definition.write(&mut self.out, s, self.ansi);
    }

    /// `(t) `, a condition.
    pub fn parens(&mut self, t: TNodeId) -> Shown {
        self.push("(");
        self.tree(t)?;
        self.push(") ");
        Ok(())
    }

    /// The annotations of definition `s` but `@forceInline`, each followed by a space (dotty's
    /// `printDefAnnotations`).
    pub fn def_annotations(&mut self, s: SymbolId) -> Shown {
        let mut annots = self.h.typer.annotations.get(&s).cloned().unwrap_or_default();
        if annots.is_empty() && !self.h.u.tasty_annotations(s).is_empty() {
            let h = &mut *self.h;
            annots =
                typer::unpickle::annotations(&mut h.u, &mut h.tpd, &mut h.names, s).map_err(|_| REFUSED)?;
        }
        annots.retain(|&a| {
            let class = self.h.u.class_symbol(self.h.tpd.tpe(a));
            class.is_none_or(|c| self.h.u.full_name(c) != "scala.forceInline")
        });
        for (i, &a) in annots.iter().enumerate() {
            if i > 0 {
                self.push(" ");
            }
            self.annotation(a)?;
        }
        if !annots.is_empty() {
            self.push(" ");
        }
        Ok(())
    }

    /// Inferred type `tpe` followed by its annotations (dotty's `printTypeAndAnnots`); refused for
    /// an annotation of a class that takes arguments, which the type does not keep.
    pub fn annotated_type(&mut self, tpe: TypeId) -> Shown {
        let Type::Annotated(inner, annot) = *self.h.u.types().get(tpe) else {
            self.ty(tpe)?;
            if self.is_singleton(tpe) {
                self.push(".type");
            }
            return Ok(());
        };
        if !self.h.u.is_argless_annotation(annot) {
            return Err(REFUSED);
        }
        self.annotated_type(inner)?;
        self.push(" @");
        self.ty(annot)
    }

    /// Parameter type `tpe` of a method's type without its `@InlineParam` and `@ErasedParam`, as
    /// the parameter's type tree has it.
    pub fn without_param_annotations(&mut self, tpe: TypeId) -> TypeId {
        match *self.h.u.types().get(tpe) {
            Type::ByName(r) => {
                let r = self.without_param_annotations(r);
                return self.h.u.intern_type(Type::ByName(r));
            }
            Type::Annotated(inner, annot) => {
                let class = self.h.u.class_symbol(annot).map(|c| self.h.u.full_name(c));
                let internal = class.as_deref().and_then(|c| c.strip_prefix("scala.annotation.internal."));
                if matches!(internal, Some("InlineParam" | "ErasedParam")) {
                    return self.without_param_annotations(inner);
                }
            }
            _ => {}
        }
        tpe
    }

    /// An annotation `new A(args)` as `@A(args)`.
    fn annotation(&mut self, a: TNodeId) -> Shown {
        let a = self.r(a);
        let (mut fun, mut args) = (a, Vec::new());
        loop {
            match self.tree_of(fun) {
                T::Apply { fun: f, args: a } => {
                    args.splice(0..0, self.items(a));
                    fun = self.r(f);
                }
                T::TypeApply { fun: f, .. } => fun = self.r(f),
                T::Select { qualifier, .. } => fun = self.r(qualifier),
                T::New { tpt } => {
                    self.push("@");
                    self.type_tree(tpt)?;
                    break;
                }
                _ => return Err(REFUSED),
            }
        }
        if !args.is_empty() {
            self.push("(");
            self.trees(&args, ", ")?;
            self.push(")");
        }
        Ok(())
    }
}
