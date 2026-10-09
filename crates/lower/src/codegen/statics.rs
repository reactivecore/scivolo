//! Static forwarders to the methods of top-level objects, in their companion classes or in
//! mirror classes (nscplugin's `genStaticMethodForwarders` and `genMirrorClass`).

use super::Gen;
use super::method::Method;
use crate::symbols::{
    companion_class, companion_module, has, is_constructor, is_extern, is_module_class, is_static,
    is_static_module, is_trait,
};
use crate::{Result, unsupported};
use nir::{Attr, Body, Defn, DefnKind, GlobalId, LexicalScope, Op, Param, PosId, Type, Val};
use syntax::ast::Span;
use tasty::tags::{
    FIELDaccessor, INLINE, MACRO, PRIVATE, PRIVATEqualified, PROTECTED, PROTECTEDqualified, STATIC,
};
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
        self.forwarders_from_module_class(existing, module_class)
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
        defns.extend(self.forwarders_from_module_class(&[], c)?);
        self.mirrors.push(defns);
        Ok(())
    }

    fn forwarders_from_module_class(
        &mut self,
        existing: &[GlobalId],
        module_class: SymbolId,
    ) -> Result<Vec<Defn>> {
        let d = &self.l.defs;
        let plain = [d.object, d.any, d.any_ref];
        let bases = self.l.u.linearization(module_class);
        let classes: Vec<SymbolId> = bases
            .into_iter()
            .filter(|&b| {
                let library_trait = is_trait(self.l.u, b) && self.l.u.symbol(b).tree.is_some();
                !plain.contains(&b) && self.l.u.symbol(b).name != "Matchable" && !library_trait
            })
            .collect();
        if classes.iter().skip(1).any(|&c| self.l.u.symbol(c).tree.is_some()) {
            return unsupported(Span::NONE, "forwarders to methods of library classes");
        }
        let mut seen = Vec::new();
        let mut out = Vec::new();
        let members: Vec<SymbolId> =
            classes.into_iter().flat_map(|c| self.l.u.decls(c).unwrap_or_default()).collect();
        for m in members {
            let excluded = [PROTECTED, STATIC, PRIVATE, MACRO, INLINE, PRIVATEqualified, PROTECTEDqualified];
            let method = self.l.u.symbol(m).kind == Kind::Def || has(self.l.u, m, FIELDaccessor);
            if !method
                || is_constructor(self.l.u, m)
                || is_extern(self.l.u, m)
                || self.l.u.is_deferred(m)
                || excluded.iter().any(|&f| has(self.l.u, m, f))
            {
                continue;
            }
            let signature = (self.l.u.symbol(m).name.clone(), self.l.u.erased_method(m));
            if seen.contains(&signature) {
                continue;
            }
            seen.push(signature);
            let name = self.static_member_name(m, Some(module_class));
            if existing.contains(&name) {
                return unsupported(Span::NONE, "a static method beside the forwarder of the same name");
            }
            let pos = self.sym_pos(m, module_class);
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
        let ty = self.ty(Type::Function(args[1..].to_vec(), ret));
        self.static_define(name, ty, pos)
    }

    /// `static def f(): T = C.f` for static field `f` of type `ty` of object class `c`.
    pub fn static_getter(&mut self, f: SymbolId, c: SymbolId, ty: nir::TypeId, pos: PosId) -> Defn {
        self.m = Method::new(None, 0, pos);
        let entry = self.fresh();
        self.label(entry, Vec::new());
        let obj = self.gen_module(c);
        let name = self.field_name(f);
        let value = self.emit(Op::Fieldload { ty, obj, name });
        self.ret(value);
        let name = self.static_member_name(f, Some(c));
        let fun = self.ty(Type::Function(Vec::new(), ty));
        self.static_define(name, fun, pos)
    }

    /// The inline-hinted static method `name` of type `ty` whose body the current method holds.
    fn static_define(&mut self, name: GlobalId, ty: nir::TypeId, pos: PosId) -> Defn {
        let m = std::mem::replace(&mut self.m, Method::new(None, 0, pos));
        let body = Body {
            insts: m.buf.insts,
            positions: m.buf.positions,
            local_names: Vec::new(),
            scopes: vec![LexicalScope { id: 0, parent: 0, pos: self.no_pos }],
        };
        Defn { name, attrs: vec![Attr::InlineHint], pos, kind: DefnKind::Define { ty, body } }
    }
}
