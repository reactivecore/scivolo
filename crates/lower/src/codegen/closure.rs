//! Closures: per closure an anonymous class `C$$Lambda$N` of the function type, its fields the
//! captured values, its abstract methods calling the lifted method (nscplugin's `genClosure`).

use super::Gen;
use super::method::Method;
use crate::symbols::{is_java_static, is_trait};
use crate::{Result, unsupported};
use nir::sig::{Scope, Sig};
use nir::{Body, Defn, DefnKind, Global, GlobalId, LexicalScope, Op, Param, Type, TypeId, Val, ValId};
use tasty::{Kind, SymbolId};
use typer::tpd::{List, TNodeId};

impl Gen<'_, '_> {
    /// Closure `t` of method `meth` with captured values `env`, implementing functional interface
    /// `tpt` if given, else `t`'s type: an instance of its anonymous class.
    pub fn gen_closure(
        &mut self,
        t: TNodeId,
        env: List,
        meth: TNodeId,
        tpt: Option<TNodeId>,
    ) -> Result<ValId> {
        let span = self.l.tpd.span(t);
        let fun = self.l.tpd.sym(meth).expect("a closure's method");
        if !is_java_static(self.l.u, fun) {
            return unsupported(span, "closures of instance methods");
        }
        let interface = tpt.map_or(self.l.tpd.tpe(t), |tpt| self.l.tpd.tpe(tpt));
        let Some(function) = self.l.u.class_symbol(interface) else {
            return unsupported(span, "closures of a type that is not a class");
        };
        let saved = self.at_span(span);
        let class = self.class.expect("a closure's class");
        self.lambdas += 1;
        let top = self.type_name(class);
        let anon = format!("{}$$Lambda${}", self.pool.top_name(top), self.lambdas);
        let anon = self.pool.top(&anon);
        let env = self.l.tpd.items(env).to_vec();
        let captures: Vec<TypeId> = env.iter().map(|&e| self.gen_type(self.l.tpd.tpe(e))).collect();
        let fields: Vec<GlobalId> = (0..env.len())
            .map(|i| self.pool.member(anon, &Sig::Field(format!("capture{i}"), Scope::Public)))
            .collect();
        let pos = self.m.pos;
        let mut traits = vec![self.type_name(function)];
        if tpt.is_some() {
            let parents: Vec<SymbolId> =
                self.l.u.parents(function).into_iter().filter_map(|p| self.l.u.class_symbol(p)).collect();
            let parents: Vec<SymbolId> = parents.into_iter().filter(|&p| is_trait(self.l.u, p)).collect();
            traits.extend(parents.into_iter().map(|p| self.type_name(p)));
        }
        let parent = Some(self.type_name(self.l.defs.object));
        self.closures.push(Defn {
            name: anon,
            attrs: Vec::new(),
            pos,
            kind: DefnKind::Class { parent, traits },
        });
        let (ctor, ctor_ty) = self.gen_closure_ctor(anon, &captures, &fields);
        for (&name, &ty) in fields.iter().zip(&captures) {
            let rhs = self.val(Val::Zero(ty));
            self.closures.push(Defn { name, attrs: Vec::new(), pos, kind: DefnKind::Var { ty, rhs } });
        }
        for sam in self.abstract_methods(function) {
            let defn = self.gen_closure_method(anon, sam, fun, &captures, &fields);
            self.closures.push(defn);
        }
        let alloc = self.emit(Op::Classalloc { name: anon, zone: None });
        let mut args = vec![alloc];
        for e in env {
            args.push(self.gen_expr(e)?);
        }
        let ptr = self.global_ptr(ctor);
        self.emit(Op::Call { ty: ctor_ty, ptr, args });
        self.m.pos = saved;
        Ok(alloc)
    }

    /// The constructor of anonymous class `anon`, storing the captured values of types
    /// `captures` in `fields`; its name and type.
    fn gen_closure_ctor(
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
    /// unboxed or cast, passed to static method `fun`; the result boxed if primitive.
    fn gen_closure_method(
        &mut self,
        anon: GlobalId,
        sam: SymbolId,
        fun: SymbolId,
        captures: &[TypeId],
        fields: &[GlobalId],
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
        let impl_params: Vec<TypeId> = self.param_types(fun).into_iter().skip(captures.len()).collect();
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
            let owner = g.l.u.symbol(fun).owner;
            let call_ty = g.method_sig(fun, true);
            let ptr = g.static_member_name(fun, owner);
            let ptr = g.global_ptr(ptr);
            let result =
                g.emit(Op::Call { ty: call_ty, ptr, args: loaded.into_iter().chain(args).collect() });
            let result = g.ensure_boxed(result, ret);
            g.ret(result);
        });
        Defn { name, attrs: Vec::new(), pos: self.m.pos, kind: DefnKind::Define { ty, body } }
    }

    /// The abstract methods of `class`.
    fn abstract_methods(&mut self, class: SymbolId) -> Vec<SymbolId> {
        let decls = self.l.u.decls(class).unwrap_or_default();
        decls
            .into_iter()
            .filter(|&d| self.l.u.symbol(d).kind == Kind::Def && self.l.u.body(d).is_none())
            .filter(|&d| self.l.u.symbol(d).name != "<init>")
            .collect()
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

    fn global_ptr(&mut self, name: GlobalId) -> ValId {
        let ptr = self.ty(Type::Ptr);
        self.val(Val::Global(name, ptr))
    }
}
