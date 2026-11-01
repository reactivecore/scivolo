// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/Inlines.scala
//! `scala.compiletime.testing.typeChecks` and `typeCheckErrors` (dotty's `Inlines.Intrinsics`).

use super::{CheckedErrors, Inlining, Unsupported};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::Span;
use syntax::ast::constant::Constant as C;
use syntax::source::SourceFile;
use tasty::tags::INLINE;
use tasty::{Kind, SymbolId};

impl Inlining<'_> {
    /// The constant string `code` of `typeChecks(code)` and the errors of parsing, typing it in
    /// the context of the outermost call and expanding it.
    pub fn checked_errors(
        &mut self,
        arg: TNodeId,
        owner: SymbolId,
        span: Span,
    ) -> Result<(String, CheckedErrors), Unsupported> {
        let mut r = super::Reflect { u: &mut *self.u, tpd: &mut *self.tpd, names: &mut *self.names, span };
        let underlying = r.underlying(arg);
        let mut code = self.stripped(underlying);
        let meth = self.tpd.sym(self.tpd.meth_part(code)).map(|m| self.u.symbol(m));
        if meth.is_some_and(|m| m.kind == Kind::Def && m.flags.has(INLINE)) {
            let outer = std::mem::replace(&mut self.force_inline, true);
            let inlined = self.transform(code, owner);
            self.force_inline = outer;
            code = inlined?;
        }
        let Some(C::String(code)) = self.code_constant(code) else {
            return Err((span, "typeChecks of code that is not a constant string"));
        };
        let code = String::from_utf16_lossy(&code);
        let ctx = self.ctx.ok_or((span, "typeChecks without the call's context"))?;
        let mark = self.host.error_count();
        let lent = (&mut *self.u, &mut *self.tpd, &mut *self.names, &mut *self.fresh);
        let errors = match self.host.typed_code(lent, &code, ctx) {
            Ok(typed) => {
                let (bodies, with_inits) = (self.bodies.clone(), self.with_inits.clone());
                let parts = ((&mut *self.u, &mut *self.tpd), (&mut *self.names, &mut *self.fresh));
                let mut inlining = Inlining::new(parts.0, parts.1, &mut *self.host, false);
                (inlining.bodies, inlining.with_inits) = (bodies, with_inits);
                inlining.transform(typed, owner)?;
                self.host.errors_since(mark)
            }
            Err(errors) => errors,
        };
        Ok((code, errors))
    }

    /// The constant `t` is without typed, empty blocks and inlined trees without bindings, or
    /// its type's, folded (dotty's `ConstFold`).
    fn code_constant(&mut self, t: TNodeId) -> Option<C> {
        let code = self.stripped(t);
        self.fold_constants(code);
        match self.tpd.tree(code) {
            T::Literal { value } => Some(value.clone()),
            _ => self.constant_of_type(self.tpd.tpe(code)),
        }
    }

    fn stripped(&self, mut t: TNodeId) -> TNodeId {
        loop {
            t = match *self.tpd.tree(t) {
                T::Typed { expr, .. } => expr,
                T::Block { stats, expr } if stats.is_empty() => expr,
                T::Inlined { bindings, expansion, .. } if bindings.is_empty() => expansion,
                _ => return t,
            };
        }
    }

    /// The list of errors `typeCheckErrors(code)` call `call` returns for the errors `errors` of
    /// expansions in `code`: `Error(message, line, column, ErrorKind.Typer)` values ascribed the
    /// call's type (dotty's `packErrors`); refused for an error outside the code.
    pub fn error_values(
        &mut self,
        call: TNodeId,
        code: &str,
        errors: &[(Span, String)],
        span: Span,
    ) -> Result<TNodeId, Unsupported> {
        let text = SourceFile::new("tasty-reflect", format!("{{\n{code}\n}}"));
        let testing = "_root_.scala.compiletime.testing";
        let mut values = Vec::new();
        for (at, message) in errors {
            let offset = at.in_own_source().point();
            if !at.is_in_other_source() || offset as usize >= text.text.len() {
                return Err((span, "typeCheckErrors of an error outside its code"));
            }
            let start = text.line_to_offset(text.offset_to_line(offset)) as usize;
            let line = text.text[start..].lines().next().unwrap_or("").trim_end_matches([' ', '\n']);
            let column = text.column(offset);
            let (message, line) = (quoted(message), quoted(line));
            values.push(format!("{testing}.Error({message}, {line}, {column}, {testing}.ErrorKind.Typer)"));
        }
        let list = format!("_root_.scala.collection.immutable.List[{testing}.Error]({})", values.join(", "));
        let ctx = self.ctx.ok_or((span, "typeCheckErrors without the call's context"))?;
        let lent = (&mut *self.u, &mut *self.tpd, &mut *self.names, &mut *self.fresh);
        let typed =
            self.host.typed_code(lent, &list, ctx).map_err(|_| (span, "the errors of typeCheckErrors"))?;
        let list = self.stripped(typed);
        let mut todo = vec![list];
        while let Some(t) = todo.pop() {
            self.tpd.get_mut(t).span = span.to_synthetic();
            todo.extend(self.tpd.children(t));
        }
        let tpe = self.tpd.tpe(call);
        let tpt = self.tpd.node(T::TypeTree { inferred: true }, span.to_synthetic(), Some(tpe), None);
        Ok(self.tpd.node(T::Typed { expr: list, tpt }, span, Some(tpe), None))
    }

    /// The empty list of errors `typeCheckErrors(code)` call `call` returns for code without
    /// errors: `Nil` ascribed the call's type.
    pub fn no_errors(&mut self, call: TNodeId, span: Span) -> Result<TNodeId, Unsupported> {
        let immutable =
            self.u.package("scala.collection.immutable").ok_or((span, "scala.collection.immutable"))?;
        let nil =
            self.u.decl(immutable, "Nil").unwrap_or_default().into_iter().find(|&d| self.u.is_module(d));
        let nil = nil.ok_or((span, "scala.collection.immutable.Nil"))?;
        let mut r = super::Reflect { u: &mut *self.u, tpd: &mut *self.tpd, names: &mut *self.names, span };
        let nil = r.local_ident(nil).map_err(|what| (span, what))?;
        let tpe = self.tpd.tpe(call);
        let tpt = self.tpd.node(T::TypeTree { inferred: true }, span.to_synthetic(), Some(tpe), None);
        Ok(self.tpd.node(T::Typed { expr: nil, tpt }, span, Some(tpe), None))
    }
}

/// `s` as a string literal.
fn quoted(s: &str) -> String {
    let mut q = String::from("\"");
    s.chars().for_each(|c| tasty::escape(&mut q, c));
    q.push('"');
    q
}
