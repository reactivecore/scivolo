// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/GenReflectiveInstantisation.scala
//! The registration with `scala.scalanative.reflect.Reflect` of classes and objects deriving from
//! an `@EnableReflectiveInstantiation` class: a static initializer and the function classes it
//! passes.

use super::Gen;
use super::literal::primitive_name;
use crate::phases::casts::class_named;
use crate::{Result, unsupported};
use names::nme;
use nir::sig::{Scope, Sig};
use nir::{Body, Defn, DefnKind, GlobalId, Op, PosId, Type, TypeId, Val, ValId};
use tasty::SymbolId;
use tasty::tags::{ABSTRACT, PRIVATE, PROTECTED, PROTECTEDqualified};
use typer::tpd::TNodeId;

const ANNOTATION: &str = "scala.scalanative.reflect.annotation.EnableReflectiveInstantiation";
const REFLECT: &str = "scala.scalanative.reflect.Reflect$";
const REGISTER_MODULE: &str = "registerLoadableModuleClass";
const REGISTER_CLASS: &str = "registerInstantiatableClass";
const FUNCTION_SUFFIX: &str = "$scalanative$ReflectivelyInstantiate$";
const FUNCTION0: &str = "scala.runtime.AbstractFunction0";
const FUNCTION1: &str = "scala.runtime.AbstractFunction1";
const SERIALIZABLE: &str = "java.io.Serializable";
const TUPLE2: &str = "scala.Tuple2";
const CLASS: &str = "java.lang.Class";

/// A public constructor to register: its parameter types and position.
struct Constructor {
    params: Vec<TypeId>,
    pos: PosId,
}

impl Gen<'_, '_> {
    /// Generates what registers class `sym` of definition `td` with `Reflect` if a base class of
    /// it is `@EnableReflectiveInstantiation` (nscplugin's `genReflectiveInstantiation`): an object
    /// that is not local by a loader function, a concrete class that is not local by an
    /// instantiating function per public constructor, in `<clinit>`. Refuses a public constructor
    /// with a parameter of type `Nothing`, `Null`, `RawPtr` or `RawSize`, which have no runtime
    /// class and which nscplugin fails on.
    pub fn gen_reflective_instantiation(&mut self, td: TNodeId, sym: SymbolId) -> Result<()> {
        let span = self.l.tpd.span(td);
        let bases = self.l.u.linearization(sym);
        if !bases.into_iter().any(|b| self.l.u.is_annotated(b, ANNOTATION)) {
            return Ok(());
        }
        let owner = self.l.original_owner(sym);
        let lifted = owner != self.l.u.symbol(sym).owner;
        let module = self.l.u.is_module_class(sym);
        let local = owner.is_none_or(|o| !self.l.u.is_class(o));
        if lifted && (module || local) {
            return Ok(());
        }
        let method = if module { REGISTER_MODULE } else { REGISTER_CLASS };
        let register = class_named(self.l, REFLECT)
            .and_then(|reflect| Some((reflect, *self.l.u.decls_named(reflect, method).first()?)));
        let Some(register) = register else {
            return unsupported(span, "reflective instantiation without `Reflect`");
        };
        let top = self.type_name(sym);
        let saved = self.at_span(span);
        let body = if module {
            let loads = owner.is_none_or(|o| self.l.u.symbol(o).is_package() || self.l.u.is_module_class(o));
            Some(self.closure_body(|g| {
                g.entry_label();
                let loader = g.gen_module_loader(top, loads);
                g.gen_register(register, top, loader);
            }))
        } else {
            self.gen_class_registration(td, sym, top, register)?
        };
        if let Some(body) = body {
            let clinit = self.pool.member(top, &Sig::Clinit);
            let unit = self.ty(Type::Unit);
            self.push_define(clinit, Vec::new(), unit, body);
        }
        self.m.pos = saved;
        Ok(())
    }

    /// The body registering class `sym`, none without a public constructor.
    fn gen_class_registration(
        &mut self,
        td: TNodeId,
        sym: SymbolId,
        top: GlobalId,
        register: (SymbolId, SymbolId),
    ) -> Result<Option<Body>> {
        let concrete = !self.l.u.is_trait(sym) && !crate::symbols::has(self.l.u, sym, ABSTRACT);
        let ctors = if concrete { self.public_constructors(td, sym)? } else { Vec::new() };
        if ctors.is_empty() {
            return Ok(None);
        }
        Ok(Some(self.closure_body(|g| {
            g.entry_label();
            let info = g.gen_constructors_info(top, &ctors);
            g.gen_register(register, top, info);
        })))
    }

