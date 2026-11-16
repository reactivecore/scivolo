// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeHelpers.scala
//   compiler/src/dotty/tools/dotc/transform/Memoize.scala
//! The exceptions a method declares by its `@throws` annotations.

use super::descriptors::class_internal;
use super::{Gen, unsupported};
use classfile::ClassFile;
use lower::Result;
use names::{name_kinds, nme};
use syntax::ast::Span;
use tasty::tags::{FIELDaccessor, LAZY};
use tasty::{Kind, SymbolId, TypeId};
use typer::tpd::{TNodeId, Tree};

impl Gen<'_, '_> {
    /// Adds to the method of `cf` added last the `Exceptions` attribute of method `sym`, if it has
    /// `@throws` annotations (dotty's `getExceptions`).
    pub fn exceptions(&mut self, cf: &mut ClassFile, sym: SymbolId) {
        let thrown = self.thrown(sym);
        if !thrown.is_empty() {
            let classes: Vec<String> = thrown.iter().map(|&t| class_internal(&self.l.u.erase(t))).collect();
            cf.exceptions(&classes);
        }
    }

    /// The exception types method `sym` declares, last first: those of its `@throws` annotations
    /// if it is a method or the accessor of a value without a field, as a lazy, abstract or
    /// constant `final` value or a trait's value; none for the getter or setter of a field, whose
    /// annotations stay on the field (dotty's `Memoize`).
    pub fn thrown(&mut self, sym: SymbolId) -> Vec<TypeId> {
        let s = self.l.u.symbol(sym);
        let kept = match s.kind {
            Kind::Def => !self.is_field_setter(sym),
            Kind::Val => {
                s.flags.has(LAZY)
                    || self.l.u.is_deferred(sym)
                    || !self.l.has_field(sym) && !self.l.is_getterless_field(sym)
            }
            _ => false,
        };
        if kept { self.annotated_thrown(sym) } else { Vec::new() }
    }

    /// The exception types the generic signature of method `sym` declares: those of its
    /// `@throws` annotations, also on a field's getter, but none on a field's setter.
    pub fn signature_thrown(&mut self, sym: SymbolId) -> Vec<TypeId> {
        if self.is_field_setter(sym) { Vec::new() } else { self.annotated_thrown(sym) }
    }

    /// Whether `sym` is the setter `x_=` of a variable.
    fn is_field_setter(&self, sym: SymbolId) -> bool {
        let s = self.l.u.symbol(sym);
        s.kind == Kind::Def && s.flags.has(FIELDaccessor) && s.name.ends_with(name_kinds::SETTER_SUFFIX)
    }

    /// Refuses `@throws` on method `sym`, defined at `at`, whose thrown classes are not known; on
    /// a class, object or other value it is ignored, as dotty does.
    pub fn check_throws(&mut self, sym: SymbolId, at: Span) -> Result<()> {
        let method = self.l.u.symbol(sym).kind == Kind::Def;
        let throws = self.l.u.is_annotated(sym, "scala.throws");
        if throws && method && self.annotated_thrown(sym).is_empty() {
            return unsupported(at, "@throws (the Exceptions attribute) on the JVM");
        }
        Ok(())
    }

    /// The exception types of the `@throws` annotations of `sym`, last first: the class of a
    /// `classOf` argument, else the type argument (dotty's `ThrownException`).
    fn annotated_thrown(&mut self, sym: SymbolId) -> Vec<TypeId> {
        let mut out = Vec::new();
        for annot in self.forwarded_annotations(sym).into_iter().rev() {
            let tpe = self.l.tpd.tpe(annot);
            let class = self.l.u.lambda(tpe).map_or(tpe, |l| l.result);
            if self.l.u.class_symbol(class).is_none_or(|c| self.l.u.full_name(c) != "scala.throws") {
                continue;
            }
            let args = match *self.l.tpd.tree(annot) {
                Tree::Apply { args, .. } => self.l.tpd.items(args).to_vec(),
                _ => Vec::new(),
            };
            if let Some(t) = args.first().and_then(|&a| self.class_of(a)) {
                out.push(t);
            } else if let Some((_, args)) = self.l.u.applied(tpe)
                && let [t] = args[..]
            {
                out.push(t);
            }
        }
        out
    }

    /// The type `classOf` call or literal `t` stands for.
    fn class_of(&self, t: TNodeId) -> Option<TypeId> {
        match *self.l.tpd.tree(t) {
            Tree::ClassLiteral { tpe } => Some(tpe),
            Tree::TypeApply { fun, args }
                if self.l.tpd.sym(fun).is_some_and(|f| self.l.u.symbol(f).name == nme::CLASS_OF) =>
            {
                self.l.tpd.items(args).first().map(|&a| self.l.tpd.tpe(a))
            }
            _ => None,
        }
    }

    /// The annotations of method `sym`; of a mixin forwarder without any, those of the trait
    /// method it calls, read from its TASTy if need be (dotty's `mkForwarderSym`).
    fn forwarded_annotations(&mut self, sym: SymbolId) -> Vec<TNodeId> {
        let own = self.l.tpd.annotations(sym).to_vec();
        let forwarded =
            self.l.mixed_in_from(sym).filter(|_| own.is_empty() && !self.l.is_mixin_accessor(sym));
        let Some(origin) = forwarded else { return own };
        if self.l.tpd.annotations(origin).is_empty() && self.l.u.symbol(origin).tree.is_some() {
            let read = typer::unpickle::annotations(self.l.u, self.l.tpd, self.l.names, origin);
            for annot in read.unwrap_or_default() {
                self.l.tpd.annotate(origin, annot);
            }
        }
        self.l.tpd.annotations(origin).to_vec()
    }
}
