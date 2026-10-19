//! Splices and quotes of macro bodies, read as dotty types them: `${ e }` as
//! `scala.quoted.runtime.Expr.splice[T](e)`, `'{ e }` as `scala.quoted.runtime.Expr.quote[T](e)`.

use super::{Refusal, Unpickler};
use crate::defn::ref_type;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::TName;
use tasty::tags::{MACRO, OBJECT};
use tasty::{Kind, NodeId, Type};

const NO_RUNTIME: Refusal = "a library without scala.quoted.runtime.Expr";

impl Unpickler<'_> {
    /// Whether the body read is a macro's.
    pub fn in_macro(&self) -> bool {
        self.u.symbol(self.meth).flags.has(MACRO)
    }

    /// `SPLICE`.
    pub fn splice(&mut self, id: NodeId) -> Result<TNodeId, Refusal> {
        self.runtime_call(id, "splice")
    }

    /// `QUOTE`.
    pub fn quote(&mut self, id: NodeId) -> Result<TNodeId, Refusal> {
        self.runtime_call(id, "quote")
    }

    /// `scala.quoted.runtime.Expr.<name>[T](e)` of the expression and type tree at `id`.
    fn runtime_call(&mut self, id: NodeId, name: &str) -> Result<TNodeId, Refusal> {
        let k = self.kids(id);
        let (arg, tpt) = (self.term(k[0])?, self.tpt(k[1])?);
        let fun = self.runtime_expr(name)?;
        let tpe = self.type_apply_type(fun, &[tpt])?;
        let targs = self.tpd.list(&[tpt]);
        let fun = self.bare(T::TypeApply { fun, args: targs }, Some(tpe), None);
        let tpe = self.apply_type(fun, &[arg])?;
        let args = self.tpd.list(&[arg]);
        Ok(self.bare(T::Apply { fun, args }, Some(tpe), None))
    }

    /// A reference to method `name` of object `scala.quoted.runtime.Expr`.
    fn runtime_expr(&mut self, name: &str) -> Result<TNodeId, Refusal> {
        let pkg = self.u.package("scala.quoted.runtime").ok_or(NO_RUNTIME)?;
        let decls = self.u.decl(pkg, "Expr").unwrap_or_default();
        let is_module =
            |u: &tasty::Universe, s| u.symbol(s).kind == Kind::Val && u.symbol(s).flags.has(OBJECT);
        let module = decls.into_iter().find(|&s| is_module(self.u, s)).ok_or(NO_RUNTIME)?;
        let class = self.u.module_class(module).ok_or(NO_RUNTIME)?;
        let found = self.u.decl(class, name).unwrap_or_default();
        let [sym] = found[..] else { return Err(NO_RUNTIME) };
        let pkg = self.u.intern_type(Type::Package(pkg));
        let module = ref_type(self.u, Some(pkg), module);
        let tpe = ref_type(self.u, Some(module), sym);
        let name = TName { name: self.names.intern(name), is_type: false };
        Ok(self.bare(T::Ident { name }, Some(tpe), Some(sym)))
    }
}
