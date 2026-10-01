//! Static forwarders to the methods of top-level objects, in their companion classes or in
//! mirror classes (nscplugin's `genStaticMethodForwarders` and `genMirrorClass`).

use super::Gen;
use super::method::Method;
use crate::symbols::{
    companion_class, companion_module, has, is_constructor, is_module_class, is_static, is_static_module,
};
use crate::{Result, unsupported};
use nir::{Attr, Body, Defn, DefnKind, GlobalId, Param, PosId, Type, Val};
use syntax::ast::Span;
use tasty::tags::{MACRO, PRIVATE, PRIVATEqualified, PROTECTED, PROTECTEDqualified, STATIC};
use tasty::{Kind, SymbolId};

impl Gen<'_, '_> {
    /// Whether `c` is static and its owner a package.
    fn is_candidate_for_forwarders(&self, c: SymbolId) -> bool {
        let owner = self.l.u.symbol(c).owner;
        is_static(self.l.u, c) && owner.is_some_and(|o| self.l.u.symbol(o).kind == Kind::Package)
    }

    /// The forwarders of the current class to the methods of its companion object.
    pub fn static_method_forwarders(&mut self, existing: &[GlobalId]) -> Result<Vec<Defn>> {
        let c = self.class.expect("a class");
        if !self.is_candidate_for_forwarders(c) || is_static_module(self.l.u, c) {
            return Ok(Vec::new());
        }
        let Some(module) = companion_module(self.l.u, c) else { return Ok(Vec::new()) };
        let Some(module_class) = self.l.u.module_class(module) else { return Ok(Vec::new()) };
        let pos = self.m.pos;
        self.forwarders_from_module_class(existing, module_class, pos)
    }

    /// A class `C` of forwarders for a top-level object `C` without a companion class.
    pub fn gen_mirror_class(&mut self, c: SymbolId, pos: PosId) -> Result<()> {
        if !is_module_class(self.l.u, c) || !self.is_candidate_for_forwarders(c) {
            return Ok(());
        }
        if companion_class(self.l.u, c).is_some() {
            return Ok(());
        }
        let module_name = self.type_name(c);
        let id = self.pool.top_name(module_name).to_owned();
        let name = self.pool.top(id.strip_suffix('$').unwrap_or(&id));
        let parent = self.pool.top("java.lang.Object");
        let class = Defn {
            name,
            attrs: Vec::new(),
            pos,
            kind: DefnKind::Class { parent: Some(parent), traits: Vec::new() },
        };
        let mut defns = vec![class];
        defns.extend(self.forwarders_from_module_class(&[], c, pos)?);
        self.mirrors.push(defns);
        Ok(())
    }

    fn forwarders_from_module_class(
        &mut self,
        existing: &[GlobalId],
        module_class: SymbolId,
        pos: PosId,
    ) -> Result<Vec<Defn>> {
        let d = &self.l.defn;
        let plain = [d.object, d.any, d.any_ref];
        let bases = self.l.u.linearization(module_class);
        if bases.iter().skip(1).any(|b| !plain.contains(b) && self.l.u.symbol(*b).name != "Matchable") {
            return unsupported(Span::NONE, "forwarders to inherited methods");
        }
        let decls = self.l.u.decls(module_class).unwrap_or_default();
        let mut out = Vec::new();
        for m in decls {
            let excluded = [PROTECTED, STATIC, PRIVATE, MACRO, PRIVATEqualified, PROTECTEDqualified];
            if self.l.u.symbol(m).kind != Kind::Def
                || is_constructor(self.l.u, m)
                || excluded.iter().any(|&f| has(self.l.u, m, f))
            {
                continue;
            }
            let name = self.static_member_name(m, Some(module_class));
            if existing.contains(&name) {
                return unsupported(Span::NONE, "a static method beside the forwarder of the same name");
            }
            out.push(self.forwarder(m, module_class, name, pos));
        }
        Ok(out)
    }

    /// `static def m(params) = C.m(params)` for method `m` of object class `module_class`.
    fn forwarder(&mut self, m: SymbolId, module_class: SymbolId, name: GlobalId, pos: PosId) -> Defn {
        self.m = Method::new(None, 0, pos);
        let sig = self.method_sig(m, false);
        let Type::Function(args, ret) = self.pool.ty(sig).clone() else { unreachable!("a method type") };
        let params: Vec<Param> = args[1..].iter().map(|&ty| Param { id: self.fresh(), ty }).collect();
        let values = params.iter().map(|p| self.val(Val::Local(p.id, p.ty))).collect();
        let entry = self.fresh();
        self.label(entry, params);
        let module = self.gen_module(module_class);
        let result = self.call_method(m, true, module, values);
        self.ret(result);
        let m = std::mem::replace(&mut self.m, Method::new(None, 0, pos));
        let body = Body {
            insts: m.buf.insts,
            positions: m.buf.positions,
            local_names: Vec::new(),
            scopes: Vec::new(),
        };
        let ty = self.ty(Type::Function(args[1..].to_vec(), ret));
        Defn { name, attrs: vec![Attr::InlineHint], pos, kind: DefnKind::Define { ty, body } }
    }
}
