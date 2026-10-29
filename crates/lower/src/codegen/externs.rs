// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenStat.scala
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenType.scala
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenName.scala
//! Extern objects and traits: their declarations, names and C signatures (nscplugin's
//! `genExternMethod`, `genExternMethodSig`, `genExternType`).

use super::Gen;
use crate::symbols::{has, is_extern};
use crate::{Result, unsupported};
use nir::sig::Sig;
use nir::{Attr, Defn, DefnKind, GlobalId, PosId, Type, TypeId};
use tasty::tags::{FIELDaccessor, SYNTHETIC};
use tasty::{Erased, SymbolId};
use typer::tpd::{TNodeId, Tree};

const EXTERN_BODY: &str = "scala.scalanative.unsafe.package$.extern";
const NAME: &str = "scala.scalanative.unsafe.name";
const BLOCKING: &str = "scala.scalanative.unsafe.package$.blocking";

/// The arity of function pointer class `name`.
fn func_arity(name: &str) -> Option<u8> {
    name.strip_prefix("scala.scalanative.unsafe.CFuncPtr")?.parse().ok()
}

/// The C type a value of class `name` passes as (NIR's `Type.unbox`, without `CFuncPtr22`).
pub fn unboxed(name: &str) -> Option<Type> {
    let func = func_arity(name).is_some_and(|n| n < 22);
    Some(match name {
        _ if func => Type::Ptr,
        "scala.scalanative.unsafe.Ptr" | "scala.scalanative.unsafe.CArray" => Type::Ptr,
        "scala.scalanative.unsafe.CVarArgList" => Type::Ptr,
        "scala.scalanative.unsafe.Size" | "scala.scalanative.unsigned.USize" => Type::Size,
        "scala.scalanative.unsigned.UByte" | "java.lang.Byte" => Type::Byte,
        "scala.scalanative.unsigned.UShort" | "java.lang.Short" => Type::Short,
        "scala.scalanative.unsigned.UInt" | "java.lang.Integer" => Type::Int,
        "scala.scalanative.unsigned.ULong" | "java.lang.Long" => Type::Long,
        "java.lang.Boolean" => Type::Bool,
        "java.lang.Character" => Type::Char,
        "java.lang.Float" => Type::Float,
        "java.lang.Double" => Type::Double,
        _ => return None,
    })
}

impl Gen<'_, '_> {
    /// An extern object or trait: its declarations, without constructor, fields or forwarders.
    pub fn gen_extern_class(&mut self, td: TNodeId, sym: SymbolId, pos: PosId) -> Result<()> {
        let span = self.l.tpd.span(td);
        if !self.l.u.is_annotated(sym, crate::symbols::EXTERN) {
            return unsupported(span, "classes in extern objects");
        }
        let blocking = self.is_blocking(sym);
        let attrs = self.class_attrs(sym, Vec::new());
        let attrs = attrs.into_iter().map(|a| if a.is_extern() { Attr::Extern { blocking } } else { a });
        let attrs = attrs.collect();
        let name = self.type_name(sym);
        let traits = self.class_interfaces(sym);
        let kind = if crate::symbols::is_static_module(self.l.u, sym) {
            DefnKind::Module { parent: Some(self.pool.top("java.lang.Object")), traits }
        } else if self.l.u.is_trait(sym) {
            DefnKind::Trait { traits }
        } else {
            return unsupported(span, "extern classes that are no object or trait");
        };
        self.defns.push(Defn { name, attrs, pos, kind });
        let Tree::TypeDef { rhs, .. } = *self.l.tpd.tree(td) else { unreachable!("a class definition") };
        let Tree::Template { constr, body, .. } = *self.l.tpd.tree(rhs) else {
            return unsupported(span, "a type definition that is not a class");
        };
        if self.l.u.is_trait(sym) {
            let pos = self.pos_or(self.l.tpd.span(constr), pos);
            let name = self.pool.member(name, &Sig::Extern("$init$".into()));
            let unit = self.ty(Type::Unit);
            let ty = self.ty(Type::Function(Vec::new(), unit));
            let attrs = vec![Attr::Extern { blocking: false }];
            self.defns.push(Defn { name, attrs, pos, kind: DefnKind::Declare { ty } });
        }
        for &stat in self.l.tpd.items(body).to_vec().iter() {
            match self.l.tpd.tree(stat) {
                Tree::DefDef { .. } => {
                    let defn = self.gen_extern_method(stat)?;
                    self.defns.extend(defn);
                }
                Tree::TypeDef { .. } | Tree::Import { .. } => {}
                Tree::ValDef { .. } => self.gen_extern_field(stat, sym, pos)?,
                _ => return unsupported(self.l.tpd.span(stat), "statements in an extern object"),
            }
        }
        self.gen_mirror_class(sym, pos)
    }

