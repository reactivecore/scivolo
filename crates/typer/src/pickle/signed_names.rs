// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//   compiler/src/dotty/tools/dotc/core/Signature.scala
//! Methods' signed names: their parameter and result types as qualified class names.

use super::names::package_name;
use super::{Refusal, TreePickler};
use crate::signatures::{params_sig, result_sig};
use crate::tpd::print::symbols::full_name;
use tasty::tags::OBJECT;
use tasty::write::{Name, ParamSig};
use tasty::{Kind, SymbolId, Universe};

/// The name of a class in a signature, `full` its full name: qualified, a module class's as an
/// `OBJECTCLASS` name, each array dimension a `[]` on the last part.
fn sig_type_name(u: &mut Universe, full: &str) -> Result<Name, Refusal> {
    let base = full.trim_end_matches("[]");
    let arrays = &full[base.len()..];
    if let Some((pkg, name)) = base.rsplit_once('.')
        && arrays.is_empty()
        && let Some(p) = u.package(pkg)
        && let Some(c) = member_class(u, p, name)
    {
        return Ok(class_sig_name(u, c));
    }
    let (module, base) = match base.strip_suffix('$') {
        Some(b) => match is_module_class(u, base) {
            Ok(module) => (module, if module { b } else { base }),
            Err(why) => match nested_class(u, base).filter(|_| arrays.is_empty()) {
                Some(c) => return Ok(class_sig_name(u, c)),
                None => return Err(why),
            },
        },
        None => (false, base),
    };
    let path = format!("{base}{arrays}");
    let parts: Vec<&str> = path.split('.').collect();
    let mut name: Option<Name> = None;
    for (i, part) in parts.iter().enumerate() {
        let object = i + 1 < parts.len() && part.len() > 1 && part.ends_with('$');
        let part = Name::utf8(if object { &part[..part.len() - 1] } else { part });
        let qualified = match name {
            Some(q) => Name::Qualified(Box::new(q), Box::new(part)),
            None => part,
        };
        name = Some(if object { Name::ObjectClass(Box::new(qualified)) } else { qualified });
    }
    let name = name.expect("a name");
    Ok(if module { Name::ObjectClass(Box::new(name)) } else { name })
}

/// The class a member of a class declared in `owner` that binary name `name` names, e.g.
/// `Tuple$package$EmptyTuple$`.
fn member_class(u: &mut Universe, owner: SymbolId, name: &str) -> Option<SymbolId> {
    let class = |u: &mut Universe, o, n: &str| {
        let decls = u.decl(o, n).ok()?;
        decls.into_iter().find(|&s| u.symbol(s).kind == Kind::Class)
    };
    name.match_indices('$').map(|(k, _)| k + 1).filter(|&k| k > 1 && k < name.len()).find_map(|k| {
        let outer = class(u, owner, &name[..k])?;
        class(u, outer, &name[k..]).or_else(|| member_class(u, outer, &name[k..]))
    })
}

/// Whether the class of full name `full`, which ends in `$`, is a module class.
fn is_module_class(u: &mut Universe, full: &str) -> Result<bool, Refusal> {
    let (pkg, name) = full.rsplit_once('.').unwrap_or(("", full));
    let p = u.package(pkg).ok_or("a signature naming a class outside a package")?;
    let decls = u.decl(p, name).map_err(|_| "a signature naming an unknown class")?;
    let class = decls.into_iter().find(|&s| u.symbol(s).kind == Kind::Class);
    let class = class.ok_or("a signature naming an unknown class")?;
    Ok(u.symbol(class).flags.has(OBJECT))
}

/// The class of full name `full` nested in a class of a package, if it is one.
fn nested_class(u: &mut Universe, full: &str) -> Option<SymbolId> {
    let dots: Vec<usize> = full.match_indices('.').map(|(i, _)| i + 1).collect();
    let mut starts = dots.into_iter().rev().chain([0]);
    let (pkg, rest) = starts.find_map(|i| Some((u.package(full[..i].trim_end_matches('.'))?, &full[i..])))?;
    let ends = (1..=rest.len()).filter(|&i| i == rest.len() || matches!(rest.as_bytes()[i], b'.' | b'$'));
    let ends: Vec<usize> = ends.chain((1..rest.len()).filter(|&i| rest.as_bytes()[i - 1] == b'$')).collect();
    let mut todo: Vec<SymbolId> = Vec::new();
    for i in ends {
        let decls = u.decls_named(pkg, &rest[..i]);
        todo.extend(decls.into_iter().filter(|&c| u.is_class(c)));
    }
    while let Some(c) = todo.pop() {
        let members = u.decls(c).unwrap_or_default();
        for m in members.into_iter().filter(|&m| u.is_class(m)) {
            let names = [full_name(u, m), u.binary_name(m)];
            if names.iter().any(|n| n == full) {
                return Some(m);
            }
            if names.iter().any(|n| full.starts_with(n.as_str())) {
                todo.push(m);
            }
        }
    }
    None
}

/// The name of class `c` in a signature (dotty's `fullNameSeparated(QualifiedName)`): a `_$` for
/// each term between it and its enclosing class, a module class's as an `OBJECTCLASS` name.
fn class_sig_name(u: &Universe, c: SymbolId) -> Name {
    let sym = u.symbol(c);
    let module = sym.kind == Kind::Class && sym.flags.has(OBJECT);
    let mut name = match sym.name.strip_suffix('$') {
        Some(n) if module => n.to_owned(),
        _ => sym.name.clone(),
    };
    let mut encl = sym.owner;
    while let Some(e) = encl.filter(|&e| !matches!(u.symbol(e).kind, Kind::Class | Kind::Package)) {
        name.insert_str(0, "_$");
        encl = u.symbol(e).owner;
    }
    let last = Box::new(Name::utf8(&name));
    let name = match encl.filter(|&e| e != u.root()) {
        Some(e) if u.symbol(e).kind == Kind::Package => Name::Qualified(Box::new(package_name(u, e)), last),
        Some(e) => Name::Qualified(Box::new(class_sig_name(u, e)), last),
        None => *last,
    };
    if module { Name::ObjectClass(Box::new(name)) } else { name }
}

impl TreePickler<'_> {
    /// The name of `s`, signed if `signed` and `s` is a method (dotty's `pickleNameAndSig`).
    pub fn signed_name(&mut self, s: SymbolId, signed: bool) -> Result<(), Refusal> {
        let orig = self.ref_name(s);
        let result = if signed { result_sig(self.u, s) } else { None };
        let Some(result) = result else {
            self.name(&orig);
            return Ok(());
        };
        let params = params_sig(self.u, s)
            .iter()
            .map(|p| match p.parse() {
                Ok(n) => Ok(ParamSig::TypeParams(n)),
                Err(_) => sig_type_name(self.u, p).map(ParamSig::Term),
            })
            .collect::<Result<_, _>>()?;
        let target = match (self.accessor_targets.get(&s), &self.u.symbol(s).target) {
            (Some(t), _) => t.clone(),
            (None, Some(t)) => Name::utf8(t),
            (None, None) => orig.clone(),
        };
        let class = self.u.symbol(s).owner.filter(|_| self.u.is_constructor(s));
        let result = match class.filter(|&c| self.u.underlying(c).is_none()) {
            Some(class) => Box::new(class_sig_name(self.u, class)),
            None => Box::new(sig_type_name(self.u, &result)?),
        };
        self.name(&Name::Signed { orig: Box::new(orig), target: Box::new(target), result, params });
        Ok(())
    }
}
