// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/TreeInfo.scala
//   compiler/src/dotty/tools/dotc/typer/Linter.scala
//! dotty's `Linter.warnOnInterestingResultInStatement`: `-Wnonunit-statement`.

use super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use syntax::config::Lints;
use syntax::diag::{Message, MessageId};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Warns about statement `t` if it has an interesting result under `-Wnonunit-statement`;
    /// whether it did.
    pub fn warn_on_interesting_result(&mut self, t: TNodeId) -> bool {
        if !self.lints.contains(Lints::NONUNIT_STATEMENT) || !self.interesting_shape(t) {
            return false;
        }
        let at = match *self.tpd.tree(t) {
            T::Block { expr, .. } => expr,
            T::If { thenp, elsep, .. } if matches!(self.tpd.tree(elsep), T::Literal { .. }) => {
                match *self.tpd.tree(thenp) {
                    T::Block { expr, .. } => expr,
                    _ => thenp,
                }
            }
            _ => t,
        };
        let tpe = self.tpd.tpe(at);
        let span = self.tpd.span(t);
        let method_ref = self.term_symbol(tpe).is_some_and(|s| self.u.symbol(s).kind == Kind::Def);
        let Some(shown) = self.found_text(tpe).filter(|_| !method_ref) else {
            self.unsupported_at(span, "an unused value of this type under -Wnonunit-statement");
            return true;
        };
        let text = format!("unused value of type {shown}");
        let id = MessageId { code: 176, kind: "Potential Issue", explained: false };
        self.diags.warning(span.point(), Message { text, id: Some(id) });
        true
    }

    fn interesting_shape(&mut self, t: TNodeId) -> bool {
        match self.tpd.tree(t).clone() {
            T::If { thenp, elsep, .. } => self.interesting_shape(thenp) || self.interesting_shape(elsep),
            T::Block { expr, .. } => self.interesting_shape(expr),
            T::Match { cases, .. } => {
                let cases = self.tpd.items(cases).to_vec();
                cases.into_iter().any(|c| match *self.tpd.tree(c) {
                    T::CaseDef { body, .. } => self.interesting_shape(body),
                    _ => false,
                })
            }
            _ => self.interesting_result(t),
        }
    }

    /// dotty's `checksForInterestingResult`.
    fn interesting_result(&mut self, t: TNodeId) -> bool {
        let sym = match *self.tpd.tree(t) {
            T::Ident { .. } | T::Select { .. } | T::Apply { .. } | T::TypeApply { .. } => {
                self.tpd.sym(self.tpd.meth_part(t))
            }
            _ => None,
        };
        let Some(sym) = sym else { return false };
        let tpe = self.tpd.tpe(t);
        !self.uninteresting_symbol(sym)
            && !self.uninteresting_type(tpe)
            && !self.is_this_type_result(t)
            && !self.is_java_application(t)
    }

    fn uninteresting_symbol(&mut self, s: SymbolId) -> bool {
        let b = self.u.builtins();
        let sym = self.u.symbol(s);
        let name = sym.name.trim_end_matches('$');
        name == "<init>"
            || sym.kind == Kind::Package
            || name == "package"
            || name.ends_with("$package")
            || [b.any, b.any_ref].contains(&s)
            || b.any_val == Some(s)
            || self.u.full_name(s) == "scala.runtime.BoxedUnit"
    }

    fn uninteresting_type(&mut self, t: TypeId) -> bool {
        let w = self.u.widen(t);
        if w == tasty::NOTHING || matches!(self.u.types().get(w), Type::Error) {
            return true;
        }
        let Some(c) = self.type_class(w) else { return false };
        let b = self.u.builtins();
        [self.defn.unit, b.nothing, b.null, b.any, b.object].contains(&c)
            || b.any_val == Some(c)
            || self.u.full_name(c) == "scala.runtime.BoxedUnit"
    }

    /// The class `t` names, through aliases but not abstract types (dotty's `typeSymbol`).
    fn type_class(&mut self, t: TypeId) -> Option<SymbolId> {
        let s = match self.u.types().get(t).clone() {
            Type::Sym(_, s) => s,
            Type::TypeRef(..) => self.u.resolve_ref(t)?,
            Type::Applied(inner, _) | Type::Annotated(inner, _) => return self.type_class(inner),
            _ => return None,
        };
        match self.u.symbol(s).kind {
            Kind::Class => Some(s),
            Kind::Type => {
                let info = self.u.info(s);
                match *self.u.types().get(info) {
                    Type::Alias(a) => self.type_class(a),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// The symbol of `t` if it is a `this` type or a reference to a term.
    fn singleton_symbol(&mut self, t: TypeId) -> Option<SymbolId> {
        match *self.u.types().get(t) {
            Type::This(c) => self.u.class_symbol(c),
            Type::Sym(_, s) if self.u.symbol(s).kind.is_term() => Some(s),
            Type::TermRef(..) => self.u.resolve_ref(t),
            _ => None,
        }
    }

    /// dotty's `isThisTypeResult`: an application whose result is its receiver or an argument.
    fn is_this_type_result(&mut self, t: TNodeId) -> bool {
        let tpe = self.tpd.tpe(t);
        let core = self.tpd.meth_part(t);
        let T::Select { qualifier: receiver, .. } = *self.tpd.tree(core) else {
            return matches!(self.u.types().get(tpe), Type::This(_));
        };
        let recv = match *self.tpd.tree(receiver) {
            T::This { .. } => {
                let r = self.tpd.tpe(receiver);
                self.singleton_symbol(r)
            }
            _ => self.tpd.sym(receiver),
        };
        if let Some(s) = self.singleton_symbol(tpe) {
            return Some(s) == recv || self.term_args(t).into_iter().any(|a| self.tpd.sym(a) == Some(s));
        }
        let Some(f) = self.tpd.sym(core) else { return false };
        let mut info = self.u.info(f);
        loop {
            match self.u.types().get(info).clone() {
                Type::Method { result, .. } => match self.singleton_symbol(result) {
                    Some(s) => return self.check_single(s, recv, receiver, f),
                    None => info = result,
                },
                Type::Poly { result, .. } => info = result,
                _ => return false,
            }
        }
    }

    fn check_single(&mut self, s: SymbolId, recv: Option<SymbolId>, receiver: TNodeId, f: SymbolId) -> bool {
        if Some(s) == recv {
            return true;
        }
        if matches!(self.tpd.tree(receiver), T::Apply { .. }) {
            let op = &self.u.symbol(f).name;
            let first = op.chars().next().unwrap_or('=');
            return op.ends_with('=')
                && !matches!(op.as_str(), "!=" | "<=" | ">=")
                && first != '='
                && !first.is_alphanumeric()
                && first != '_';
        }
        recv.is_some_and(|r| {
            let sym = self.u.symbol(r);
            matches!(sym.kind, Kind::Val | Kind::Param)
                || sym.kind == Kind::Def
                    && sym.flags.has(tasty::tags::FIELDaccessor)
                    && !sym.name.ends_with("_=")
        })
    }

    /// The arguments of all argument lists of application `t`.
    fn term_args(&self, t: TNodeId) -> Vec<TNodeId> {
        match *self.tpd.tree(t) {
            T::Apply { fun, args } => [self.tpd.items(args), &self.term_args(fun)].concat(),
            T::TypeApply { fun, .. } => self.term_args(fun),
            _ => Vec::new(),
        }
    }

    /// dotty's `isJavaApplication`: a call of a Java method not of `Object`.
    fn is_java_application(&mut self, t: TNodeId) -> bool {
        let T::Apply { fun, .. } = *self.tpd.tree(t) else { return false };
        let Some(f) = self.tpd.sym(fun) else { return false };
        let b = self.u.builtins();
        let owner = self.u.symbol(f).owner;
        self.is_java_member(f) && !owner.is_some_and(|o| [b.object, b.any, b.matchable].contains(&o))
    }
}
