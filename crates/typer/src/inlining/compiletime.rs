// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/Inliner.scala
//   compiler/src/dotty/tools/dotc/inlines/Inlines.scala
//! The operations of `scala.compiletime` that reduce without their bodies, and `error` (dotty's
//! `InlineCall.expand` intrinsics, `Inliner.issueError`).

use super::{Inlining, Unsupported};
use crate::tpd::{TNodeId, Tree as T, constant_type_of};
use syntax::ast::constant::Constant as C;
use syntax::ast::{Span, TName};
use tasty::{Kind, SymbolId, Type};

impl Inlining<'_> {
    /// The name of the `scala.compiletime` operation `call` calls, if it calls one.
    pub fn compiletime_op(&self, call: TNodeId) -> Option<String> {
        let meth = self.tpd.sym(self.tpd.meth_part(call))?;
        let owner = self.u.symbol(meth).owner?;
        let package = self.u.symbol(owner).owner?;
        let ours = crate::defn::is_named_package_object(self.u, owner)
            && self.u.full_name(package) == "scala.compiletime";
        ours.then(|| self.u.symbol(meth).name.clone())
    }

    /// What `call`, owned by `owner`, of `scala.compiletime` operation `op` reduces to without its
    /// body, if it does; an implicit it summons with its inline calls expanded.
    pub fn intrinsic(
        &mut self,
        op: Option<&str>,
        call: TNodeId,
        owner: SymbolId,
    ) -> Result<Option<TNodeId>, Unsupported> {
        let Some(op) = op else { return Ok(None) };
        let span = self.tpd.span(call);
        let (targs, vargss) = self.call_args(call);
        let arg = match vargss.as_slice() {
            [args] if args.len() == 1 => Some(args[0]),
            _ => None,
        };
        let targ = (targs.len() == 1).then(|| targs[0]);
        Ok(Some(match (op, arg, targ) {
            ("requireConst", Some(arg), _) => {
                if !self.is_constant_value(arg) {
                    return Err((self.tpd.span(arg), "requireConst of a value that is not constant"));
                }
                let tpe = self.u.class_ref("scala.Unit");
                self.tpd.node(T::Literal { value: C::Unit }, span, Some(tpe), None)
            }
            ("codeOf", Some(arg), _) => self.code_of(arg, span)?,
            ("constValue", _, Some(t)) => {
                let tpe = self.tpd.tpe(t);
                let value =
                    self.const_value(tpe).ok_or((span, "constValue of a type that is not constant"))?;
                self.literal(value, span)
            }
            ("constValueOpt", _, Some(t)) => self.const_value_opt(t, span)?,
            ("summonInline", _, Some(t)) => {
                let (tpe, at) = (self.tpd.tpe(t), self.tpd.span(t).start_pos());
                let found = self.summon(tpe, at)?.ok_or((span, "no implicit argument"))?;
                self.transform(found, owner)?
            }
            ("constValueTuple", _, Some(t)) => self.tuple_intrinsic(t, false, span)?,
            ("summonAll", _, Some(t)) => self.tuple_intrinsic(t, true, span)?,
            _ => return Ok(None),
        }))
    }

    /// The type arguments of `call` and its value arguments, clause by clause.
    fn call_args(&self, call: TNodeId) -> (Vec<TNodeId>, Vec<Vec<TNodeId>>) {
        let (mut t, mut targs, mut vargss) = (call, Vec::new(), Vec::new());
        loop {
            match *self.tpd.tree(t) {
                T::Apply { fun, args } => {
                    vargss.insert(0, self.tpd.items(args).to_vec());
                    t = fun;
                }
                T::TypeApply { fun, args } => {
                    targs = self.tpd.items(args).to_vec();
                    t = fun;
                }
                _ => return (targs, vargss),
            }
        }
    }

    /// Whether `t` is constant by its type, or an inlined argument ascribed one (dotty's
    /// `ConstantValue` in `requireConst`).
    fn is_constant_value(&mut self, t: TNodeId) -> bool {
        let inner = match *self.tpd.tree(t) {
            T::Inlined { bindings, expansion, .. } if bindings.is_empty() => {
                match *self.tpd.tree(expansion) {
                    T::Typed { expr, .. } => expr,
                    _ => t,
                }
            }
            _ => t,
        };
        [t, inner]
            .iter()
            .any(|&x| self.tpd.get(x).tpe.is_some_and(|tpe| self.constant_of_type(tpe).is_some()))
    }

    pub fn literal(&mut self, value: C, span: Span) -> TNodeId {
        let tpe = constant_type_of(self.u, &value);
        self.tpd.node(T::Literal { value }, span, Some(tpe), None)
    }

    /// `Some(c)` for the constant `c` of type argument `t`.
    fn const_value_opt(&mut self, t: TNodeId, span: Span) -> Result<TNodeId, Unsupported> {
        let tpe = self.tpd.tpe(t);
        let Some(value) = self.const_value(tpe) else {
            return Err((span, "constValueOpt of a type that is not constant"));
        };
        let some = self.scala_member("Some", Kind::Class).ok_or((span, "scala.Some"))?;
        let ctor = self.u.decl(some, "<init>").ok().and_then(|d| d.first().copied()).ok_or((span, "Some"))?;
        let literal = self.literal(value, span);
        let elem = self.tpd.tpe(literal);
        let at = span.start_pos();
        let tycon = crate::defn::type_ref(self.u, some);
        let tpt = self.tpd.node(T::TypeTree { inferred: false }, at, Some(tycon), None);
        let new = self.tpd.node(T::New { tpt }, at, Some(tycon), None);
        let ctor_ref = self.u.intern_type(Type::Sym(Some(tycon), ctor));
        let name = self.term_name(ctor);
        let select = self.tpd.node(T::Select { qualifier: new, name }, at, Some(ctor_ref), Some(ctor));
        let applied = crate::defn::applied(self.u, some, vec![elem]);
        let binder = self.u.fresh_binder();
        let params = vec![("value".to_owned(), elem)];
        let method = self.u.intern_type(Type::Method {
            binder,
            params,
            result: applied,
            implicit: false,
            contextual: false,
        });
        let targ = self.tpd.node(T::TypeTree { inferred: false }, at, Some(elem), None);
        let targs = self.tpd.list(&[targ]);
        let type_apply = self.tpd.node(T::TypeApply { fun: select, args: targs }, at, Some(method), None);
        let args = self.tpd.list(&[literal]);
        Ok(self.tpd.node(T::Apply { fun: type_apply, args }, span.to_synthetic(), Some(applied), None))
    }

    /// The member `name` of kind `kind` of package `scala`.
    pub fn scala_member(&mut self, name: &str, kind: Kind) -> Option<SymbolId> {
        let scala = self.u.package("scala")?;
        self.u.decl(scala, name).ok()?.into_iter().find(|&s| self.u.symbol(s).kind == kind)
    }

    pub fn term_name(&mut self, s: SymbolId) -> TName {
        TName { name: self.names.intern(&self.u.symbol(s).name.clone()), is_type: false }
    }

    /// The source text of argument `arg` as dotty shows it, for a literal whose text is the same.
    fn code_of(&mut self, arg: TNodeId, span: Span) -> Result<TNodeId, Unsupported> {
        let inner = match *self.tpd.tree(arg) {
            T::Inlined { call: None, bindings, expansion } if bindings.is_empty() => expansion,
            _ => arg,
        };
        let text = match self.tpd.tree(inner) {
            T::Literal { value: C::Int(n) } => n.to_string(),
            T::Literal { value: C::Long(n) } => format!("{n}L"),
            T::Literal { value: C::Boolean(b) } => b.to_string(),
            _ => return Err((span, "codeOf an argument other than an Int, Long or Boolean literal")),
        };
        Ok(self.literal(C::String(text.encode_utf16().collect()), span))
    }

    /// Reports the message of `call` of `error` at the outermost call.
    pub fn issue_error(&mut self, call: TNodeId) -> Result<(), Unsupported> {
        let (_, vargss) = self.call_args(call);
        let [args] = vargss.as_slice() else { return Ok(()) };
        let [msg] = args.as_slice() else { return Ok(()) };
        let tpe = self.tpd.tpe(*msg);
        let Some(C::String(text)) = self.constant_of_type(tpe) else {
            return Err((self.tpd.span(*msg), "compiletime.error with a message that is not a literal"));
        };
        let text = String::from_utf16_lossy(&text);
        self.error(self.outermost_span, text);
        Ok(())
    }
}
