// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Flags.scala
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//! Export clauses: the forwarders they define in their class (dotty's `Namer.exportForwarders`).

use crate::ctx::Ctx;
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
    /// The forwarders an export clause in class `cls` with path type `path` and `selectors`
    /// defines (dotty's `exportForwarders`).
    pub fn export_forwarders(
        &mut self,
        cls: SymbolId,
        (path, selectors): (TypeId, List),
        path_method: Option<SymbolId>,
        ctx: Ctx,
    ) -> Vec<Forwarder> {
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
        let public = ![t::PRIVATE, t::PROTECTED, t::PRIVATEqualified, t::PROTECTEDqualified]
            .iter()
            .any(|&fl| flags.has(fl));
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
        if let Some(name) = self.u.symbol(m).target.clone() {
            self.u.set_target(sym, &name);
        }
        if stable {
            let target = self.u.intern_type(Type::Sym(Some(e.path), m));
            self.set_forwarder_result(sym, Vec::new(), target);
        } else {
            self.set_forwarder_info(sym, m, e.path, e.path_method);
            let info = self.u.info(sym);
            if self.refers_to_private(e.path) && self.mentions(info, e.path) {
                self.unsupported_at(span, "export forwarder whose type refers to a private path");
            }
            let result = self.u.result_type(info);
            if self.is_context_function_type(result) {
                self.unsupported_at(span, "export forwarder of a context function result");
            }
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

    /// Whether type `t` has part `part`.
    fn mentions(&mut self, t: TypeId, part: TypeId) -> bool {
        let mut found = false;
        self.u.map_type(t, &mut |_, x| {
            found |= x == part;
            None
        });
        found
    }

    /// The type alias forwarding to type member `m` of `e`'s path.
    fn add_type_forwarder(&mut self, e: &mut Export, alias: &str, m: SymbolId, span: Span) {
        let info = self.u.info(m);
        let class = self.u.symbol(m).kind == Kind::Class;
        let class_params =
            class && self.u.symbol(m).decls.iter().any(|&d| self.u.symbol(d).kind == Kind::TypeParam);
        if class_params || matches!(self.u.types().get(info), Type::Lambda { .. }) {
            self.unsupported_at(span, "export of a type constructor");
            return;
        }
        let name = self.u.symbol(m).name.clone();
        if class && !self.type_members(e.path, &name).iter().any(|&c| !self.u.is_type_symbol(c)) {
            self.unsupported_at(span, "export of a class without a companion object");
            return;
        }
        let taken =
            self.u.decl(e.cls, alias).unwrap_or_default().into_iter().any(|d| self.u.is_type_symbol(d));
        if taken {
            self.unsupported_at(span, "export of a type whose name the class defines");
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

    /// Fails on two forwarders of the same name that erase alike (dotty's `avoidClashes`).
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
