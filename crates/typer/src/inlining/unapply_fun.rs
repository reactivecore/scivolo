// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/Inlines.scala
//! Extractors that call inline methods (dotty's `Inlines.inlinedUnapplyFun`): `fun` of a pattern
//! becomes `{ final class $anon { def unapply(s: S): R = fun(s) }; new $anon() }.unapply`, the
//! call `fun(s)` expanded.

use super::{Inlining, Unsupported};
use crate::defn::type_ref;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{Span, TName};
use tasty::tags::{FINAL, INLINE, SYNTHETIC};
use tasty::{Flags, Kind, Symbol, SymbolId, Type, TypeId};

impl Inlining<'_> {
    /// Function `fun` of an extractor pattern owned by `owner`, wrapped as above if it applies an
    /// inline method.
    pub fn inlined_unapply_fun(&mut self, fun: TNodeId, owner: SymbolId) -> Result<TNodeId, Unsupported> {
        let Some(meth) = self.method_part(fun).and_then(|f| self.tpd.sym(f)) else { return Ok(fun) };
        if self.transparent_only || !self.u.symbol(meth).flags.has(INLINE) {
            return Ok(fun);
        }
        let span = self.tpd.span(fun);
        let info = self.u.widen(self.tpd.tpe(fun));
        let Type::Method { params, result, .. } = self.u.types().get(info).clone() else {
            return Err((span, "inline extractor that is not a method"));
        };
        let [(ref param_name, param_type)] = params[..] else {
            return Err((span, "inline extractor of several parameters"));
        };
        if matches!(self.u.types().get(result), Type::Method { .. } | Type::Poly { .. }) {
            return Err((span, "inline extractor with implicit parameters"));
        }
        let object = self.u.builtins().object;
        let cls = self.synthetic("$anon", Kind::Class, owner, Flags::default().with(FINAL).with(SYNTHETIC));
        let object_type = type_ref(self.u, object);
        self.u.set_parents(cls, vec![object_type]);
        let cls_type = type_ref(self.u, cls);
        let ctor = self.synthetic("<init>", Kind::Def, cls, Flags::default().with(SYNTHETIC));
        let ctor_info = self.nullary(cls_type);
        self.u.set_info(ctor, ctor_info);
        let name = self.u.symbol(meth).name.clone();
        let unapply = self.synthetic(&name, Kind::Def, cls, Flags::default().with(SYNTHETIC));
        self.u.set_info(unapply, info);
        let param = self.synthetic(param_name, Kind::Param, unapply, Flags::default());
        self.u.set_info(param, param_type);
        self.change_owner(fun, owner, unapply);
        let (arg_name, arg_type) = (self.term_name(param), self.u.intern_type(Type::Sym(None, param)));
        let at = Span::at(span.end());
        let arg = self.tpd.node(T::Ident { name: arg_name }, at, Some(arg_type), Some(param));
        let args = self.tpd.list(&[arg]);
        let call = self.tpd.node(T::Apply { fun, args }, span, Some(result), None);
        let body = match self.inline_method(call)? {
            Some(def) => self.inline_call(call, def, unapply)?,
            None => call,
        };
        let vparam = self.param_def(param, param_type, span);
        let unapply_def = self.method_def(unapply, &[vparam], (result, Some(body)), span);
        let constr = self.method_def(ctor, &[], (cls_type, None), span);
        let object_ctor = self.u.decl(object, "<init>").ok().and_then(|d| d.first().copied());
        let object_ctor = object_ctor.ok_or((span, "Object without a constructor"))?;
        let super_call = self.new_call(object_type, object_ctor, span);
        let (parents, body) = (self.tpd.list(&[super_call]), self.tpd.list(&[unapply_def]));
        let template = T::Template { constr, parents, self_: None, body };
        let template = self.tpd.node(template, span, None, Some(cls));
        let cls_name = TName { name: self.names.intern("$anon"), is_type: true };
        let class_def = self.tpd.node(T::TypeDef { name: cls_name, rhs: template }, span, None, Some(cls));
        let new = self.new_call(cls_type, ctor, span);
        let stats = self.tpd.list(&[class_def]);
        let block = self.tpd.node(T::Block { stats, expr: new }, span, Some(cls_type), None);
        let tpe = self.u.intern_type(Type::Sym(Some(cls_type), unapply));
        let name = self.term_name(unapply);
        Ok(self.tpd.node(T::Select { qualifier: block, name }, span, Some(tpe), Some(unapply)))
    }

    fn synthetic(&mut self, name: &str, kind: Kind, owner: SymbolId, flags: Flags) -> SymbolId {
        self.u.define(Symbol {
            name: name.to_owned(),
            kind,
            owner: Some(owner),
            flags,
            target: None,
            is_extern: false,
            derived: false,
            decls: Vec::new(),
            tree: None,
        })
    }

    fn nullary(&mut self, result: TypeId) -> TypeId {
        let binder = self.u.fresh_binder();
        self.u.intern_type(Type::Method {
            binder,
            params: Vec::new(),
            result,
            implicit: false,
            contextual: false,
        })
    }

    fn param_def(&mut self, s: SymbolId, tpe: TypeId, span: Span) -> TNodeId {
        let name = self.term_name(s);
        let tpt = self.tpd.node(T::TypeTree { inferred: false }, span, Some(tpe), None);
        self.tpd.node(T::ValDef { name, tpt, rhs: None }, span, None, Some(s))
    }

    fn method_def(
        &mut self,
        s: SymbolId,
        params: &[TNodeId],
        (result, rhs): (TypeId, Option<TNodeId>),
        span: Span,
    ) -> TNodeId {
        let name = self.term_name(s);
        let clause = self.tpd.list(params);
        let paramss = self.tpd.clauses(&[clause]);
        let tpt = self.tpd.node(T::TypeTree { inferred: false }, span, Some(result), None);
        self.tpd.node(T::DefDef { name, paramss, tpt, rhs }, span, None, Some(s))
    }

    /// `new C()` of class type `tpe` through constructor `ctor`.
    fn new_call(&mut self, tpe: TypeId, ctor: SymbolId, span: Span) -> TNodeId {
        let tpt = self.tpd.node(T::TypeTree { inferred: false }, span, Some(tpe), None);
        let new = self.tpd.node(T::New { tpt }, span, Some(tpe), None);
        let ctor_type = self.u.intern_type(Type::Sym(Some(tpe), ctor));
        let name = TName { name: self.names.intern("<init>"), is_type: false };
        let select = self.tpd.node(T::Select { qualifier: new, name }, span, Some(ctor_type), Some(ctor));
        let args = self.tpd.list(&[]);
        self.tpd.node(T::Apply { fun: select, args }, span, Some(tpe), None)
    }
}