    /// `def m(...): T = extern`, or a forwarder to such a method, as a declaration of `m`.
    /// None for a synthetic method.
    fn gen_extern_method(&mut self, dd: TNodeId) -> Result<Option<Defn>> {
        let sym = self.l.tpd.sym(dd).expect("a method definition's symbol");
        let Tree::DefDef { rhs, .. } = *self.l.tpd.tree(dd) else { unreachable!("a method") };
        let span = self.l.tpd.span(dd);
        let Some(rhs) = rhs else { return unsupported(span, "abstract methods in extern objects") };
        let target = match *self.l.tpd.tree(rhs) {
            _ if self.is_extern_body(rhs) => sym,
            _ if has(self.l.u, sym, SYNTHETIC) && !self.l.bridges.contains(&sym) => return Ok(None),
            _ if has(self.l.u, sym, FIELDaccessor) && !self.l.bridges.contains(&sym) => return Ok(None),
            _ if self.l.forwarded.contains_key(&sym) => return Ok(None),
            Tree::Apply { fun, .. }
                if let Some(t) = self.l.tpd.sym(fun)
                    && is_extern(self.l.u, t)
                    && self.l.u.symbol(t).name == self.l.u.symbol(sym).name =>
            {
                t
            }
            _ => return unsupported(span, "methods in extern objects without an extern body"),
        };
        let ty = self.extern_sig(target, span)?;
        if target != sym && self.method_sig(sym, true) != ty {
            return unsupported(span, "extern forwarders of another signature");
        }
        let inline = has(self.l.u, sym, FIELDaccessor) || self.l.bridges.contains(&sym);
        let owner = self.l.u.symbol(sym).owner.expect("a method's owner");
        let extern_ = Attr::Extern { blocking: self.is_blocking(sym) || self.is_blocking(owner) };
        let attrs = if inline { vec![Attr::AlwaysInline, extern_] } else { vec![extern_] };
        let attrs = self.method_attrs(sym, attrs);
        let pos = self.pos_or(self.l.tpd.span(rhs), self.no_pos);
        let name = self.extern_name(owner, target, span)?;
        Ok(Some(Defn { name, attrs, pos, kind: DefnKind::Declare { ty } }))
    }

    fn is_blocking(&mut self, sym: SymbolId) -> bool {
        self.l.u.is_annotated(sym, BLOCKING)
    }

    /// Whether `t` is `extern` or `extern()`.
    fn is_extern_body(&self, t: TNodeId) -> bool {
        let t = match *self.l.tpd.tree(t) {
            Tree::Apply { fun, args } if self.l.tpd.items(args).is_empty() => fun,
            _ => t,
        };
        self.l.tpd.sym(t).is_some_and(|s| self.l.u.full_name(s) == EXTERN_BODY)
    }

    /// The name in class `owner` of extern method `m`: its `@name`, else its simple name.
    pub fn extern_name(&mut self, owner: SymbolId, m: SymbolId, span: syntax::ast::Span) -> Result<GlobalId> {
        let owner = self.type_name(owner);
        let mut id = self.l.u.symbol(m).name.clone();
        if self.l.u.is_annotated(m, NAME) {
            let given = if self.l.u.symbol(m).tree.is_some() {
                self.l.u.annotation_string(m, NAME)
            } else {
                let class = self.l.u.annotations(m).iter().copied().find(|&c| self.l.u.full_name(c) == NAME);
                class.and_then(|c| self.l.u.annotation_strings(m, c).first().cloned().flatten())
            };
            let Some(given) = given else { return unsupported(span, "@name without a string literal") };
            id = given;
        }
        Ok(self.pool.member(owner, &Sig::Extern(id)))
    }

    /// The C signature of extern method `m`: unboxed parameters and result, no receiver.
    pub fn extern_sig(&mut self, m: SymbolId, span: syntax::ast::Span) -> Result<TypeId> {
        let (params, result) = self.l.u.erased_method(m);
        let vararg = self.extern_vararg_index(m);
        if vararg.is_some_and(|i| i + 1 != params.len()) {
            return unsupported(span, "extern methods with non-final variable arguments");
        }
        for (i, p) in params.iter().chain([&result]).enumerate() {
            if matches!(p, Erased::Class(n) if n == "scala.collection.immutable.Seq") && vararg != Some(i) {
                return unsupported(span, "extern methods with variable arguments");
            }
            if matches!(p, Erased::Class(n) if n == "scala.scalanative.unsafe.CFuncPtr") {
                return unsupported(span, "extern methods of a `CFuncPtr`");
            }
        }
        let args = params
            .iter()
            .enumerate()
            .map(|(i, p)| if vararg == Some(i) { self.ty(Type::Vararg) } else { self.extern_type(p) })
            .collect();
        let ret = self.extern_type(&result);
        Ok(self.ty(Type::Function(args, ret)))
    }

    pub fn extern_type(&mut self, e: &Erased) -> TypeId {
        match e {
            Erased::Class(name) if func_arity(name).is_some() => self.ty(Type::Ptr),
            Erased::Class(name) if let Some(t) = unboxed(name) => self.ty(t),
            e => self.erased(e),
        }
    }
}
