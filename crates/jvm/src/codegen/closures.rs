// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeBodyBuilder.scala
//! Closures as `invokedynamic` of `LambdaMetafactory` (dotty's `genLoad` of a `Closure`,
//! `genInvokeDynamicLambda`).

use super::body::Body;
use super::descriptors::{class_descriptor, method_descriptor, method_type};
use super::{Gen, is_static_member, method_name, unsupported};
use classfile::{Constant, Handle};
use lower::Result;
use tasty::tags::PRIVATE;
use tasty::{SymbolId, Type};
use typer::tpd::{List, TNodeId, Tree};

const FLAG_SERIALIZABLE: i32 = 1;
const FLAG_BRIDGES: i32 = 4;

/// The leading parameters of a bootstrap method.
pub const BOOTSTRAP: &str =
    "Ljava/lang/invoke/MethodHandles$Lookup;Ljava/lang/String;Ljava/lang/invoke/MethodType;";

impl Gen<'_, '_> {
    /// Closure `t` of method `meth` capturing `env`, implementing `tpt` if given, else `t`'s type.
    pub fn closure(
        &mut self,
        b: &mut Body,
        t: TNodeId,
        env: List,
        meth: TNodeId,
        tpt: Option<TNodeId>,
    ) -> Result<i32> {
        let span = self.l.tpd.span(t);
        let target = self.l.tpd.sym(meth).expect("a closure's method");
        let tpe = tpt.map_or(self.l.tpd.tpe(t), |tpt| self.l.tpd.tpe(tpt));
        let (tpe, _) = self.l.u.without_refinements(tpe);
        let Some(interface) = self.l.u.class_symbol(tpe) else {
            return unsupported(span, "closures of a type that is not a class");
        };
        let &[sam] = &lower::abstract_methods(self.l, interface)[..] else {
            return unsupported(span, "closures of a type without a single abstract method");
        };
        let owner = self.l.u.symbol(target).owner.expect("a method's owner");
        let static_ = is_static_member(self.l, target);
        if !static_ {
            let (qualifier, prefix) = match *self.l.tpd.tree(meth) {
                Tree::Select { qualifier, .. } => (Some(qualifier), None),
                _ => (None, self.ident_prefix(meth)),
            };
            self.receiver(b, (target, true), qualifier, prefix)?;
        }
        let desc = method_descriptor(self.l.u, target);
        let (params, result) = classfile::split_method(&desc);
        let env = self.l.tpd.items(env).to_vec();
        if env.len() > params.len() {
            return unsupported(span, "a closure capturing more values than its method takes");
        }
        for (&e, param) in env.iter().zip(&params) {
            if self.expr(b, e)? != classfile::value_slots(param) {
                return unsupported(self.l.tpd.span(e), "a captured value of another JVM type");
            }
        }
        let owner_name = self.class_name(owner);
        let receiver = if static_ { String::new() } else { class_descriptor(&owner_name) };
        let interface_name = self.class_name(interface);
        let captured = receiver + &params[..env.len()].concat();
        let call_desc = method_type(&captured, &class_descriptor(&interface_name));
        let sam_desc = method_descriptor(self.l.u, sam);
        let (_, sam_result) = classfile::split_method(&sam_desc);
        let result = match boxed(result) {
            Some(boxed) if !is_primitive(sam_result) => class_descriptor(boxed),
            _ => result.to_owned(),
        };
        let instantiated = method_type(&params[env.len()..].concat(), &result);
        let mut bridges: Vec<String> = Vec::new();
        for d in std::iter::once(instantiated.clone()).chain(self.overridden(sam)) {
            if d != sam_desc && !bridges.contains(&d) {
                bridges.push(d);
            }
        }
        let serializable = self.is_serializable(interface);
        let flags = if serializable { FLAG_SERIALIZABLE } else { 0 }
            | if bridges.is_empty() { 0 } else { FLAG_BRIDGES };
        let in_trait = self.l.u.is_trait(owner);
        let kind = match () {
            _ if static_ => classfile::REF_INVOKE_STATIC,
            _ if self.l.u.symbol(target).flags.has(PRIVATE) || self.l.u.is_constructor(target) => {
                classfile::REF_INVOKE_SPECIAL
            }
            _ if in_trait => classfile::REF_INVOKE_INTERFACE,
            _ => classfile::REF_INVOKE_VIRTUAL,
        };
        let name = method_name(self.l.u, target);
        let handle = Handle { kind, owner: owner_name, name, descriptor: desc.clone(), interface: in_trait };
        let mut args = vec![
            Constant::MethodType(&sam_desc),
            Constant::Handle(&handle),
            Constant::MethodType(&instantiated),
        ];
        let factory = "java/lang/invoke/LambdaMetafactory";
        let bootstrap = match flags {
            0 => {
                let d = format!(
                    "({BOOTSTRAP}Ljava/lang/invoke/MethodType;Ljava/lang/invoke/MethodHandle;Ljava/lang/invoke/MethodType;)Ljava/lang/invoke/CallSite;"
                );
                Handle::static_method(factory, "metafactory", &d)
            }
            _ => {
                args.push(Constant::Int(flags));
                if !bridges.is_empty() {
                    args.push(Constant::Int(bridges.len() as i32));
                    args.extend(bridges.iter().map(|d| Constant::MethodType(d)));
                }
                let d = format!("({BOOTSTRAP}[Ljava/lang/Object;)Ljava/lang/invoke/CallSite;");
                Handle::static_method(factory, "altMetafactory", &d)
            }
        };
        let sam_name = method_name(self.l.u, sam);
        b.code.invoke_dynamic(b.pool, (&sam_name, &call_desc), &bootstrap, &args);
        if serializable && !self.lambdas.contains(&handle) {
            self.lambdas.push(handle);
        }
        Ok(1)
    }

