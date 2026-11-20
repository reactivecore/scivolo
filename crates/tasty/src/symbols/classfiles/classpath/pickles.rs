//! The Scala 2 classes of the jars' pickles (ADR 0128): entered on lookup, refused whole or by
//! member, and refused with a refused one they refer to.

use super::super::super::scala2::{self, Footprint};
use super::super::super::{Kind, SymbolId, Universe};
use super::{dotted, java_class};
use crate::types::NameId;
use rustc_hash::FxHashSet;

/// The simple name of a package object's class file.
const PACKAGE_OBJECT: &str = "package";

/// A Scala 2 pickle entered while another was: its internal name, the roots it entered and its
/// footprint.
pub struct Completed {
    internal: String,
    roots: Vec<SymbolId>,
    footprint: Footprint,
}

impl Universe {
    /// Enters the package object of package `pkg` from its Scala 2 pickle if a jar holds one; a
    /// refused one counts as a refused lookup.
    pub fn look_up_scala2_package_object(&mut self, pkg: SymbolId) {
        let outer = self.full_name(pkg).replace('.', "/");
        if outer.is_empty() {
            return;
        }
        let internal = format!("{outer}/{PACKAGE_OBJECT}");
        java_class(self, &internal);
        scala2_class(self, pkg, PACKAGE_OBJECT, &internal);
    }

    /// Whether class `c` was entered from a Scala 2 pickle of a jar.
    pub fn is_pickled(&self, c: SymbolId) -> bool {
        self.java_classes.as_ref().is_some_and(|classes| classes.pickled.contains(&c))
    }

    /// Records that a lookup of `name` in class `owner` of a Scala 2 pickle is refused for `why`.
    pub fn refuse_member(&mut self, owner: SymbolId, name: NameId, why: String) {
        if let Some(classes) = self.java_classes.as_mut() {
            classes.refused_members.entry((owner, name)).or_insert(why);
        }
    }

    /// Counts a lookup of `name` in class `owner` if its Scala 2 pickle's member of that name is
    /// refused.
    pub fn look_up_refused_member(&mut self, owner: SymbolId, name: NameId) {
        let Some(why) = self.java_classes.as_ref().and_then(|c| c.refused_members.get(&(owner, name))) else {
            return;
        };
        let refusal =
            format!("Scala 2 member `{}.{}` of a jar: {why}", self.full_name(owner), self.name(name));
        let classes = self.java_classes.as_mut().expect("a refused member has its classes");
        classes.refused_lookups.count += 1;
        classes.refused_lookups.last = refusal;
    }

    /// Enters the class of every Scala 2 pickle of the jars and lists the refusals, one line per
    /// class (`class: why`) and per member (`member: why`) refused, and the counts of pickles
    /// (`pickles: N`) and classes read (`read: N`); for measurements only.
    pub fn scala2_report(&mut self) -> Vec<String> {
        let Some(classes) = self.java_classes.as_ref() else { return Vec::new() };
        let mut names: Vec<String> = classes.jars.keys().cloned().collect();
        names.sort();
        for internal in &names {
            java_class(self, internal);
        }
        let Some(classes) = self.java_classes.as_ref() else { return Vec::new() };
        let mut pickles: Vec<String> = classes.pickles.keys().cloned().collect();
        pickles.sort();
        for internal in &pickles {
            let (pkg, simple) = internal.rsplit_once('/').unwrap_or(("", internal));
            let pkg = self.package(&pkg.replace('/', ".")).unwrap_or(self.root());
            scala2_class(self, pkg, simple, internal);
        }
        let Some(classes) = self.java_classes.as_ref() else { return Vec::new() };
        let mut lines =
            vec![format!("pickles: {}", pickles.len()), format!("read: {}", classes.pickled.len())];
        let refused = pickles.iter().filter_map(|p| classes.refused.get(p));
        lines.extend(refused.map(|why| format!("class: {why}")));
        lines.extend(classes.refused_members.values().map(|why| format!("member: {why}")));
        lines
    }
}

