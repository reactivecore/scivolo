// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeSkelBuilder.scala
//   compiler/src/dotty/tools/backend/jvm/BCodeHelpers.scala
//! The attributes of a method besides its code (dotty's `initJMethod`, `emitParamNames`).

use super::body::Body;
use super::descriptors::class_internal;
use super::{Gen, unsupported};
use classfile::{ACC_FINAL, ACC_SYNTHETIC, ClassFile, Last};
use lower::Result;
use names::nme;
use syntax::ast::Span;
use tasty::tags::ARTIFACT;
use tasty::{SymbolId, TypeId};
use typer::tpd::{TNodeId, Tree};

/// The full name of the annotation the `Deprecated` attribute stands for.
const DEPRECATED: &str = "scala.deprecated";

impl Gen<'_, '_> {
    /// Adds to the method of `cf` added last, `sym` of descriptor `desc`, its generic signature
    /// (from `desc` alone if `erased`), `Deprecated` if `deprecated`, and the names of its parameters `params`,
    /// `$this` first if `this_param`.
    pub fn method_attributes(
        &mut self,
        cf: &mut ClassFile,
        (sym, deprecated): (SymbolId, bool),
        desc: &str,
        params: &[SymbolId],
        (erased, this_param): (bool, bool),
    ) {
        let sig = match erased {
            true => self.erased_signature(sym, desc),
            false => self.generic_signature(sym, desc),
        };
        if let Some(sig) = sig {
            cf.member_signature(Last::Method, &sig);
        }
        if deprecated {
            cf.member_deprecated(Last::Method);
        }
        let mut names: Vec<(String, u16)> = Vec::new();
        if this_param {
            names.push((nme::SELF.into(), ACC_FINAL));
        }
        for &p in params {
            let s = self.l.u.symbol(p);
            let synthetic = if s.flags.has(ARTIFACT) || s.name == nme::OUTER { ACC_SYNTHETIC } else { 0 };
            names.push((tasty::encode(&s.name), ACC_FINAL | synthetic));
        }
        if !names.is_empty() {
            cf.method_parameters(&names);
        }
    }

    /// Adds to the method of `cf` added last the `Exceptions` attribute of method `sym`, if it has
    /// `@throws` annotations (dotty's `getExceptions`).
    pub fn exceptions(&mut self, cf: &mut ClassFile, sym: SymbolId) {
        let thrown = self.thrown(sym);
        if !thrown.is_empty() {
            let classes: Vec<String> = thrown.iter().map(|&t| class_internal(&self.l.u.erase(t))).collect();
            cf.exceptions(&classes);
        }
    }

    /// The exception types of the `@throws` annotations of method `sym`, last first: the class
    /// of a `classOf` argument, else the type argument (dotty's `ThrownException`).
    pub fn thrown(&mut self, sym: SymbolId) -> Vec<TypeId> {
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

    /// Refuses the Java annotations of `sym`, defined at `at`, that `kept_annotations` refuses, and
    /// `@throws` on a symbol other than a method of the unit.
    pub fn check_annotations(&mut self, sym: SymbolId, at: Span) -> Result<()> {
        let method = self.l.u.symbol(sym).kind == tasty::Kind::Def;
        if self.l.u.is_annotated(sym, "scala.throws") && !(method && !self.thrown(sym).is_empty()) {
            return unsupported(at, "@throws (the Exceptions attribute) on the JVM");
        }
        let source_only = ["java.lang.Override", "java.lang.SuppressWarnings"];
        for c in self.l.u.annotations(sym).to_vec() {
            let from_classfile = self.l.u.classfile_retention(c).is_some();
            if !from_classfile
                && self.is_java_class(c)
                && !source_only.contains(&self.l.u.full_name(c).as_str())
            {
                return unsupported(
                    at,
                    "Java annotations of the library (RuntimeVisibleAnnotations) on the JVM",
                );
            }
        }
        match self.kept_annotations(sym) {
            Ok(_) => Ok(()),
            Err(what) => unsupported(at, what),
        }
    }

    /// Determines whether `sym` is `@deprecated` (dotty's `isDeprecated`): a field also if its
    /// getter is, a mixin forwarder or an extension method if the member it copies is.
    pub fn is_deprecated(&mut self, sym: SymbolId) -> bool {
        let holders = [Some(sym), Some(self.l.annotations_holder(sym)), self.l.field_getter(sym)];
        holders.into_iter().flatten().any(|s| self.l.u.is_annotated(s, DEPRECATED))
    }

    /// `ACC_TRANSIENT` and `ACC_VOLATILE` of field `field` annotated `@transient` and `@volatile`,
    /// or of its getter.
    pub fn annotated_field_flags(&mut self, field: SymbolId) -> u16 {
        let holders = [Some(field), self.l.field_getter(field)];
        let mut annotated = |full| holders.into_iter().flatten().any(|s| self.l.u.is_annotated(s, full));
        let transient = if annotated("scala.transient") { classfile::ACC_TRANSIENT } else { 0 };
        transient | if annotated("scala.volatile") { classfile::ACC_VOLATILE } else { 0 }
    }

    /// Marks the code from here as of the source line of `t` (dotty's `lineNumber`).
    pub fn line(&mut self, b: &mut Body, t: TNodeId) {
        if let Some(line) = self.line_of(t) {
            b.code.line(line);
        }
    }

    /// The source line of `t`, if it has a position.
    pub fn line_of(&self, t: TNodeId) -> Option<u16> {
        let span = self.l.tpd.span(t);
        span.exists().then(|| self.line_at(span.point()))
    }

    /// The source line of unit offset `offset`.
    pub fn line_at(&self, offset: u32) -> u16 {
        let (file, at) = self.l.sources.locate(offset);
        u16::try_from(file.offset_to_line(at) + 1).unwrap_or(u16::MAX)
    }
}
