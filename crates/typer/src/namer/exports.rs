//! Export clauses: the forwarders they define in their class (dotty's `Namer.exportForwarders`
//! and `processExports`).

use crate::ctx::{Ctx, Mode};
use crate::typer::Pt;
use crate::typer::Typer;
use crate::typer::stability::is_stable_path;
use syntax::ast::flags::{self as f, FlagSet};
use syntax::ast::{List, NodeId, Span, Tree};
use tasty::tags as t;
use tasty::{Kind, SymbolId, Type, TypeId};

/// A forwarder of an export clause: `sym` forwards to member `target` of the clause's path.
#[derive(Clone, Copy)]
pub struct Forwarder {
    pub sym: SymbolId,
    pub target: SymbolId,
    /// The span of the selector that defines it.
    pub span: Span,
}

/// What an export clause defines: its forwarders, and in an extension the extension method its
/// path refers to.
#[derive(Clone)]
pub struct Exported {
    pub path_method: Option<SymbolId>,
    pub forwarders: Vec<Forwarder>,
}

/// Selector `name as rename`: `name` empty for `given`, `_` for a wildcard; `rename` `_` omits.
struct Selector {
    name: String,
    rename: String,
    span: Span,
}

/// An export clause being elaborated in class `cls`: the type of its path, its path method, the
/// aliases its named selectors took, the forwarders so far.
pub struct Export {
    pub cls: SymbolId,
    pub path: TypeId,
    pub path_method: Option<SymbolId>,
    pub ctx: Ctx,
    targets: Vec<String>,
    pub out: Vec<Forwarder>,
}

impl Export {
    /// Whether a named selector of the clause took `alias` already.
    pub fn has_target(&self, alias: &str) -> bool {
        self.targets.iter().any(|a| a == alias)
    }
}

/// The term flags a forwarder keeps from its target (dotty's `RetainedExportTermFlags`).
const RETAINED: [(u8, FlagSet); 4] = [
    (t::INFIX, f::INFIX),
    (t::GIVEN, f::GIVEN),
    (t::IMPLICIT, f::IMPLICIT),
    (t::EXTENSION, f::EXTENSION_METHOD),
];

