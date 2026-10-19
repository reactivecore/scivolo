// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/reporting/DidYouMean.scala
//! The names dotty's "did you mean" hints may suggest, of either kind, and `closestTo`'s
//! distance (dotty's `DidYouMean`).

use super::Typer;
use super::imports::ImportEntry;
use super::java_classes::java_classes;
use crate::ctx::{Ctx, Level};
use syntax::ast::children::Child;
use syntax::ast::{Ast, NodeId, Tree};
use syntax::names::Name;
use tasty::{Kind, SymbolId, Type, TypeId};

/// The root packages of the JDK's modules.
const JDK_ROOTS: [&str; 8] = ["apple", "com", "java", "javax", "jdk", "netscape", "org", "sun"];

/// The members of `Any`, `AnyRef` and the JDK's `Object`.
pub const OBJECT_MEMBERS: &str = "== != ## equals hashCode toString getClass isInstanceOf asInstanceOf eq ne \
    synchronized notify notifyAll wait clone finalize";

/// The Java classes whose members are known: the universe's or `OBJECT_MEMBERS`.
pub const KNOWN_JAVA: [&str; 3] = ["java.lang.Object", "java.lang.Comparable", "java.io.Serializable"];

impl Typer<'_> {
    /// The names of all bindings dotty's hint may suggest in `ctx`, and more; `None` where they
    /// are not all known, as in a named package or with members of the JDK.
    pub fn candidates(&mut self, ctx: Ctx) -> Option<Vec<String>> {
        let mut script: Vec<SymbolId> = self.syms.flags.keys().copied().collect();
        script.extend(self.syms.by_tree.values().copied());
        script.extend(self.syms.closure_syms.iter().copied());
        let mut names: Vec<String> = script.iter().map(|&s| self.u.symbol(s).name.clone()).collect();
        names.extend(OBJECT_MEMBERS.split_whitespace().map(str::to_owned));
        let mut sites = vec![self.u.root()];
        let mut level = Some(ctx.level);
        while let Some(l) = level {
            match self.levels.get(l) {
                Level::Scope(entries) => names.extend(entries.iter().map(|(n, _)| n.clone())),
                &Level::Members(p) if p != self.u.root() && self.u.symbol(p).kind == Kind::Package => {
                    return None;
                }
                &Level::Members(owner) => sites.push(owner),
                &Level::Import(i) => {
                    let ImportEntry::Done(info) = &self.imports[i] else { return None };
                    names.extend(info.selectors.iter().map(|s| s.rename.clone()));
                    let qual = info.qual;
                    sites.push(self.import_site_class(qual)?);
                }
            }
            level = self.levels.outer(l);
        }
        for site in sites {
            self.member_names(site, &mut names)?;
        }
        self.defined_names(&mut names);
        Some(names)
    }

    /// Adds the names the unit's trees define, entered or not, to `names`.
    fn defined_names(&self, names: &mut Vec<String>) {
        let mut found = Vec::new();
        for (&t, &s) in &self.syms.by_tree {
            if self.u.symbol(s).owner.is_none_or(|o| self.u.symbol(o).kind == Kind::Package) {
                definitions(self.ast, t, false, &mut found);
            }
        }
        names.extend(found.into_iter().map(|n| self.text(n).to_owned()));
    }

    /// The package or class whose members an import of a path of type `qual` makes visible.
    pub fn import_site_class(&mut self, qual: TypeId) -> Option<SymbolId> {
        match self.u.types().get(qual).clone() {
            Type::Package(p) => Some(p),
            Type::Sym(_, s) if self.u.symbol(s).kind == Kind::Package => Some(s),
            Type::Sym(_, s) if self.u.symbol(s).flags.has(tasty::tags::OBJECT) => self.u.module_class(s),
            _ => {
                let widened = self.u.widen(qual);
                self.u.class_symbol(widened)
            }
        }
    }

    /// Adds the names of the members of package or class `site` to `names`; `None` for members
    /// of the JDK the universe may not know, or of a self type or export clause.
    pub fn member_names(&mut self, site: SymbolId, names: &mut Vec<String>) -> Option<()> {
        let full = self.u.full_name(site);
        if self.u.symbol(site).kind == Kind::Package {
            if site == self.u.root() {
                names.extend(JDK_ROOTS.iter().map(|&n| n.to_owned()));
            } else if is_jdk(&full) && full != "java.lang" {
                return None;
            }
            names.extend(java_classes(&full).map(str::to_owned));
            let decls = self.u.decls(site).ok()?;
            for d in decls {
                names.push(self.u.symbol(d).name.clone());
                if self.is_named_package_object(d) {
                    self.decl_names(d, names)?;
                }
            }
            return Some(());
        }
        self.known_bases(site)?.into_iter().try_for_each(|base| self.decl_names(base, names))
    }

    /// The linearization of class `site`, completed; `None` if the universe may not know its members.
    pub fn known_bases(&mut self, site: SymbolId) -> Option<Vec<SymbolId>> {
        self.complete(site);
        let bases = self.u.linearization(site);
        for &base in &bases {
            self.complete(base);
            let full = self.u.full_name(base);
            let unknown_parent = self.u.parents(base).into_iter().any(|p| self.u.class_symbol(p).is_none());
            let jdk = is_jdk(&full) && !KNOWN_JAVA.contains(&&*full);
            if unknown_parent || jdk || self.has_undeclared_members(base) {
                return None;
            }
        }
        Some(bases)
    }

    fn decl_names(&mut self, s: SymbolId, names: &mut Vec<String>) -> Option<()> {
        let decls = self.u.decls(s).ok()?;
        names.extend(decls.iter().map(|&d| self.u.symbol(d).name.clone()));
        Some(())
    }
}