    /// The public constructors of `sym`, in the order of its member lookup: the last defined first.
    fn public_constructors(&mut self, td: TNodeId, sym: SymbolId) -> Result<Vec<Constructor>> {
        let span = self.l.tpd.span(td);
        let primary = self.l.tpd.class_template(td).map(|t| t.constr);
        let primary = primary.and_then(|c| Some((self.l.tpd.sym(c)?, self.l.tpd.span(c))));
        let mut out = Vec::new();
        for c in self.l.u.decls_named(sym, nme::CONSTRUCTOR).into_iter().rev() {
            let hidden = [PRIVATE, PROTECTED, PROTECTEDqualified];
            if hidden.into_iter().any(|f| crate::symbols::has(self.l.u, c, f)) {
                continue;
            }
            let params = self.param_types(c);
            if !params.iter().all(|&p| has_param_class(self.pool.ty(p))) {
                return unsupported(span, "reflective instantiation through a parameter without a class");
            }
            let declared = primary.filter(|&(p, _)| p == c).map(|(_, at)| at);
            let at = declared.or(self.spans.get(&c).copied()).unwrap_or(span);
            out.push(Constructor { params, pos: self.pos_or(at, self.m.pos) });
        }
        Ok(out)
    }

    fn entry_label(&mut self) {
        let entry = self.fresh();
        self.label(entry, Vec::new());
    }

    /// `Reflect.method(top's name, classOf[top], arg); ()` for `register`, `Reflect` and `method`.
    fn gen_register(&mut self, (reflect, method): (SymbolId, SymbolId), top: GlobalId, arg: ValId) {
        let name = self.pool.top_name(top).to_owned();
        let name = self.str(&name);
        let args = vec![self.val(Val::String(name)), self.val(Val::ClassOf(top)), arg];
        let module = self.gen_module(reflect);
        self.call_method(method, true, module, args);
        let unit = self.val(Val::Unit);
        self.ret(unit);
    }

    /// An instance of a new `Function0` loading object `top`, null unless `loads`
    /// (nscplugin's `genModuleLoader`).
    fn gen_module_loader(&mut self, top: GlobalId, loads: bool) -> ValId {
        let name = self.function_class_name(top, 0);
        let object = self.class_ref(self.l.u.defn().object);
        let self_ty = self.top_ref(name);
        let body = self.closure_body(|g| {
            let this = g.param(self_ty);
            let entry = g.fresh();
            g.label(entry, vec![this]);
            let module = if loads { g.emit(Op::Module(top)) } else { g.val(Val::Null) };
            g.ret(module);
        });
        let apply = self.pool.member(name, &Sig::Method("apply".into(), vec![object], Scope::Public));
        self.push_define(apply, vec![self_ty], object, body);
        self.gen_function_class(name, FUNCTION0)
    }

    /// The array of (parameter classes, instantiating function) of `ctors`, of class `top`
    /// (nscplugin's `genClassConstructorsInfo`).
    fn gen_constructors_info(&mut self, top: GlobalId, ctors: &[Constructor]) -> ValId {
        let tuple_name = self.pool.top(TUPLE2);
        let tuple = self.top_ref(tuple_name);
        let tuples = self.ty(Type::Array { elem: tuple, nullable: true });
        let length = self.val(Val::Int(ctors.len() as i32));
        let info = self.emit(Op::Arrayalloc { ty: tuples, init: length, zone: None });
        let class_name = self.pool.top(CLASS);
        let class = self.top_ref(class_name);
        let object = self.class_ref(self.l.u.defn().object);
        for (i, ctor) in ctors.iter().enumerate() {
            let saved = self.at(ctor.pos);
            let instantiator = self.gen_instantiator(top, i, &ctor.params);
            let count = self.val(Val::Int(ctor.params.len() as i32));
            let classes = self.emit(Op::Arrayalloc { ty: class, init: count, zone: None });
            for (j, &param) in ctor.params.iter().enumerate() {
                let idx = self.val(Val::Int(j as i32));
                let param_class = self.param_class(param);
                let value = self.val(Val::ClassOf(param_class));
                self.emit(Op::Arraystore { ty: class, arr: classes, idx, value });
            }
            let pair = self.gen_new(tuple_name, vec![object, object], vec![classes, instantiator]);
            let idx = self.val(Val::Int(i as i32));
            self.emit(Op::Arraystore { ty: tuple, arr: info, idx, value: pair });
            self.m.pos = saved;
        }
        info
    }