impl Typer<'_> {
    /// Defines the forwarders of the export clauses among `body`, the statements of class `cls`
    /// indexed in `ctx`.
    pub fn process_exports(&mut self, cls: SymbolId, body: &[NodeId], mut ctx: Ctx) {
        for &stat in body {
            if let Tree::Export { expr, selectors, ext_clauses } = *self.ast.tree(stat) {
                let path_method = match ext_clauses {
                    0 => None,
                    n => match self.export_path_method(cls, expr, n as usize) {
                        Some(m) => Some(m),
                        None => continue,
                    },
                };
                let forwarders = self.export_forwarders(cls, (expr, selectors), path_method, ctx);
                self.exports.insert(stat, Exported { path_method, forwarders });
            }
            ctx = self.after_stat(stat, ctx);
        }
    }

    /// The extension method of class `cls` that path `expr` of an export in an extension of `n`
    /// clauses refers to: one without clauses of its own (dotty's `exportPathSym`).
    fn export_path_method(&mut self, cls: SymbolId, expr: NodeId, n: usize) -> Option<SymbolId> {
        let found = match *self.ast.tree(expr) {
            Tree::Ident { name } => {
                let name = self.text(name.name).to_owned();
                let decls = self.u.decl(cls, &name).unwrap_or_default();
                let extension = |t: &Self, d: &SymbolId| {
                    t.syms.flags.get(d).is_some_and(|fl| fl.is(f::EXTENSION_METHOD))
                        && t.syms.def_trees.get(d).is_some_and(|&tree| t.def_clauses(tree).len() == n)
                };
                decls.into_iter().filter(|d| extension(self, d)).collect()
            }
            _ => Vec::new(),
        };
        match found[..] {
            [m] => Some(m),
            _ => {
                self.unsupported(expr, "export qualifier that is not one parameterless extension method");
                None
            }
        }
    }

    fn export_forwarders(
        &mut self,
        cls: SymbolId,
        (expr, selectors): (NodeId, List),
        path_method: Option<SymbolId>,
        ctx: Ctx,
    ) -> Vec<Forwarder> {
        let path = match path_method {
            Some(m) => {
                self.complete(m);
                let mut t = self.u.info(m);
                while let Type::Poly { result, .. } | Type::Method { result, .. } = *self.u.types().get(t) {
                    t = result;
                }
                t
            }
            None => {
                let path = self.typed(expr, Pt::Any, ctx.with_mode(Mode::QUALIFIER));
                self.typed_ahead(expr, path, ctx);
                let package = self.tpd.sym(path).is_some_and(|s| self.u.symbol(s).kind == Kind::Package);
                if package || !self.is_stable_path(path) {
                    self.unsupported(expr, "export prefix that is not a stable path to a value");
                    return Vec::new();
                }
                self.tpd.tpe(path)
            }
        };
        let Some(sels) = self.export_selectors(selectors) else { return Vec::new() };
        let mut e = Export { cls, path, path_method, ctx, targets: Vec::new(), out: Vec::new() };
        let mut seen = Vec::new();
        for sel in &sels {
            if sel.name.is_empty() || sel.name == "_" {
                let given = sels.iter().any(|s| s.name.is_empty());
                let plain = sels.iter().any(|s| s.name == "_");
                self.add_wildcard_forwarders(&mut e, &seen, (given, plain), sel.span);
                break;
            }
            if sel.rename != "_" {
                self.add_forwarders_named(&mut e, sel);
            }
            seen.push(sel.name.clone());
        }
        self.check_clashes(&e.out);
        e.out
    }

    fn export_selectors(&mut self, selectors: List) -> Option<Vec<Selector>> {
        let mut out = Vec::new();
        for s in self.ast.items(selectors).to_vec() {
            let Tree::ImportSelector { imported, renamed, bound } = *self.ast.tree(s) else {
                self.unsupported(s, "export selector");
                return None;
            };
            if let Some(b) = bound {
                self.unsupported(b, "export bound");
                return None;
            }
            let ident = |t: &Self, id: NodeId| match *t.ast.tree(id) {
                Tree::Ident { name } => Some(t.text(name.name).to_owned()),
                _ => None,
            };
            let (Some(name), rename) = (ident(self, imported), renamed.map(|r| ident(self, r))) else {
                self.unsupported(s, "export selector");
                return None;
            };
            let rename = rename.flatten().unwrap_or_else(|| name.clone());
            out.push(Selector { name, rename, span: self.ast.span(s) });
        }
        Some(out)
    }

    fn add_forwarders_named(&mut self, e: &mut Export, sel: &Selector) {
        let before = e.out.len();
        for m in self.terms_then_types(e.path, &sel.name) {
            self.add_forwarder(e, &sel.rename, m, sel.span);
        }
        if e.out.len() == before {
            self.unsupported_at(sel.span, "export of no eligible member");
        } else {
            e.targets.push(sel.rename.clone());
        }
    }

    /// Adds a forwarder named `alias` to member `m` of `e`'s path if it may have one (dotty's
    /// `addForwarder`).
    pub fn add_forwarder(&mut self, e: &mut Export, alias: &str, m: SymbolId, span: Span) {
        if self.can_forward(e, m, alias) != Some(true) {
            return;
        }
        if self.u.is_type_symbol(m) {
            return self.add_type_forwarder(e, alias, m, span);
        }
        let flags = self.u.symbol(m).flags;
        if [t::INLINE, t::ERASED, t::MACRO].iter().any(|&fl| flags.has(fl)) {
            self.unsupported_at(span, "export of an inline or erased member");
            return;
        }
        let mut fl = RETAINED
            .iter()
            .filter(|&&(tag, _)| flags.has(tag))
            .fold(f::EXPORTED.or(f::METHOD).or(f::FINAL), |acc, &(_, fl)| acc.or(fl));
        let public = !flags.has(t::PRIVATE) && !flags.has(t::PROTECTED) && !flags.has(t::PRIVATEqualified);
        let stable = self.is_stable_member(m)
            && public
            && e.path_method.is_none()
            && is_stable_path(self.u, e.path)
            && !self.refers_to_private(e.path);
        if stable {
            fl = fl.or(f::STABLE_REALIZABLE);
        }
        if e.path_method.is_some() {
            fl = fl.or(f::EXTENSION_METHOD);
        }
        let sym = self.syms.define(self.u, alias, Kind::Def, e.cls, fl);
        if stable {
            let target = self.u.intern_type(Type::Sym(Some(e.path), m));
            self.set_forwarder_result(sym, Vec::new(), target);
        } else {
            self.set_forwarder_info(sym, m, e.path, e.path_method);
        }
        e.out.push(Forwarder { sym, target: m, span });
        self.add_default_forwarders(e, m, alias, span);
    }

    /// Whether path type `t` refers to a private term (dotty's `refersToPrivate`).
    fn refers_to_private(&self, t: TypeId) -> bool {
        match *self.u.types().get(t) {
            Type::Sym(p, s) if self.u.symbol(s).kind.is_term() => {
                self.u.symbol(s).flags.has(t::PRIVATE) || p.is_some_and(|p| self.refers_to_private(p))
            }
            _ => false,
        }
    }

    /// The type alias forwarding to type member `m` of `e`'s path.
    fn add_type_forwarder(&mut self, e: &mut Export, alias: &str, m: SymbolId, span: Span) {
        let info = self.u.info(m);
        let class_params = self.u.symbol(m).kind == Kind::Class
            && self.u.symbol(m).decls.iter().any(|&d| self.u.symbol(d).kind == Kind::TypeParam);
        if class_params || matches!(self.u.types().get(info), Type::Lambda { .. }) {
            self.unsupported_at(span, "export of a type constructor");
            return;
        }
        let mut target = self.u.intern_type(Type::Sym(Some(e.path), m));
        if self.refers_to_private(e.path) {
            let dealiased = self.u.dealias(target);
            if dealiased == target {
                self.unsupported_at(span, "export of a type through a private path");
                return;
            }
            target = dealiased;
        }
        let fl = f::EXPORTED.or(f::FINAL).or(if self.u.symbol(m).flags.has(t::INFIX) {
            f::INFIX
        } else {
            f::EMPTY
        });
        let sym = self.syms.define(self.u, alias, Kind::Type, e.cls, fl);
        let info = self.u.intern_type(Type::Alias(target));
        self.u.set_info(sym, info);
        e.out.push(Forwarder { sym, target: m, span });
    }

    /// Fails on two forwarders of the same name that erase alike (dotty's `avoidClashes`, which
    /// keeps the one overloading resolution prefers).
    fn check_clashes(&mut self, out: &[Forwarder]) {
        for (i, a) in out.iter().enumerate() {
            for b in &out[i + 1..] {
                let same_name = self.u.symbol(a.sym).name == self.u.symbol(b.sym).name;
                if same_name
                    && self.u.symbol(a.sym).kind == Kind::Def
                    && self.u.symbol(b.sym).kind == Kind::Def
                    && self.u.erased_method(a.sym) == self.u.erased_method(b.sym)
                {
                    self.unsupported_at(b.span, "export forwarders that erase alike");
                    return;
                }
            }
        }
    }
}