    /// The descriptors of the methods abstract method `sam` overrides (dotty's
    /// `allOverriddenSymbols`).
    fn overridden(&mut self, sam: SymbolId) -> Vec<String> {
        let owner = self.l.u.symbol(sam).owner.expect("a method's owner");
        let name = self.l.u.symbol(sam).name.clone();
        let site = self.l.u.class_ref(&self.l.u.full_name(owner));
        let site = self.l.u.intern_type(Type::This(site));
        let mut out = Vec::new();
        for base in self.l.u.linearization(owner).into_iter().skip(1) {
            for m in self.l.u.decls_named(base, &name) {
                if self.l.u.symbol(m).kind == tasty::Kind::Def && self.l.u.info_matches(site, sam, m) {
                    out.push(method_descriptor(self.l.u, m));
                }
            }
        }
        out
    }

    /// Whether lambdas of `interface` are serializable: of a `Serializable` interface or a
    /// `scala.FunctionN`.
    fn is_serializable(&mut self, interface: SymbolId) -> bool {
        let function = self
            .l
            .u
            .full_name(interface)
            .strip_prefix("scala.Function")
            .is_some_and(|n| n.parse::<u8>().is_ok());
        let serializable = self.l.u.class_ref("java.io.Serializable");
        let serializable = self.l.u.class_symbol(serializable);
        function || serializable.is_some_and(|s| self.l.u.derives_from(interface, s))
    }
}

fn is_primitive(desc: &str) -> bool {
    classfile::Kind::of(desc) != classfile::Kind::Reference
}

/// The class boxing values of primitive descriptor `desc`.
fn boxed(desc: &str) -> Option<&'static str> {
    Some(match desc {
        "Z" => "java/lang/Boolean",
        "C" => "java/lang/Character",
        "B" => "java/lang/Byte",
        "S" => "java/lang/Short",
        "I" => "java/lang/Integer",
        "J" => "java/lang/Long",
        "F" => "java/lang/Float",
        "D" => "java/lang/Double",
        "V" => "scala/runtime/BoxedUnit",
        _ => return None,
    })
}
