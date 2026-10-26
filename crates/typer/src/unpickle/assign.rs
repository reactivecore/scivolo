// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/TypeAssigner.scala
//! The types `TypeAssigner` gives applications and type applications.

use super::{Refusal, Unpickler};
use crate::defn::type_ref;
use crate::tpd::{TNodeId, Tree as T};
use crate::typer::dependent_params::applied_result;
use crate::typer::seen_from::resolve_class_params;
use crate::typer::type_apply::instantiate_poly;
use tasty::tags::{MUTABLE, OBJECT, SYNTHETIC};
use tasty::{Flags, Kind, SymbolId, Type, TypeId};

impl Unpickler<'_> {
    /// The class of full name `full`.
    pub fn class_symbol(&mut self, full: &str) -> Result<SymbolId, Refusal> {
        let r = self.u.class_ref(full);
        self.u.class_symbol(r).ok_or("a class missing from the library")
    }

    /// The class of full name `full`, referred to from where it is visible.
    pub fn class_type(&mut self, full: &str) -> Result<TypeId, Refusal> {
        let c = self.class_symbol(full)?;
        Ok(type_ref(self.u, c))
    }

    /// The type `fun` applies, seen from its prefix, and the method it refers to.
    fn method_type(&mut self, fun: TNodeId) -> (TypeId, Option<SymbolId>) {
        let tpe = self.tpd.tpe(fun);
        let meth = self.tpd.sym(self.tpd.meth_part(fun));
        match *self.u.types().get(tpe) {
            Type::Sym(p, s) if self.u.symbol(s).kind == Kind::Def => {
                let info = match p.filter(|&p| !matches!(self.u.types().get(p), Type::Package(_))) {
                    Some(p) => {
                        let p = self.aliased_object(p);
                        self.u.member_info(p, s)
                    }
                    None => self.u.info(s),
                };
                let info = resolve_class_params(self.u, info);
                (self.with_repeated_class(info), Some(s))
            }
            _ => (tpe, meth),
        }
    }

    /// The object that `p`, a reference to a value whose type is a reference to an object (such
    /// as `scala.package.List`), stands for; `p` otherwise (as `Typer::method_info`).
    fn aliased_object(&mut self, mut p: TypeId) -> TypeId {
        for _ in 0..8 {
            let Type::Sym(_, s) = *self.u.types().get(p) else { break };
            let sym = self.u.symbol(s);
            if sym.kind != Kind::Val || sym.flags.has(OBJECT) || sym.flags.has(MUTABLE) {
                break;
            }
            let info = self.u.info(s);
            match *self.u.types().get(info) {
                Type::Sym(_, o) if self.u.symbol(o).flags.has(OBJECT) => p = info,
                Type::TermRef(..) => p = info,
                _ => break,
            }
        }
        p
    }

    /// Method type `t` with its repeated parameters, `Seq[T] @Repeated` in TASTy, typed
    /// `<repeated>[T]`, in each clause.
    fn with_repeated_class(&mut self, t: TypeId) -> TypeId {
        match self.u.types().get(t).clone() {
            Type::Method { binder, params, result, implicit, contextual } => {
                let mut mapped = Vec::new();
                for (n, p) in params {
                    let elem = match self.u.types().get(p).clone() {
                        Type::Annotated(seq, annot) if self.is_repeated_annot(annot) => {
                            match self.u.types().get(seq).clone() {
                                Type::Applied(_, args) if args.len() == 1 => Some(args[0]),
                                _ => None,
                            }
                        }
                        _ => None,
                    };
                    mapped.push((n, elem.map_or(p, |e| self.repeated_type(e))));
                }
                let result = self.with_repeated_class(result);
                self.u.intern_type(Type::Method { binder, params: mapped, result, implicit, contextual })
            }
            Type::Poly { binder, params, result } => {
                let result = self.with_repeated_class(result);
                self.u.intern_type(Type::Poly { binder, params, result })
            }
            _ => t,
        }
    }

    fn is_repeated_annot(&mut self, annot: TypeId) -> bool {
        self.u
            .class_symbol(annot)
            .is_some_and(|c| self.u.full_name(c) == "scala.annotation.internal.Repeated")
    }

    /// `<repeated>[elem]`.
    fn repeated_type(&mut self, elem: TypeId) -> TypeId {
        let scala = self.u.package("scala").expect("package scala");
        let synthetic = Flags::default().with(SYNTHETIC);
        let class = crate::defn::special(self.u, scala, "<repeated>", Kind::Class, synthetic);
        let class = type_ref(self.u, class);
        self.u.intern_type(Type::Applied(class, vec![elem]))
    }

    /// The type of `fun(args)`: the result of `fun`'s method type, its parameters replaced by the
    /// arguments' types.
    pub fn apply_type(&mut self, fun: TNodeId, args: &[TNodeId]) -> Result<TypeId, Refusal> {
        let (info, meth) = self.method_type(fun);
        let Type::Method { binder, params, result, .. } = self.u.types().get(info).clone() else {
            return Err("application of a value in a library inline body");
        };
        if params.len() != args.len() {
            return Err("application with missing arguments in a library inline body");
        }
        let part = self.tpd.meth_part(fun);
        let curried = matches!(self.u.types().get(result), Type::Method { .. });
        if self.tpd.sym(part).is_some_and(|s| self.u.symbol(s).name == "<init>") {
            match *self.tpd.tree(part) {
                T::Select { qualifier, .. } if matches!(self.tpd.tree(qualifier), T::New { .. }) => {
                    if !curried {
                        return Ok(self.tpd.tpe(qualifier));
                    }
                }
                _ => return Err("constructor call in a library inline body"),
            }
        }
        let names: Vec<String> = params.into_iter().map(|(n, _)| n).collect();
        let args: Vec<TypeId> = args.iter().map(|&a| self.tpd.tpe(a)).collect();
        Ok(applied_result(self.u, result, (binder, &names), meth, &args))
    }

    /// The type of `fun[args]`: the result of `fun`'s polymorphic type instantiated.
    pub fn type_apply_type(&mut self, fun: TNodeId, args: &[TNodeId]) -> Result<TypeId, Refusal> {
        let (info, meth) = self.method_type(fun);
        let Type::Poly { binder, params, result } = self.u.types().get(info).clone() else {
            return Err("type application of a value in a library inline body");
        };
        if params.len() != args.len() {
            return Err("type application with missing arguments in a library inline body");
        }
        let names: Vec<String> = params.into_iter().map(|(n, _)| n).collect();
        let args: Vec<TypeId> = args.iter().map(|&a| self.tpd.tpe(a)).collect();
        Ok(instantiate_poly(self.u, result, (binder, &names), &args, meth))
    }
}
