// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenExpr.scala
//! The methods of a closure's anonymous class: the constructor storing the captured values, and
//! the abstract methods calling the lifted method (nscplugin's `genAnonymousClassCtor`,
//! `genAnonClassMethod`).

use super::Gen;
use super::method::Method;
use nir::sig::Sig;
use nir::{Body, Defn, DefnKind, Global, GlobalId, LexicalScope, Op, Param, Type, TypeId, Val};
use tasty::SymbolId;

impl Gen<'_, '_> {
    /// The constructor of anonymous class `anon`, storing the captured values of types
    /// `captures` in `fields`; its name and type.
    pub fn gen_closure_ctor(
        &mut self,
        anon: GlobalId,
        captures: &[TypeId],
        fields: &[GlobalId],
    ) -> (GlobalId, TypeId) {
        let name = self.pool.member(anon, &Sig::Ctor(captures.to_vec()));
        let self_ty = self.ty(Type::Ref { name: anon, exact: false, nullable: true });
        let unit = self.ty(Type::Unit);
        let ty =
            self.ty(Type::Function(std::iter::once(self_ty).chain(captures.iter().copied()).collect(), unit));
        let body = self.closure_body(|g| {
            let this = g.param(self_ty);
            let params: Vec<Param> = captures.iter().map(|&t| g.param(t)).collect();
            let entry = g.fresh();
            g.label(entry, std::iter::once(this).chain(params.iter().copied()).collect());
            let this = g.val(Val::Local(this.id, self_ty));
            let object = g.class_ref(g.l.defs.object);
            let object_name = g.type_name(g.l.defs.object);
            let object_ctor = g.pool.member(object_name, &Sig::Ctor(Vec::new()));
            let super_ty = g.ty(Type::Function(vec![object], unit));
            let ptr = g.global_ptr(object_ctor);
            g.emit(Op::Call { ty: super_ty, ptr, args: vec![this] });
            for (&name, p) in fields.iter().zip(params) {
                let value = g.val(Val::Local(p.id, p.ty));
                g.emit(Op::Fieldstore { ty: p.ty, obj: this, name, value });
            }
            let unit = g.val(Val::Unit);
            g.ret(unit);
        });
        let pos = self.m.pos;
        self.closures.push(Defn { name, attrs: Vec::new(), pos, kind: DefnKind::Define { ty, body } });
        (name, ty)
    }

    /// Abstract method `sam` of anonymous class `anon`: the captured values, then the arguments
    /// unboxed or cast, passed to method `fun`, an `instance` method of the first captured value
    /// or static; the result boxed if primitive.
    pub fn gen_closure_method(
        &mut self,
        anon: GlobalId,
        sam: SymbolId,
        fun: SymbolId,
        captures: &[TypeId],
        fields: &[GlobalId],
        instance: bool,
    ) -> Defn {
        let sam_name = self.method_name(sam);
        let Global::Member(_, sig) = *self.pool.global(sam_name) else { unreachable!("a member") };
        let name = GlobalId(self.pool.globals.intern(Global::Member(anon, sig)));
        let sam_ty = self.method_sig(sam, false);
        let Type::Function(ref sam_params, ret) = *self.pool.ty(sam_ty) else {
            unreachable!("a method type")
        };
        let sam_params = sam_params[1..].to_vec();
        let self_ty = self.ty(Type::Ref { name: anon, exact: false, nullable: true });
        let ty = self
            .ty(Type::Function(std::iter::once(self_ty).chain(sam_params.iter().copied()).collect(), ret));
        let env = captures.len() - usize::from(instance);
        let impl_params: Vec<TypeId> = self.param_types(fun).into_iter().skip(env).collect();
        let body = self.closure_body(|g| {
            let this = g.param(self_ty);
            let params: Vec<Param> = sam_params.iter().map(|&t| g.param(t)).collect();
            let entry = g.fresh();
            g.label(entry, std::iter::once(this).chain(params.iter().copied()).collect());
            let this = g.val(Val::Local(this.id, self_ty));
            let mut args = Vec::new();
            for (p, &to) in params.iter().zip(&impl_params) {
                let value = g.val(Val::Local(p.id, p.ty));
                args.push(g.ensure_unboxed(value, to));
            }
            let mut loaded = Vec::new();
            for (&name, &ty) in fields.iter().zip(captures) {
                loaded.push(g.emit(Op::Fieldload { ty, obj: this, name }));
            }
            let result = if instance {
                let receiver = loaded.remove(0);
                g.call_method(fun, false, receiver, loaded.into_iter().chain(args).collect())
            } else {
                let owner = g.l.u.symbol(fun).owner;
                let call_ty = g.method_sig(fun, true);
                let ptr = g.static_member_name(fun, owner);
                let ptr = g.global_ptr(ptr);
                g.emit(Op::Call { ty: call_ty, ptr, args: loaded.into_iter().chain(args).collect() })
            };
            let result = g.ensure_boxed(result, ret);
            g.ret(result);
        });
        Defn { name, attrs: Vec::new(), pos: self.m.pos, kind: DefnKind::Define { ty, body } }
    }

    /// The body `emit` generates, as a method of its own at the current position.
    fn closure_body(&mut self, emit: impl FnOnce(&mut Self)) -> Body {
        let pos = self.m.pos;
        let outer = std::mem::replace(&mut self.m, Method::new(None, 0, pos));
        self.m.scopes.push(LexicalScope { id: 0, parent: 0, pos: self.no_pos });
        emit(self);
        let m = std::mem::replace(&mut self.m, outer);
        Body { insts: m.buf.insts, positions: m.buf.positions, local_names: m.local_names, scopes: m.scopes }
    }

    fn param(&mut self, ty: TypeId) -> Param {
        Param { id: self.fresh(), ty }
    }
}
