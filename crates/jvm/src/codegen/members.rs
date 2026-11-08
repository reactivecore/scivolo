// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeSkelBuilder.scala
//   compiler/src/dotty/tools/backend/jvm/BCodeHelpers.scala
//! The attributes of a method besides its code (dotty's `initJMethod`, `emitParamNames`).

use super::body::Body;
use super::{Gen, unsupported};
use classfile::{ACC_FINAL, ACC_SYNTHETIC, ClassFile, Last};
use lower::Result;
use syntax::ast::Span;
use tasty::tags::{ARTIFACT, PRIVATE};
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
        let thrown = self.thrown(sym);
        if !thrown.is_empty() {
            let classes: Vec<String> =
                thrown.iter().map(|&t| super::class_internal(&self.l.u.erase(t))).collect();
            cf.exceptions(&classes);
        }
        if !erased && let Some(mut sig) = self.generic_signature(sym, desc) {
            for t in thrown {
                sig.push('^');
                sig.push_str(&super::descriptor(&self.l.u.erase(t)));
            }
            cf.member_signature(Last::Method, &sig);
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

    /// The exception types of the `@throws` annotations of method `sym`, last first (dotty's
    /// `ThrownException` over `annotations`).
    fn thrown(&mut self, sym: SymbolId) -> Vec<TypeId> {
        let mut out = Vec::new();
        for annot in self.l.tpd.annotations(sym).to_vec().into_iter().rev() {
            let tpe = self.l.tpd.tpe(annot);
            if self.l.u.class_symbol(tpe).is_none_or(|c| self.l.u.full_name(c) != "scala.throws") {
                continue;
            }
            if let tasty::Type::Applied(_, args) = self.l.u.types().get(tpe)
                && let [t] = args[..]
                && !out.contains(&t)
            {
                out.push(t);
            }
        }
        out
    }

    /// Refuses Java annotations a class file would keep on `sym`, defined at `at`, and `@throws`
    /// on a symbol other than a method of the unit.
    pub fn check_annotations(&mut self, sym: SymbolId, at: Span) -> Result<()> {
        let method = self.l.u.symbol(sym).kind == tasty::Kind::Def;
        if self.l.u.is_annotated(sym, "scala.throws") && !(method && !self.thrown(sym).is_empty()) {
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

    /// `ACC_TRANSIENT` and `ACC_VOLATILE` of field `field` annotated `@transient` and `@volatile`,
    /// or of its getter.
    pub fn annotated_field_flags(&mut self, field: SymbolId) -> u16 {
        let holders = [Some(field), self.l.field_getter(field)];
        let mut annotated = |full| holders.into_iter().flatten().any(|s| self.l.u.is_annotated(s, full));
        let transient = if annotated("scala.transient") { classfile::ACC_TRANSIENT } else { 0 };
        transient | if annotated("scala.volatile") { classfile::ACC_VOLATILE } else { 0 }
    }

    /// The generic signature of member `sym` of descriptor `desc` (dotty's `getGenericSignature`
    /// with `Mixin`'s `mixinGenericInfos`): none for an accessor of a private trait value, nor for
    /// a mixin forwarder whose signature does not erase to `desc`.
    pub fn generic_signature(&mut self, sym: SymbolId, desc: &str) -> Option<String> {
        let info = self.seen_from_owner(sym);
        if self.mixed_origin(sym).is_some_and(|o| self.l.u.symbol(o).flags.has(PRIVATE)) {
            return None;
        }
        let sig = super::signatures::member(self.l.u, sym, info, desc)?;
        let forwarder = self.l.mixed_in_from(sym).is_some() && !self.l.is_mixin_accessor(sym);
        (!forwarder || super::signatures::erases_to(&sig, desc)).then_some(sig)
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
        if let Some(line) = self.line_of(t) {
            b.code.line(line);
        }
    }

    /// The source line of `t`, if it has a position.
    pub fn line_of(&self, t: TNodeId) -> Option<u16> {
        let span = self.l.tpd.span(t);
        span.exists().then(|| {
            let (file, at) = self.l.sources.locate(span.point());
            u16::try_from(file.offset_to_line(at) + 1).unwrap_or(u16::MAX)
        })
    }
}