/// Pushes the names that tree `id` and the trees under it define onto `out`, and those of all
/// identifiers in patterns; `pat` if `id` is in a pattern.
fn definitions(ast: &Ast, id: NodeId, pat: bool, out: &mut Vec<Name>) {
    let pats: &[NodeId] = match ast.tree(id) {
        Tree::ValDef { name, .. }
        | Tree::DefDef { name, .. }
        | Tree::TypeDef { name, .. }
        | Tree::ModuleDef { name, .. }
        | Tree::Bind { name, .. } => {
            out.push(name.name);
            &[]
        }
        Tree::Ident { name } if pat => {
            out.push(name.name);
            &[]
        }
        Tree::PatDef { pats, .. } => ast.items(*pats),
        Tree::CaseDef { pat, .. } | Tree::GenFrom { pat, .. } | Tree::GenAlias { pat, .. } => {
            std::slice::from_ref(pat)
        }
        _ => &[],
    };
    let mut visit = |n: NodeId| definitions(ast, n, pat || pats.contains(&n), out);
    for &c in ast.children(id).iter() {
        match c {
            Child::Node(n) => visit(n),
            Child::List(l) => ast.items(l).iter().for_each(|&n| visit(n)),
            Child::Clauses(cs) => {
                ast.clause_items(cs).iter().flat_map(|&l| ast.items(l)).for_each(|&n| visit(n))
            }
            Child::Empty | Child::Mods(_) => {}
        }
    }
}

/// Whether full name `full` is in a root package of the JDK.
pub fn is_jdk(full: &str) -> bool {
    JDK_ROOTS.contains(&full.split('.').next().unwrap_or_default())
}

/// Whether dotty's `closestTo` keeps candidate `cand` for name `missing`, in UTF-16 units: an edit
/// distance of at most 3, at most a quarter of the summed lengths, and below each length.
pub fn close(missing: &[u16], cand: &str) -> bool {
    let cand: Vec<u16> = cand.encode_utf16().collect();
    if cand.len().abs_diff(missing.len()) > 3 {
        return false;
    }
    let d = distance(missing, &cand);
    d <= 3 && d * 4 <= missing.len() + cand.len() && d < missing.len() && d < cand.len()
}

/// The Levenshtein distance of `a` and `b` (dotty's `DidYouMean.distance`).
pub fn distance(a: &[u16], b: &[u16]) -> usize {
    let mut row: Vec<usize> = (0..=a.len()).collect();
    for (j, &cb) in b.iter().enumerate() {
        let mut diag = row[0];
        row[0] = j + 1;
        for (i, &ca) in a.iter().enumerate() {
            let next = if ca == cb { diag } else { diag.min(row[i]).min(row[i + 1]) + 1 };
            diag = row[i + 1];
            row[i + 1] = next;
        }
    }
    row[a.len()]
}

#[cfg(test)]
mod tests {
    use super::{close, distance};

    fn units(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    #[test]
    fn measures_edit_distance() {
        assert_eq!(distance(&units("kitten"), &units("sitting")), 3);
        assert_eq!(distance(&units(""), &units("abc")), 3);
        assert_eq!(distance(&units("flaw"), &units("lawn")), 2);
    }

    #[test]
    fn keeps_candidates_as_closest_to_does() {
        assert!(close(&units("printn"), "println"));
        assert!(!close(&units("y"), "x"));
        assert!(close(&units("y"), "y"));
        assert!(!close(&units("ab"), "cd"));
        assert!(close(&units("abc"), "abd"));
        assert!(!close(&units("abcd"), "wxyz"));
    }
}
