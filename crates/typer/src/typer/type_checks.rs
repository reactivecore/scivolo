// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/Inlines.scala
//! The code of `scala.compiletime.testing.typeChecks` parsed and typed in the context of the
//! call (dotty's `Inlines.Intrinsics.compileForErrors`).

use super::{Pt, Typer};
use crate::ctx::{Ctx, Mode};
use crate::tpd::TNodeId;
use syntax::ast::{Ast, NodeId, Span, TName, Tree};
use syntax::config::Config;
use syntax::desugar::Desugar;
use syntax::diag::Level;
use syntax::parse::Parser;
use syntax::source::SourceFile;
use tasty::tags::{GIVEN, IMPLICIT, SYNTHETIC};
use tasty::{Flags, Kind, Symbol, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Statements `code` as a block typed in `ctx`, owned by a synthetic value of the context's
    /// owner; `None` if parsing or typing them reports an error, whose diagnostics and the
    /// constructs outside the subset it holds are dropped.
    pub fn typed_code_in(&mut self, code: &str, ctx: Ctx) -> Option<TNodeId> {
        let text = format!("{{\n{code}\n}}");
        let source = SourceFile::new("tasty-reflect", &text);
        let mut p = Parser::new(&source, self.names, Config::default());
        p.ast = std::mem::take(&mut *self.ast);
        p.fresh = std::mem::take(&mut *self.fresh);
        let first = p.ast.node(Tree::Empty).index();
        let stats = p.parse_stats();
        let cfg = p.s.cfg;
        let block = match stats[..] {
            [block] if !p.s.diags.has_errors() => Some(
                Desugar::new(&mut p.ast, p.s.names, &mut p.s.diags, &mut p.fresh, &source, cfg).unit(block),
            ),
            _ => None,
        };
        let parsed = !p.s.diags.has_errors();
        let end = p.ast.node(Tree::Empty).index();
        *self.ast = std::mem::take(&mut p.ast);
        *self.fresh = std::mem::take(&mut p.fresh);
        for i in first..end {
            self.ast.get_mut(NodeId::new(i)).span = Span::NONE;
        }
        let block = block.filter(|_| parsed)?;
        let dummy = self.u.define(Symbol {
            name: "$typingDummy".to_owned(),
            kind: Kind::Val,
            owner: None,
            flags: Flags(1 << SYNTHETIC),
            target: None,
            is_extern: false,
            derived: false,
            decls: Vec::new(),
            tree: None,
        });
        self.u.set_owner(dummy, ctx.owner);
        self.u.set_info(dummy, tasty::ANY);
        let (mark, erasure_mark) = (self.diags.items.len(), self.erasure_diags.items.len());
        let outer = (self.unsupported.take(), self.checked_code.replace((block, None)));
        let typed = self.typed(block, Pt::Any, ctx.with_owner(dummy));
        let first_refusal = std::mem::replace(&mut self.checked_code, outer.1).and_then(|(_, n)| n);
        let refused = std::mem::replace(&mut self.unsupported, outer.0);
        let end = first_refusal.unwrap_or(self.diags.items.len()).max(mark);
        let errors = self.diags.items[mark..end].iter().any(|d| d.level == Level::Error);
        if !errors && self.unsupported.is_none() {
            self.unsupported = refused;
        }
        if let (Some((_, first)), Some(f)) = (&mut self.checked_code, first_refusal) {
            *first = Some(first.unwrap_or(mark).min(f).min(mark));
        }
        self.diags.items.truncate(mark);
        self.erasure_diags.items.truncate(erasure_mark);
        (!errors).then_some(typed)
    }

    /// `MissingIdent` for identifier `id` named `name` of the code of `typeChecks`, outside a
    /// pattern, if no binding of all levels of `ctx` has its name; `None` otherwise.
    pub fn unbound_in_checked_code(&mut self, id: NodeId, name: TName, ctx: Ctx) -> Option<TNodeId> {
        let (root, _) = self.checked_code?;
        if ctx.mode.is(Mode::PATTERN) || !self.ast.exists_subtree(root, &|_, n| n == id) {
            return None;
        }
        let text = self.text(name.name).to_owned();
        let b = self.bindings(ctx)?;
        let same = |n: &String| *n == text || n.strip_suffix('$') == Some(text.as_str());
        let named = b.found.iter().map(|(n, ..)| n).chain(&b.left_out).chain(&b.unknown).any(same);
        (!named).then(|| self.error(id, super::errors::not_found(&text, name.is_type, "")))
    }

    /// Whether no implicit can have type `formal`, searched in the code of `typeChecks`: a fully
    /// defined class type of the unit, not a function, in a unit that derives no instance, each of
    /// whose implicits is of a class not deriving from it or of another monomorphic type.
    pub fn unsupplied_in_checked_code(&mut self, formal: TypeId) -> bool {
        let Some((root, _)) = self.checked_code else { return false };
        if matches!(self.u.types().get(formal), Type::ByName(_)) || !self.is_fully_defined(formal) {
            return false;
        }
        let Some(c) = self.direct_class(formal) else { return false };
        if !self.syms.flags.contains_key(&c) || self.u.is_module_class(c) {
            return false;
        }
        let function = self.u.linearization(c).into_iter().any(|b| {
            let name = self.u.full_name(b);
            name.starts_with("scala.Function") || name.starts_with("scala.ContextFunction")
        });
        let derives = |ast: &Ast, n: NodeId| matches!(ast.tree(n), Tree::Template { derived: Some(_), .. });
        if function || self.ast.exists_subtree(self.unit, &derives) || self.ast.exists_subtree(root, &derives)
        {
            return false;
        }
        let implicits: Vec<SymbolId> = (self.syms.flags.keys().copied())
            .filter(|&s| {
                let sym = self.u.symbol(s);
                sym.kind.is_term() && (sym.flags.has(IMPLICIT) || sym.flags.has(GIVEN))
            })
            .collect();
        implicits.into_iter().all(|s| self.cannot_supply(s, c, formal))
    }

    /// Whether implicit `s` is of a class not deriving from `c`, or of a monomorphic type other
    /// than `formal`.
    fn cannot_supply(&mut self, s: SymbolId, c: SymbolId, formal: TypeId) -> bool {
        let (mut t, mut poly) = (self.u.info(s), false);
        loop {
            t = match *self.u.types().get(t) {
                Type::Poly { result, .. } => {
                    poly = true;
                    result
                }
                Type::Method { result, .. } => result,
                _ => break,
            };
        }
        let t = self.u.widen(t);
        let Some(class) = self.direct_class(t) else { return false };
        !self.u.derives_from(class, c) || !poly && self.is_fully_defined(t) && !self.u.is_subtype(t, formal)
    }
}
