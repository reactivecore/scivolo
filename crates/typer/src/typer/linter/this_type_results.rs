// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/TreeInfo.scala
//! Calls that return their receiver or an argument (dotty's `isThisTypeResult`), whose values
//! `-Wnonunit-statement` does not warn about.

use super::Linter;
use crate::tpd::{TNodeId, Tree as T};
use tasty::{Kind, SymbolId, Type, TypeId, Universe};

impl Linter<'_> {
    /// Whether application `t` returns its receiver or an argument, as `sb.append(x)` with a
    /// result type `sb.type` or `this.type` does.
    pub fn is_this_type_result(&self, u: &mut Universe, t: TNodeId) -> bool {
        let tpe = self.tpd.tpe(t);
        let core = self.tpd.meth_part(t);
        let T::Select { qualifier: receiver, .. } = *self.tpd.tree(core) else {
            return matches!(u.types().get(tpe), Type::This(_));
        };
        let recv = match *self.tpd.tree(receiver) {
            T::This { .. } => singleton_symbol(u, self.tpd.tpe(receiver)),
            _ => self.tpd.sym(receiver),
        };
        if let Some(s) = singleton_symbol(u, tpe) {
            return Some(s) == recv || self.term_args(t).into_iter().any(|a| self.tpd.sym(a) == Some(s));
        }
        let Some(f) = self.tpd.sym(core) else { return false };
        let mut info = u.info(f);
        loop {
            match *u.types().get(info) {
                Type::Method { result, .. } => match singleton_symbol(u, result) {
                    Some(s) => {
                        let applied = matches!(self.tpd.tree(receiver), T::Apply { .. });
                        return check_single(u, s, recv, applied, f);
                    }
                    None => info = result,
                },
                Type::Poly { result, .. } => info = result,
                _ => return false,
            }
        }
    }

    /// The arguments of all argument lists of application `t`.
    fn term_args(&self, t: TNodeId) -> Vec<TNodeId> {
        match *self.tpd.tree(t) {
            T::Apply { fun, args } => [self.tpd.items(args), &self.term_args(fun)].concat(),
            T::TypeApply { fun, .. } => self.term_args(fun),
            _ => Vec::new(),
        }
    }
}

/// Whether a call of `f` whose result is the singleton `s` returns its receiver `recv`: `s` is
/// `recv`, `f` is an assignment operator on an `applied` receiver, or `recv` is a stable value.
fn check_single(u: &Universe, s: SymbolId, recv: Option<SymbolId>, applied: bool, f: SymbolId) -> bool {
    if Some(s) == recv {
        return true;
    }
    if applied {
        let op = &u.symbol(f).name;
        let first = op.chars().next().unwrap_or('=');
        return op.ends_with('=')
            && !matches!(op.as_str(), "!=" | "<=" | ">=")
            && first != '='
            && !first.is_alphanumeric()
            && first != '_';
    }
    recv.is_some_and(|r| {
        let sym = u.symbol(r);
        matches!(sym.kind, Kind::Val | Kind::Param)
            || sym.kind == Kind::Def && sym.flags.has(tasty::tags::FIELDaccessor) && !sym.name.ends_with("_=")
    })
}

/// The symbol of `t` if it is a `this` type or a reference to a term.
fn singleton_symbol(u: &mut Universe, t: TypeId) -> Option<SymbolId> {
    match *u.types().get(t) {
        Type::This(c) => u.class_symbol(c),
        Type::Sym(_, s) if u.symbol(s).kind.is_term() => Some(s),
        Type::TermRef(..) => u.resolve_ref(t),
        _ => None,
    }
}
