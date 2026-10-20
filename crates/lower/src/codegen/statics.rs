// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenStat.scala
//! Static forwarders to the methods of top-level objects, in their companion classes or in
//! mirror classes (nscplugin's `genStaticMethodForwarders` and `genMirrorClass`).

use super::Gen;
use super::method::Method;
use crate::symbols::{has, is_extern, is_static, is_static_module};
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
        let Some(module) = self.l.u.companion_module(c) else { return Ok(Vec::new()) };
        let Some(module_class) = self.l.u.module_class(module) else { return Ok(Vec::new()) };
        self.forwarders_from_module_class(existing, module_class)
    }

    /// A class `C` of forwarders for a top-level object `C` without a companion class.
    pub fn gen_mirror_class(&mut self, c: SymbolId, pos: PosId) -> Result<()> {
        if !self.l.u.is_module_class(c) || !self.is_candidate_for_forwarders(c) {
            return Ok(());
        }
        if self.l.u.companion_class(c).is_some() {
            return Ok(());
        }
        let module_name = self.type_name(c);
        let id = self.pool.top_name(module_name).to_owned();
        let name = self.pool.top(tasty::module_name(&id).unwrap_or(&id));
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
        let is_jdk = |l: &crate::Lower, c: SymbolId| {
            let name = l.u.full_name(c);
            name.starts_with("java.") || name.starts_with("javax.")
        };
        let jdk: Vec<SymbolId> = bases.iter().copied().filter(|&b| is_jdk(self.l, b)).collect();
        let mut classes = Vec::new();
        for b in bases {
            let library_trait = self.l.u.is_trait(b) && self.l.u.symbol(b).tree.is_some();
            let behind_jdk = !is_jdk(self.l, b) && jdk.iter().any(|&j| self.l.u.derives_from(j, b));
            if !plain.contains(&b) && self.l.u.symbol(b).name != "Matchable" && !library_trait && !behind_jdk
            {
                classes.push(b);
            }
        }
        let mut seen = Vec::new();
        let mut out = Vec::new();
        let members: Vec<SymbolId> =
            classes.into_iter().flat_map(|c| self.l.u.decls(c).unwrap_or_default()).collect();
        for m in members {
            let excluded = [PROTECTED, STATIC, PRIVATE, MACRO, INLINE, PROTECTEDqualified];
            let library = self.l.u.symbol(m).tree.is_some();
            let owner = self.l.u.symbol(m).owner.map(|o| self.l.u.full_name(o)).unwrap_or_default();
            let jdk = owner.starts_with("java.") || owner.starts_with("javax.");
            let value = self.l.u.symbol(m).kind == Kind::Val;
            let method = self.l.u.symbol(m).kind == Kind::Def || has(self.l.u, m, FIELDaccessor);
            let deferred = if library { self.l.u.body(m).is_none() } else { self.l.u.is_deferred(m) };
            if !(method || library && value)
                || self.l.u.is_constructor(m)
                || is_extern(self.l.u, m)
                || deferred
                || excluded.iter().any(|&f| has(self.l.u, m, f))
                || self.l.lifted_from.contains_key(&m)
                || jdk && has(self.l.u, m, PRIVATEqualified)
            {
                continue;
            }
            if !method && jdk {
                return unsupported(Span::NONE, "forwarders to fields of JDK classes");
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
            let pos = match self.l.coords.get(&m) {
                Some(&span) => self.pos_or(span, self.no_pos),
                None => self.sym_pos(m, module_class),
            };
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
