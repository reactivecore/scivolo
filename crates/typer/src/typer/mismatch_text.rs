//! The found and required types of a type mismatch as dotty's message shows them, `(1 : Int)`,
//! `(x : Int)`, `List[Int]`, where `show_user` shows them so and no `where` clause would follow.

use super::Typer;
use super::not_a_member::is_concrete_part;
use rustc_hash::FxHashMap;
use syntax::ast::flags as f;
use tasty::tags::OBJECT;
use tasty::{Const, Kind, SymbolId, Type, TypeId, constant};

impl Typer<'_> {
    /// The parts of `t`, `t` included.
    pub fn type_parts(&mut self, t: TypeId) -> Vec<TypeId> {
        let mut parts = Vec::new();
        self.u.map_type(t, &mut |_, part| {
            parts.push(part);
            None
        });
        parts
    }

    /// Whether no two of the classes and type definitions `types` mention share a name, which
    /// dotty's message would tell apart with a `where` clause.
    pub fn distinct_names(&mut self, types: &[TypeId]) -> bool {
        let mut seen: FxHashMap<String, SymbolId> = FxHashMap::default();
        for &t in types {
            for part in self.type_parts(t) {
                let sym = match self.u.types().get(part).clone() {
                    Type::Sym(_, s) => Some(s),
                    Type::TypeRef(p, n) => self.u.resolve(p, &n, true),
                    _ => None,
                };
                let Some(s) = sym.filter(|&s| !self.u.symbol(s).kind.is_term()) else { continue };
                let name = self.u.symbol(s).name.clone();
                if *seen.entry(name).or_insert(s) != s {
                    return false;
                }
            }
        }
        true
    }

    /// `t` as dotty's message shows it, if `show_user` shows it so and it is not a singleton.
    pub fn plain_text(&mut self, t: TypeId) -> Option<String> {
        if matches!(self.u.types().get(t), Type::This(_) | Type::TermRef(..)) || self.term_symbol(t).is_some()
        {
            return None;
        }
        for part in self.type_parts(t) {
            let singleton = matches!(self.u.types().get(part), Type::Constant(..) | Type::Str(_));
            if singleton || !is_concrete_part(self.u, part) || !self.shown_as_dotty(part) {
                return None;
            }
        }
        Some(self.u.show_user(t)).filter(|text| !text.contains(['<', '?']))
    }

    /// The non-module term singleton `t` refers to.
    fn term_symbol(&mut self, t: TypeId) -> Option<SymbolId> {
        let s = match self.u.types().get(t).clone() {
            Type::Sym(_, s) => s,
            Type::TermRef(p, n) => self.u.resolve(p, &n, false)?,
            _ => return None,
        };
        let sym = self.u.symbol(s);
        (sym.kind.is_term() && !sym.flags.has(OBJECT)).then_some(s)
    }

    /// Found type `t` as dotty's message shows it: a constant or a reference to a value with its
    /// type, `(1 : Int)`, `(x : Int)`, else as `plain_text` does.
    pub fn found_text(&mut self, t: TypeId) -> Option<String> {
        let widened = self.u.widen(t);
        let literal = match self.u.types().get(t).clone() {
            Type::Str(s) => format!("\"{}\"", plain_chars(&s)?),
            Type::Constant(tag, bits) => match constant(tag, bits)? {
                Const::Bool(b) => b.to_string(),
                Const::Char(c) => format!("'{}'", plain_chars(&c.to_string())?),
                Const::Int(n) => n.to_string(),
                Const::Long(n) => format!("{n}L"),
                Const::Float(_) => format!("{}f", self.u.show_user(t)),
                Const::Double(_) => format!("{}d", self.u.show_user(t)),
                _ => return None,
            },
            Type::This(class) => self.this_path(class)?,
            _ => match self.term_symbol(t) {
                Some(s) if self.u.symbol(s).kind == Kind::Def => return self.plain_text(widened),
                Some(s) => self.singleton_path(t, s)?,
                None => return self.plain_text(t),
            },
        };
        Some(format!("({literal} : {})", self.plain_text(widened)?))
    }

    /// The path dotty shows for a reference `t` to value or parameter `s` whose type is not a
    /// singleton nor refined by a pattern: `x` if local or of a REPL line, `O.x` or `C.this.x` for
    /// a member of a top-level object or class.
    fn singleton_path(&mut self, t: TypeId, s: SymbolId) -> Option<String> {
        let info = self.u.info(s);
        let info_singleton =
            matches!(self.u.types().get(info), Type::Constant(..) | Type::Str(_) | Type::This(_))
                || self.term_symbol(info).is_some();
        let flags = self.syms.flags.get(&s).copied().unwrap_or_default();
        let pattern_bound = flags.is(f::CASE) && !flags.is(f::ENUM);
        if info_singleton || pattern_bound || !matches!(self.u.symbol(s).kind, Kind::Val | Kind::Param) {
            return None;
        }
        let owner = self.u.symbol(s).owner?;
        let name = self.u.symbol(s).name.clone();
        let o = self.u.symbol(owner);
        let top = o.kind == Kind::Class && o.owner == Some(self.u.root());
        let path = match o.kind {
            k if k.is_term() => name,
            _ if top && o.name.starts_with("rs$line$") => name,
            _ if top && !o.name.contains('$') => format!("{}.this.{name}", o.name),
            _ if top && o.flags.has(OBJECT) => format!("{}.{name}", tasty::module_name(&o.name)?),
            _ => return None,
        };
        (self.u.show_user(t) == format!("{path}.type")).then_some(path)
    }

    /// `C.this` for the `this` of class reference `class`, a named class that is not an object and
    /// has no type parameters.
    fn this_path(&mut self, class: TypeId) -> Option<String> {
        let c = self.direct_class(class)?;
        let name = self.u.symbol(c).name.clone();
        if self.u.symbol(c).flags.has(OBJECT) || !self.u.type_params(c).is_empty() || name.contains('$') {
            return None;
        }
        Some(format!("{name}.this"))
    }
}

/// `s` if dotty's printer shows its characters unescaped.
fn plain_chars(s: &str) -> Option<&str> {
    s.chars().all(|c| !c.is_control() && !matches!(c, '"' | '\'' | '\\')).then_some(s)
}
