// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenExpr.scala
//! Closures: per closure an anonymous class `C$$Lambda$N` of the function type, its fields the
//! captured values, its abstract methods calling the lifted method (nscplugin's `genClosure`).

use super::Gen;
use crate::phases::sam::abstract_methods;
use crate::symbols::is_java_static;
use crate::{Result, unsupported};
use nir::sig::{Scope, Sig};
use nir::{Defn, DefnKind, GlobalId, Op, Type, TypeId, Val, ValId};
use tasty::SymbolId;
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
        let instance = !is_java_static(self.l.u, fun);
        let interface = tpt.map_or(self.l.tpd.tpe(t), |tpt| self.l.tpd.tpe(tpt));
        let (interface, _) = self.l.u.without_refinements(interface);
        let Some(function) = self.l.u.class_symbol(interface) else {
            return unsupported(span, "closures of a type that is not a class");
        };
        let bases = self.l.u.linearization(function);
        if bases.iter().any(|&b| self.l.u.is_trait(b) && !self.l.u.no_inits(b)) {
            return unsupported(span, "closures of traits with initialization");
        }
        let sams = abstract_methods(self.l, function);
        let name = |g: &Self, s: SymbolId| g.l.u.symbol(s).name.clone();
        if sams.iter().enumerate().any(|(i, &a)| sams[..i].iter().any(|&b| name(self, a) == name(self, b))) {
            return unsupported(span, "closures needing bridges");
        }
        let saved = self.at_span(span);
        let class = self.class.expect("a closure's class");
        self.lambdas += 1;
        let top = self.type_name(class);
        let anon = format!("{}$$Lambda${}", self.pool.top_name(top), self.lambdas);
        let anon = self.pool.top(&anon);
        let env = self.l.tpd.items(env).to_vec();
        let mut captures: Vec<TypeId> = env.iter().map(|&e| self.value_type(self.l.tpd.tpe(e))).collect();
        if instance {
            captures.insert(0, self.class_ref(class));
        }
        let fields: Vec<GlobalId> = (0..captures.len())
            .map(|i| self.pool.member(anon, &Sig::Field(format!("capture{i}"), Scope::Public)))
            .collect();
        let pos = self.m.pos;
        let mut traits = vec![self.type_name(function)];
        if tpt.is_some() {
            let parents: Vec<SymbolId> =
                self.l.u.parents(function).into_iter().filter_map(|p| self.l.u.class_symbol(p)).collect();
            let parents: Vec<SymbolId> = parents.into_iter().filter(|&p| self.l.u.is_trait(p)).collect();
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
        for sam in sams {
            let defn = self.gen_closure_method(anon, sam, fun, &captures, &fields, instance);
            self.closures.push(defn);
        }
        let alloc = self.emit(Op::Classalloc { name: anon, zone: None });
        let mut args = vec![alloc];
        if instance {
            let this = match self.l.u.is_package_object(class) {
                true => {
                    let scope = self.m.scopes.iter().find(|s| s.id == self.m.scope).map(|s| s.pos);
                    let outer = self.at(scope.unwrap_or(self.m.pos));
                    let module = self.gen_module(class);
                    self.m.pos = outer;
                    module
                }
                false => self.m.this.expect("`this` of a method"),
            };
            args.push(this);
        }
        for e in env {
            args.push(self.gen_expr(e)?);
        }
        let ptr = self.global_ptr(ctor);
        self.emit(Op::Call { ty: ctor_ty, ptr, args });
        self.m.pos = saved;
        Ok(alloc)
    }

    pub fn global_ptr(&mut self, name: GlobalId) -> ValId {
        let ptr = self.ty(Type::Ptr);
        self.val(Val::Global(name, ptr))
    }
}
