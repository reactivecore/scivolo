// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeSkelBuilder.scala
//   compiler/src/dotty/tools/backend/jvm/BCodeHelpers.scala
//! The attributes of a method besides its code (dotty's `initJMethod`, `emitParamNames`).

use super::body::Body;
use super::{Gen, unsupported};
use classfile::{ACC_FINAL, ACC_SYNTHETIC, ClassFile, Last};
use lower::Result;
use syntax::ast::Span;
use tasty::tags::ARTIFACT;
use tasty::{SymbolId, TypeId};
use typer::tpd::TNodeId;

impl Gen<'_, '_> {
    /// Adds to the method of `cf` added last, `sym` of descriptor `desc`, its generic signature
    /// unless `erased`, and the names of its parameters `params`, `$this` first if `this_param`.
    pub fn method_attributes(
        &mut self,
        cf: &mut ClassFile,
        sym: SymbolId,
        desc: &str,
        params: &[SymbolId],
        (erased, this_param): (bool, bool),
    ) {
        if !erased {
            let info = self.seen_from_owner(sym);
            if let Some(sig) = super::signatures::member(self.l.u, sym, info, desc) {
                cf.member_signature(Last::Method, &sig);
            }
        }
        if self.l.u.is_annotated(sym, "scala.deprecated") {
            cf.member_deprecated(Last::Method);
        }
        let mut names: Vec<(String, u16)> = Vec::new();
        if this_param {
            names.push(("$this".into(), ACC_FINAL));
        }
        for &p in params {
            let s = self.l.u.symbol(p);
            let synthetic = if s.flags.has(ARTIFACT) || s.name == "$outer" { ACC_SYNTHETIC } else { 0 };
            names.push((tasty::encode(&s.name), ACC_FINAL | synthetic));
        }
        if !names.is_empty() {
            cf.method_parameters(&names);
        }
    }

    /// Refuses `@throws` and Java annotations a class file would keep on `sym`, defined at `at`.
    pub fn check_annotations(&mut self, sym: SymbolId, at: Span) -> Result<()> {
        if self.l.u.is_annotated(sym, "scala.throws") {
            return unsupported(at, "@throws (the Exceptions attribute) on the JVM");
        }
        let source_only = ["java.lang.Override", "java.lang.SuppressWarnings"];
        for c in self.l.u.annotations(sym).to_vec() {
            if self.is_java_class(c) && !source_only.contains(&self.l.u.full_name(c).as_str()) {
                return unsupported(at, "Java annotations (RuntimeVisibleAnnotations) on the JVM");
            }
        }
        Ok(())
    }

    /// The type of member `sym` seen from its class (dotty's `memberInfo` of the owner's
    /// `thisType`), of a mixin accessor or forwarder that of the trait member.
    pub fn seen_from_owner(&mut self, sym: SymbolId) -> TypeId {
        let owner = self.l.u.symbol(sym).owner.filter(|&o| self.l.u.is_class(o));
        let name = &self.l.u.symbol(sym).name;
        if name == "$outer" || name.ends_with("$$$outer") {
            let (_, result) = self.l.u.erased_method(sym);
            if let tasty::Erased::Class(outer) = result
                && let Some(Some(c)) = self.classes.get(&super::internal(&outer)).copied()
            {
                return self.l.u.this_type(c);
            }
        }
        match owner {
            Some(o) => {
                let this = self.l.u.this_type(o);
                let member = self.mixed_origin(sym).unwrap_or(sym);
                self.l.u.member_info(this, member)
            }
            None => self.l.u.info(sym),
        }
    }

    /// The trait member mixin accessor or forwarder `sym` copies, or the field of a mixin getter
    /// its getter's.
    fn mixed_origin(&mut self, sym: SymbolId) -> Option<SymbolId> {
        if let Some(m) = self.l.mixed_in_from(sym) {
            return Some(m);
        }
        let (owner, name) = (self.l.u.symbol(sym).owner?, self.l.u.symbol(sym).name.clone());
        let getters = self.l.u.decl(owner, &name).unwrap_or_default();
        getters.into_iter().find(|&g| g != sym).and_then(|g| self.l.mixed_in_from(g))
    }

    /// Marks the code from here as of the source line of `t` (dotty's `lineNumber`).
    pub fn line(&mut self, b: &mut Body, t: TNodeId) {
        let span = self.l.tpd.span(t);
        if span.exists() {
            let (file, at) = self.l.sources.locate(span.point());
            b.code.line(u16::try_from(file.offset_to_line(at) + 1).unwrap_or(u16::MAX));
        }
    }
}
