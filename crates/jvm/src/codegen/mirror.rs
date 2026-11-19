// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeHelpers.scala
//! Static forwarders to the methods of a top-level object, in its companion class or in a mirror
//! class (dotty's `genMirrorClass`, `addForwarders`, `addForwarder`).

use super::annotation_holders::Holder;
use super::class_bytes::scala_markers;
use super::descriptors::class_descriptor;
use super::forwarded::Forwarded;
use super::signatures::Member;
use super::{ClassBytes, Gen, unsupported};
use classfile::{
    ACC_FINAL, ACC_PUBLIC, ACC_STATIC, ACC_SUPER, ACC_SYNTHETIC, ACC_VARARGS, ClassFile, Code, FieldOp,
    Invoke, Last,
};
use lower::Result;
use names::nme;
use tasty::SymbolId;
use typer::tpd::TNodeId;

impl Gen<'_, '_> {
    /// The mirror class of top-level object class `module` named `module_name`, which declares
    /// classes `declared`, with the object's TASTy `tasty`.
    pub fn gen_mirror_class(
        &mut self,
        td: TNodeId,
        (module, module_name): (SymbolId, &str),
        declared: &[SymbolId],
        tasty: Option<Vec<u8>>,
    ) -> Result<()> {
        let name = module_name.strip_suffix('$').unwrap_or(module_name);
        let mut cf = ClassFile::new(ACC_PUBLIC | ACC_FINAL | ACC_SUPER, name, "java/lang/Object", &[]);
        self.emit_annotations(&mut cf, None, Holder::own(module));
        self.add_forwarders(&mut cf, module, None);
        let source = self.source_name(td);
        cf.source_file(&source);
        self.nested_attributes(&mut cf, module, declared);
        scala_markers(&mut cf, true);
        let class = ClassBytes::new(name.to_owned(), cf, tasty, &mut |_, _| None)
            .or_else(|e| unsupported(self.l.tpd.span(td), e))?;
        self.out.push(class);
        Ok(())
    }

    /// Adds to `cf` the static forwarders to the methods of object class `module`, but those
    /// whose names a member of its companion class `companion` has.
    pub fn add_forwarders(&mut self, cf: &mut ClassFile, module: SymbolId, companion: Option<SymbolId>) {
        let module_name = self.class_name(module);
        let module_desc = class_descriptor(&module_name);
        for Forwarded { name, desc, synthetic, method, target } in self.forwarded(module, companion) {
            let m = if self.l.is_bridge(method) { target } else { method };
            let mut code = Code::new(0);
            code.field(
                FieldOp::GetStatic,
                &mut cf.pool,
                &module_name,
                nme::MODULE_INSTANCE_FIELD,
                &module_desc,
            );
            let (params, result) = classfile::split_method(&desc);
            let mut slot = 0;
            for param in params {
                code.load(param, slot);
                slot += classfile::value_slots(param) as u16;
            }
            code.invoke(Invoke::Virtual, &mut cf.pool, &module_name, &name, &desc);
            code.ret(result);
            let varargs = if self.l.is_varargs_forwarder(target) { ACC_VARARGS } else { 0 };
            let flags = ACC_PUBLIC | ACC_STATIC | varargs | if synthetic { ACC_SYNTHETIC } else { 0 };
            cf.method(flags, &name, &desc, Some(code));
            self.emit_method_annotations(cf, m);
            let this = self.l.u.this_type(module);
            let unflattened = self.l.unflattened_info(target);
            let info = unflattened.unwrap_or_else(|| self.l.u.member_info(this, target));
            self.exceptions(cf, m);
            let thrown = self.signature_thrown(m);
            let member = Member { forwarder: true, ..self.signed_member(target, target) };
            if !self.l.is_mixin_accessor(m)
                && !self.made_after_erasure(target)
                && (unflattened.is_some() || self.erases_as_declared(target, info, &desc))
                && let Some(sig) = self.member_signature(member, info, &desc, &thrown)
            {
                cf.member_signature(Last::Method, &sig);
            }
        }
    }

    /// Whether `m`, which a bridge forwards to, is a bridge or mixin forwarder, whose type before
    /// erasure dotty does not know.
    fn made_after_erasure(&self, m: SymbolId) -> bool {
        self.l.is_bridge(m) || self.l.mixed_in_from(m).is_some() || self.is_library_made(m)
    }
}