/// Enters class `class` of package `pkg` of internal name `internal` from its Scala 2 pickle if a
/// jar holds one not entered yet: the symbols entered, none if it is refused, which is recorded
/// and counted as a refused lookup (`java_class` counts the later ones); `None` without a pickle.
/// The pickles entered while this one was and that refer to it are refused if it is, those
/// that refer to a member it refuses are refused too.
pub fn scala2_class(u: &mut Universe, pkg: SymbolId, class: &str, internal: &str) -> Option<Vec<SymbolId>> {
    let classes = u.java_classes.as_mut()?;
    if classes.refused.contains_key(internal) {
        return Some(Vec::new());
    }
    let pickle = classes.pickles.remove(internal)?;
    let before = std::mem::take(&mut classes.refused_lookups);
    let mark = classes.completed.len();
    classes.depth += 1;
    let mut footprint = Footprint::default();
    let entered = scala2::enter(u, pkg, class, &pickle, &mut footprint);
    let classes = u.java_classes.as_mut()?;
    classes.refused_lookups = before;
    classes.depth -= 1;
    let nested = classes.completed.split_off(mark);
    let (poisoned, why) = match &entered {
        Ok(_) => (&footprint.refused, format!("refers to a refused member of `{}`", dotted(internal))),
        Err(_) => (&footprint.own, format!("refers to the refused `{}`", dotted(internal))),
    };
    let kept = refuse_dependents(u, nested, poisoned.iter().copied().collect(), &why);
    let classes = u.java_classes.as_mut()?;
    if classes.depth > 0 {
        classes.completed.extend(kept);
    }
    match entered {
        Ok(entered) => {
            let pickled: Vec<SymbolId> =
                entered.iter().copied().filter(|&s| u.symbol(s).kind == Kind::Class).collect();
            let classes = u.java_classes.as_mut()?;
            classes.pickled.extend(pickled);
            let roots = entered.clone();
            match classes.depth {
                0 => classes.completed.clear(),
                _ => classes.completed.push(Completed { internal: internal.to_owned(), roots, footprint }),
            }
            Some(entered)
        }
        Err(why) => {
            refuse(u, internal, format!("Scala 2 class file `{}` of a jar: {why}", dotted(internal)));
            Some(Vec::new())
        }
    }
}

/// Unenters and refuses for `why` each pickle of `nested`, entered while another was, that refers
/// to a symbol of `poisoned` or of a pickle refused so: the pickles kept.
fn refuse_dependents(
    u: &mut Universe,
    nested: Vec<Completed>,
    mut poisoned: FxHashSet<SymbolId>,
    why: &str,
) -> Vec<Completed> {
    let mut kept = nested;
    loop {
        let (refused, rest): (Vec<Completed>, Vec<Completed>) =
            kept.into_iter().partition(|c| c.footprint.used.iter().any(|s| poisoned.contains(s)));
        kept = rest;
        if refused.is_empty() {
            return kept;
        }
        for c in refused {
            poisoned.extend(c.footprint.own.iter().copied());
            for &s in c.roots.iter().rev() {
                u.unenter(s);
            }
            let Some(classes) = u.java_classes.as_mut() else { return kept };
            c.roots.iter().for_each(|s| _ = classes.pickled.remove(s));
            let refusal = format!("Scala 2 class file `{}` of a jar: {why}", dotted(&c.internal));
            classes.refused.insert(c.internal, refusal);
        }
    }
}

/// Records that class `internal` is refused for `refusal` and counts a refused lookup.
fn refuse(u: &mut Universe, internal: &str, refusal: String) {
    let Some(classes) = u.java_classes.as_mut() else { return };
    classes.refused_lookups.count += 1;
    classes.refused_lookups.last.clone_from(&refusal);
    classes.refused.insert(internal.to_owned(), refusal);
}
