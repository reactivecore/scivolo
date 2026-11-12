// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! What a simple name refers to (dotty's `findRef`): the context chain searched outward, a
//! found binding checked against what outer levels bind with lower or equal precedence.

use super::Typer;
use super::idents::Found;
use crate::ctx::{Ctx, Level, LevelId, Mode};
use crate::defn::ref_type;
use tasty::{Kind, Type};

/// The precedence of a binding (dotty's `BindingPrec`), lowest first.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Prec {
    PackageClause,
    WildImport,
    NamedImport,
    Definition,
}

impl Prec {
    /// Definitions beat imports, and named imports beat wildcard imports, in the same scope.
    fn beats(self, prev: Prec) -> bool {
        self == Prec::Definition || self == Prec::NamedImport && prev == Prec::WildImport
    }
}

/// A binding found so far: what, with which precedence, in which scope.
struct Hit {
    found: Found,
    prec: Prec,
    scope: Option<LevelId>,
}

/// The name looked up, as a term or a type.
#[derive(Clone, Copy)]
pub struct Name<'a> {
    pub text: &'a str,
    pub is_type: bool,
}

impl Typer<'_> {
    /// Looks up simple name `name`, a term or a type, from `ctx` outward and returns the symbols
    /// it refers to (dotty's `findRef`):
    /// - a definition in a local scope, class or the same compilation unit wins at once;
    ///   definitions elsewhere in an enclosing package, named and wildcard imports are kept
    ///   until an inner binding of higher precedence or the end of the search;
    /// - the top-level packages come before the root imports, so `io` is a top-level `io`
    ///   package if there is one, not `scala.io`;
    /// - an explicit import from `Predef` that does not supply the name hides the root import
    ///   of `Predef`.
    ///
    /// `Err` names the refused case: an ambiguous reference, a member of an intersection type,
    /// or a constructor proxy (`C(...)` for a class without `apply`) and the proxy of an exported
    /// class.
    pub fn find_ref(&mut self, name: &str, is_type: bool, ctx: Ctx) -> Result<Option<Found>, &'static str> {
        let name = Name { text: name, is_type };
        let mut prev: Option<Hit> = None;
        let mut unimported = Vec::new();
        let mut root_package = None;
        let mut level = Some(ctx.level);
        while let Some(l) = level
            && root_package != Some(l)
        {
            let scope = self.levels.def_scope(l);
            let possible = |prev: &Option<Hit>, prec: Prec| match prev {
                None => true,
                Some(h) => h.prec < prec || h.prec == prec && h.scope == scope,
            };
            match *self.levels.get(l) {
                Level::Scope(_) | Level::Members(_) => {
                    let Some(found) = self.defined(l, name, ctx).and_then(|f| self.qualifying(f, l, ctx))
                    else {
                        if let Level::Members(owner) = *self.levels.get(l)
                            && !name.is_type
                        {
                            let prefix = crate::defn::owner_prefix(self.u, owner);
                            if self.needs_joint_member(prefix, name.text) {
                                return Err("intersection member of an intersected type");
                            }
                        }
                        let classes = self.defined(l, Name { is_type: true, ..name }, ctx).map(|f| f.syms);
                        let classes = classes.unwrap_or_default();
                        let class = classes.iter().any(|&s| self.u.symbol(s).kind == Kind::Class);
                        let proxy = classes
                            .iter()
                            .any(|&s| self.needs_constructor_proxies(s) || self.is_exported_class(s));
                        if !name.is_type
                            && class
                            && !proxy
                            && prev.is_none()
                            && !ctx.mode.is(Mode::EXTENSION_METHOD)
                        {
                            let outer = self.levels.outer(l);
                            let found =
                                outer.map(|o| self.find_ref(name.text, false, Ctx { level: o, ..ctx }));
                            if !matches!(found, Some(Ok(Some(_)))) {
                                return Err("constructor proxy");
                            }
                        }
                        if !name.is_type && proxy && !ctx.mode.is(Mode::EXTENSION_METHOD) {
                            if prev.is_some() {
                                return Err("constructor proxy and an imported term");
                            }
                            let Some(outer) = self.levels.outer(l) else { return Err("constructor proxy") };
                            return match self.find_ref(name.text, false, Ctx { level: outer, ..ctx }) {
                                Ok(Some(f)) if self.shadows_proxy(&f, ctx) => {
                                    Err("constructor proxy shadowing a term")
                                }
                                Ok(_)
                                | Err("constructor proxy" | "constructor proxy of an exported class") => {
                                    Err("constructor proxy")
                                }
                                Err(what) => Err(what),
                            };
                        }
                        level = self.levels.outer(l);
                        continue;
                    };
                    let package = matches!(*self.levels.get(l), Level::Members(o) if self.u.symbol(o).kind == Kind::Package);
                    if !package || self.in_unit(&found) {
                        if self.is_inherited(l, &found) {
                            self.check_no_outer_defs(l, &found, name, ctx)?;
                        }
                        let hit = Hit { found, prec: Prec::Definition, scope };
                        return self.new_or_shadowed(prev, hit, l, name).map(Some);
                    }
                    if found.syms.iter().any(|&s| self.u.symbol(s).kind == Kind::Package) {
                        return Ok(Some(prev.map_or(found, |h| h.found)));
                    }
                    if prev.is_none() {
                        prev = Some(Hit { found, prec: Prec::PackageClause, scope });
                    }
                }
                Level::Import(i) => {
                    if let Some(s) = scope.filter(|&s| self.defines_before_imports(s, name, ctx)) {
                        level = Some(s);
                        continue;
                    }
                    if root_package.is_none()
                        && self.is_root_import(i)
                        && let Some(root) = scope
                    {
                        root_package = Some(root);
                        if let Some(found) =
                            self.defined(root, name, ctx).and_then(|f| self.qualifying(f, root, ctx))
                        {
                            return Ok(Some(prev.map_or(found, |h| h.found)));
                        }
                    }
                    if prev.is_some() && self.is_root_import(i) {
                        return Ok(prev.map(|h| h.found));
                    }
                    if self.is_root_import(i) && self.root_site(i).is_some_and(|s| unimported.contains(&s)) {
                        level = self.levels.outer(l);
                        continue;
                    }
                    let hit = if possible(&prev, Prec::NamedImport)
                        && let Some(f) = self.named_import_ref(i, name.text, name.is_type)
                        && let Some(found) = self.extensions_only(f, ctx)
                    {
                        Some(Hit { found, prec: Prec::NamedImport, scope })
                    } else if possible(&prev, Prec::WildImport) {
                        let found = self.wild_import_ref(i, name.text, name.is_type).and_then(|mut f| {
                            f.syms.retain(|&s| self.is_accessible_from(s, f.prefix, ctx));
                            self.extensions_only(f, ctx)
                        });
                        found.map(|found| Hit { found, prec: Prec::WildImport, scope })
                    } else {
                        None
                    };
                    if let Some(hit) = hit {
                        let (prec, scope) = (hit.prec, hit.scope);
                        let found = self.new_or_shadowed(prev, hit, l, name)?;
                        prev = Some(Hit { found, prec, scope });
                    } else {
                        let types = (!name.is_type && !ctx.mode.is(Mode::EXTENSION_METHOD))
                            .then(|| {
                                self.named_import_ref(i, name.text, true)
                                    .or_else(|| self.wild_import_ref(i, name.text, true))
                            })
                            .flatten();
                        if types.is_some_and(|f| f.syms.iter().any(|&s| self.is_exported_class(s))) {
                            return Err("constructor proxy of an exported class");
                        }
                        unimported.extend(self.unimported(i));
                    }
                }
            }
            level = self.levels.outer(l);
        }
        Ok(prev.map(|h| h.found))
    }

    /// The new binding `hit` at level `l` if no inner binding `prev` conflicts with it, `prev`
    /// if it shadows it (dotty's `checkNewOrShadowed`).
    fn new_or_shadowed(
        &mut self,
        prev: Option<Hit>,
        hit: Hit,
        l: LevelId,
        name: Name,
    ) -> Result<crate::typer::idents::Found, &'static str> {
        let Some(prev) = prev else { return Ok(hit.found) };
        if self.same_ref(&prev.found, &hit.found) || prev.scope == hit.scope && hit.prec.beats(prev.prec) {
            return Ok(hit.found);
        }
        if hit.prec == Prec::WildImport && self.outer_named_import(l, name) {
            return Ok(hit.found);
        }
        Err("ambiguous reference")
    }

    /// Whether an import at `l` or outward imports `name` by name.
    fn outer_named_import(&mut self, l: LevelId, name: Name) -> bool {
        let mut level = Some(l);
        while let Some(i) = level {
            if let Level::Import(imp) = *self.levels.get(i)
                && self.named_import_ref(imp, name.text, name.is_type).is_some()
            {
                return true;
            }
            level = self.levels.outer(i);
        }
        false
    }

    fn same_ref(&mut self, a: &Found, b: &Found) -> bool {
        if let ([s], [t]) = (&a.syms[..], &b.syms[..])
            && s != t
        {
            let (p, q) = (ref_type(self.u, a.prefix, *s), ref_type(self.u, b.prefix, *t));
            return self.u.is_subtype(p, q) && self.u.is_subtype(q, p);
        }
        if a.syms != b.syms {
            return false;
        }
        let methods = a.syms.iter().all(|&s| self.u.symbol(s).kind == tasty::Kind::Def);
        match (a.prefix, b.prefix) {
            (Some(p), Some(q)) if methods && p != q => {
                a.syms.iter().all(|&s| self.u.member_info(p, s) == self.u.member_info(q, s))
            }
            (Some(p), Some(q)) => p == q || self.u.is_subtype(p, q) && self.u.is_subtype(q, p),
            (p, q) => p == q,
        }
    }

    /// Whether scope or class members level `s` defines `name`; its definitions are seen before
    /// the imports that share its scope (dotty's `isNewDefScope`), unlike a package's.
    fn defines_before_imports(&mut self, s: LevelId, name: Name, ctx: Ctx) -> bool {
        let defines = match *self.levels.get(s) {
            Level::Scope(_) => true,
            Level::Members(o) => self.u.symbol(o).kind == Kind::Class && !self.is_named_package_object(o),
            Level::Import(_) => false,
        };
        defines && self.defined(s, name, ctx).and_then(|f| self.qualifying(f, s, ctx)).is_some()
    }

    /// Whether `found` has a symbol the source file being typed defines (dotty's
    /// `isDefinedInCurrentUnit`), or one of the unit without a definition tree.
    fn in_unit(&self, found: &Found) -> bool {
        found.syms.iter().filter(|s| self.syms.flags.contains_key(s)).any(|s| {
            match self.syms.def_trees.get(s) {
                Some(&tree) => self.file_of(self.ast.span(tree)) == self.file,
                None => true,
            }
        })
    }

    /// The symbols named `name` that scope or members level `l` defines, as selected in `ctx`.
    pub fn defined(&mut self, l: LevelId, name: Name, ctx: Ctx) -> Option<Found> {
        match self.levels.get(l) {
            Level::Scope(entries) => {
                let found = entries
                    .iter()
                    .rev()
                    .find(|(n, s)| n == name.text && self.u.is_type_symbol(*s) == name.is_type);
                let sym = found?.1;
                let owner = self.u.symbol(sym).owner.filter(|&o| self.u.symbol(o).kind == Kind::Class);
                let prefix = owner.map(|o| crate::defn::owner_prefix(self.u, o));
                Some(Found { prefix, syms: vec![sym] })
            }
            &Level::Members(owner) => {
                if let Some(s) = self.self_named(owner, name.text).filter(|_| !name.is_type) {
                    return Some(Found { prefix: None, syms: vec![s] });
                }
                let prefix = crate::defn::owner_prefix(self.u, owner);
                let mut found = self.members_found_in(prefix, name.text, name.is_type, ctx)?;
                if self.u.symbol(owner).kind == Kind::Package {
                    found.prefix = Some(self.u.intern_type(Type::This(prefix)));
                }
                Some(found)
            }
            Level::Import(_) => None,
        }
    }
}
