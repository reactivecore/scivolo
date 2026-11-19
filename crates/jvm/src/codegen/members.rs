// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeSkelBuilder.scala
//   compiler/src/dotty/tools/backend/jvm/BCodeHelpers.scala
//! The attributes of a method besides its code (dotty's `initJMethod`, `emitParamNames`).

use super::body::Body;
use super::{Gen, unsupported};
use classfile::{ACC_FINAL, ACC_SYNTHETIC, ClassFile, Last};
use lower::Result;
use names::nme;
use syntax::ast::Span;
use tasty::defn::META_TARGETS;
use tasty::tags::{ARTIFACT, GIVEN, IMPLICIT, SYNTHETIC};
use tasty::{Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

/// The full name of the annotation the `Deprecated` attribute stands for.
const DEPRECATED: &str = "scala.deprecated";

/// The meta-annotation target of the conversions of implicit classes, `meta.companionMethod`.
const COMPANION_METHOD: &str = "companionMethod";

/// The meta-annotation target of implicit classes, `meta.companionClass`.
const COMPANION_CLASS: &str = "companionClass";

/// Whether `s` is `implicit` or a `given`.
fn is_implicit(s: &tasty::Symbol) -> bool {
    s.flags.has(IMPLICIT) || s.flags.has(GIVEN)
}

impl Gen<'_, '_> {
    /// Adds to the method of `cf` added last, `sym` of descriptor `desc`, its generic signature
    /// (from `desc` alone if `erased`, none if lifted from a local definition), `Deprecated` if
    /// `deprecated`, and the names of its parameters `params`, `$this` first if `this_param`.
    pub fn method_attributes(
        &mut self,
        cf: &mut ClassFile,
        (sym, deprecated): (SymbolId, bool),
        desc: &str,
        params: &[SymbolId],
        (erased, this_param): (bool, bool),
    ) {
        let sig = match erased {
            _ if self.l.is_lifted(sym) => None,
            true => self.erased_signature(sym, desc, this_param),
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

    /// Refuses the Java annotations of `sym`, defined at `at`, that `kept_annotations` refuses, and
    /// the `@throws` that `check_throws` refuses, and those `check_tasty_mixin_annotations` refuses.
    pub fn check_annotations(&mut self, sym: SymbolId, at: Span) -> Result<()> {
        self.check_throws(sym, at)?;
        if let Err(what) = self.check_tasty_mixin_annotations(sym) {
            return unsupported(at, what);
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
    /// getter is, a mixin forwarder or an extension method if the member it copies is, the
    /// conversion of an `implicit class` if the class is for companion methods; an implicit class
    /// only if for companion classes or for no target (`PostTyper`'s `transform`).
    pub fn is_deprecated(&mut self, sym: SymbolId) -> bool {
        if self.l.u.symbol(sym).kind == Kind::Class && is_implicit(self.l.u.symbol(sym)) {
            return self.carries_deprecated(sym, &[COMPANION_CLASS], META_TARGETS);
        }
        let holders = [Some(sym), Some(self.l.annotations_holder(sym)), self.l.field_getter(sym)];
        holders.into_iter().flatten().any(|s| self.l.u.is_annotated(s, DEPRECATED))
            || self
                .implicit_class_of(sym)
                .is_some_and(|c| self.carries_deprecated(c, &[COMPANION_METHOD], &[]))
    }

    /// Whether `sym` is `@deprecated` meant for one of `targets` or, if `none_of` is not empty,
    /// for none of `none_of`.
    fn carries_deprecated(&mut self, sym: SymbolId, targets: &[&str], none_of: &[&str]) -> bool {
        let carried = self.l.u.annotations_carrying(sym, targets, none_of);
        carried.iter().any(|&a| self.l.u.full_name(a) == DEPRECATED)
    }

    /// The implicit class implicit method `sym` converts to, whose annotations meant for companion
    /// methods `sym` has (`PostTyper`'s `processValOrDefDef`).
    fn implicit_class_of(&mut self, sym: SymbolId) -> Option<SymbolId> {
        let s = self.l.u.symbol(sym);
        if s.kind != Kind::Def || !is_implicit(s) || s.flags.has(SYNTHETIC) {
            return None;
        }
        let info = self.l.u.info(sym);
        let result = self.l.u.result_type(info);
        self.l.u.class_symbol(result).filter(|&c| is_implicit(self.l.u.symbol(c)))
    }

    /// `ACC_TRANSIENT` and `ACC_VOLATILE` of field `field` annotated `@transient` and `@volatile`,
    /// or of its getter.
    pub fn annotated_field_flags(&mut self, field: SymbolId) -> u16 {
        let holders = [Some(field), self.l.field_getter(field)];
        let mut annotated = |full| holders.into_iter().flatten().any(|s| self.l.u.is_annotated(s, full));
        let transient = if annotated("scala.transient") { classfile::ACC_TRANSIENT } else { 0 };
        transient | if annotated("scala.volatile") { classfile::ACC_VOLATILE } else { 0 }
    }

    /// Marks the code from here as of the source line of `t` (dotty's `lineNumber`); a value
    /// definition inlined from another file at an unknown offset there gets a new mark.
    pub fn line(&mut self, b: &mut Body, t: TNodeId) {
        match self.line_of(t) {
            Some(0) => b.code.new_line(self.line_at(self.l.tpd.span(t).point())),
            Some(line) => b.code.line(line),
            None => {}
        }
    }

    /// The source line of `t`, if it has a position: of a value definition inlined from another
    /// file, the line of the call's file at its offset there, or 0 if unknown.
    pub fn line_of(&self, t: TNodeId) -> Option<u16> {
        let span = self.l.tpd.span(t);
        if !span.exists() {
            return None;
        }
        let foreign = self.l.foreign_span(t).filter(|_| matches!(self.l.tpd.tree(t), Tree::ValDef { .. }));
        let Some(own) = foreign else {
            return Some(self.line_at(span.point()));
        };
        let at = match own.exists() {
            true if own.is_in_other_source() => own.in_own_source().point(),
            true => self.l.sources.locate(own.point()).1,
            false => return Some(0),
        };
        let (call_file, _) = self.l.sources.locate(span.point());
        Some(u16::try_from(call_file.offset_to_line(at) + 1).unwrap_or(u16::MAX))
    }

    /// The source line of unit offset `offset`.
    pub fn line_at(&self, offset: u32) -> u16 {
        let (file, at) = self.l.sources.locate(offset);
        u16::try_from(file.offset_to_line(at) + 1).unwrap_or(u16::MAX)
    }
}