    /// An instance of a new `Function1` calling the constructor of `top` numbered `index`, of
    /// parameter types `params`, on the elements of its argument array.
    fn gen_instantiator(&mut self, top: GlobalId, index: usize, params: &[TypeId]) -> ValId {
        let name = self.function_class_name(top, index);
        let object = self.class_ref(self.l.u.defn().object);
        let self_ty = self.top_ref(name);
        let objects = self.ty(Type::Array { elem: object, nullable: true });
        let body = self.closure_body(|g| {
            let this = g.param(self_ty);
            let array = g.param(objects);
            let entry = g.fresh();
            g.label(entry, vec![this, array]);
            let arr = g.val(Val::Local(array.id, objects));
            let mut args = Vec::new();
            for (i, &param) in params.iter().enumerate() {
                let idx = g.val(Val::Int(i as i32));
                let elem = g.emit(Op::Arrayload { ty: object, arr, idx });
                args.push(match g.boxed_class(param) {
                    Some(boxed) => g.emit(Op::Unbox { ty: boxed, obj: elem }),
                    None => g.emit(Op::As { ty: param, obj: elem }),
                });
            }
            let instance = g.gen_new(top, params.to_vec(), args);
            g.ret(instance);
        });
        let sig = Sig::Method("apply".into(), vec![object, object], Scope::Public);
        let apply = self.pool.member(name, &sig);
        self.push_define(apply, vec![self_ty, objects], object, body);
        self.gen_function_class(name, FUNCTION1)
    }

    /// The constructor and class definition of function class `name` of superclass `parent`, and
    /// a new instance (nscplugin's `genConstructor`).
    fn gen_function_class(&mut self, name: GlobalId, parent: &str) -> ValId {
        let parent = self.pool.top(parent);
        let parent_ty = self.top_ref(parent);
        let self_ty = self.top_ref(name);
        let unit = self.ty(Type::Unit);
        let body = self.closure_body(|g| {
            let this = g.param(self_ty);
            let entry = g.fresh();
            g.label(entry, vec![this]);
            let ty = g.ty(Type::Function(vec![parent_ty], unit));
            let ctor = g.pool.member(parent, &Sig::Ctor(Vec::new()));
            let ptr = g.global_ptr(ctor);
            let this = g.val(Val::Local(this.id, self_ty));
            g.emit(Op::Call { ty, ptr, args: vec![this] });
            let result = g.val(Val::Unit);
            g.ret(result);
        });
        let ctor = self.pool.member(name, &Sig::Ctor(Vec::new()));
        self.push_define(ctor, vec![self_ty], unit, body);
        let traits = vec![self.pool.top(SERIALIZABLE)];
        let kind = DefnKind::Class { parent: Some(parent), traits };
        self.defns.push(Defn { name, attrs: Vec::new(), pos: self.m.pos, kind });
        self.gen_new(name, Vec::new(), Vec::new())
    }

    /// `new top(args)` through the constructor of parameter types `params`.
    fn gen_new(&mut self, top: GlobalId, params: Vec<TypeId>, args: Vec<ValId>) -> ValId {
        let alloc = self.emit(Op::Classalloc { name: top, zone: None });
        let self_ty = self.top_ref(top);
        let unit = self.ty(Type::Unit);
        let ty =
            self.ty(Type::Function(std::iter::once(self_ty).chain(params.iter().copied()).collect(), unit));
        let ctor = self.pool.member(top, &Sig::Ctor(params));
        let ptr = self.global_ptr(ctor);
        self.emit(Op::Call { ty, ptr, args: std::iter::once(alloc).chain(args).collect() });
        alloc
    }

    /// The class standing for constructor parameter type `t`, one of `has_param_class`
    /// (`Type.typeToName`).
    fn param_class(&mut self, t: TypeId) -> GlobalId {
        let primitive = match *self.pool.ty(t) {
            Type::Ref { name, .. } if self.pool.top_name(name) == "scala.runtime.BoxedUnit" => "Unit",
            Type::Ref { name, .. } => return name,
            Type::Array { elem, .. } => return self.array_class(elem),
            ref other => primitive_name(other).expect("a parameter type with a class"),
        };
        self.pool.top(&format!("scala.scalanative.runtime.Primitive{primitive}"))
    }

    /// The name of the function class of the constructor of `top` numbered `index`.
    fn function_class_name(&mut self, top: GlobalId, index: usize) -> GlobalId {
        let number = if index == 0 { String::new() } else { format!("${index}") };
        let name = format!("{}{number}{FUNCTION_SUFFIX}", self.pool.top_name(top));
        self.pool.top(&name)
    }

    fn top_ref(&mut self, name: GlobalId) -> TypeId {
        self.ty(Type::Ref { name, exact: false, nullable: true })
    }

    /// Adds method `name` of parameter types `params` and result `result` at the current position.
    fn push_define(&mut self, name: GlobalId, params: Vec<TypeId>, result: TypeId, body: Body) {
        let ty = self.ty(Type::Function(params, result));
        self.defns.push(Defn {
            name,
            attrs: Vec::new(),
            pos: self.m.pos,
            kind: DefnKind::Define { ty, body },
        });
    }
}

/// Whether constructor parameter type `t` has a runtime class to register.
fn has_param_class(t: &Type) -> bool {
    matches!(t, Type::Ref { .. } | Type::Array { .. }) || primitive_name(t).is_some()
}
