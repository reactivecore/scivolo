// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The symbols of `quotes.reflect`: owners, names, flags and kinds. Flags are the bits of the
//! TASTy modifier tags, with bits 2 to 5 for `Method`, `Package`, `Param` and `AbsOverride`,
//! bit 50 for `JavaDefined` and bit 51 for `Scala2x`.

use crate::host::Host;
use crate::reflect::symbol;
use tasty::tags::{ABSTRACT, FINAL, LAZY, MACRO, OBJECT, OVERRIDE, PROTECTED, PROTECTEDqualified, SYNTHETIC};
use tasty::{Flags, Kind, Symbol, SymbolId, Universe};
use vm::Reflected;

type Answer = Result<Reflected, &'static str>;

const METHOD: u32 = 2;
const PACKAGE: u32 = 3;
const PARAM: u32 = 4;
const ABS_OVERRIDE: u32 = 5;
const JAVA_DEFINED: u32 = 50;
const SCALA2X: u32 = 51;

/// The symbols an expansion makes: the owner of its splice and the empty package.
#[derive(Default)]
pub struct Made {
    macro_owner: Option<SymbolId>,
    empty_package: Option<SymbolId>,
}

impl Host {
    /// The value `macro` that owns the splice (dotty's `Splicer.splice`).
    fn macro_owner(&mut self) -> SymbolId {
        let flags = Flags::default().with(MACRO).with(SYNTHETIC);
        let owner = self.owner;
        *self.made.macro_owner.get_or_insert_with(|| local(&mut self.u, "macro", Kind::Val, owner, flags))
    }

    /// The empty package, the owner of the definitions at the root (dotty's `EmptyPackageClass`).
    fn empty_package(&mut self) -> SymbolId {
        let root = self.u.root();
        let made = &mut self.made.empty_package;
        *made.get_or_insert_with(|| local(&mut self.u, "<empty>", Kind::Package, root, Flags::default()))
    }

    /// Runs symbol operation `op` on `args`.
    pub fn run_symbol(&mut self, op: &str, args: &[i32]) -> Answer {
        let arg = |i: usize| args.get(i).copied().ok_or("a quotes.reflect operation without its arguments");
        let text = |s: String| Ok((Vec::new(), Some(s)));
        let one = |s: Option<SymbolId>| Ok((vec![s.map_or(-1, |s| s.0 as i32)], None));
        match op {
            "spliceOwner" => one(Some(self.macro_owner())),
            "root" => one(Some(self.u.root())),
            "emptyPackage" => one(Some(self.empty_package())),
            "owner" => {
                let s = symbol(arg(0)?)?;
                match self.u.symbol(s).owner {
                    Some(o) if o == self.u.root() && self.u.symbol(s).kind != Kind::Package => {
                        one(Some(self.empty_package()))
                    }
                    owner => one(owner),
                }
            }
            "name" => {
                let s = symbol(arg(0)?)?;
                let name = self.u.symbol(s).name.to_string();
                match name.is_empty() {
                    true if s == self.u.root() => text("<root>".into()),
                    true => text("<empty>".into()),
                    false => text(name),
                }
            }
            "fullName" => text(self.u.full_name(symbol(arg(0)?)?)),
            "flags" => {
                let bits = self.flags(symbol(arg(0)?)?);
                Ok((vec![bits as u32 as i32, (bits >> 32) as u32 as i32], None))
            }
            "kind" => {
                let kind = self.u.symbol(symbol(arg(0)?)?).kind;
                let n = [
                    Kind::Package,
                    Kind::Class,
                    Kind::Type,
                    Kind::TypeParam,
                    Kind::Param,
                    Kind::Val,
                    Kind::Def,
                ]
                .iter()
                .position(|&k| k == kind);
                Ok((vec![n.map_or(-1, |n| n as i32)], None))
            }
            "moduleClass" => {
                let s = symbol(arg(0)?)?;
                let class = match self.u.symbol(s).kind {
                    Kind::Package => Some(s),
                    _ if self.u.is_module_class(s) => Some(s),
                    _ if self.u.is_module(s) => self.u.module_class(s),
                    _ => None,
                };
                one(class)
            }
            _ => Err("a quotes.reflect operation the frontend does not know"),
        }
    }

    /// The flags of `s` as dotty's `TreeUnpickler` reads them (`readModifiers`, `normalizeFlags`),
    /// but for `Deferred`, `ParamAccessor`, `NoInits` and `StableRealizable`.
    fn flags(&self, s: SymbolId) -> u64 {
        let sym = self.u.symbol(s);
        let has = |tag: u8| sym.flags.has(tag);
        let bit = |tag: u8| 1u64 << tag;
        let mut bits = sym.flags.0 & !0b11;
        if has(PROTECTEDqualified) {
            bits |= bit(PROTECTED);
        }
        if sym.kind.is_term() && has(ABSTRACT) && has(OVERRIDE) {
            bits = bits & !bit(ABSTRACT) | 1 << ABS_OVERRIDE;
        }
        if has(OBJECT) {
            bits |= bit(FINAL) | if sym.kind.is_term() { bit(LAZY) } else { 0 };
        }
        let in_class = sym.owner.is_some_and(|o| self.u.symbol(o).kind == Kind::Class);
        bits | match sym.kind {
            Kind::Class if self.u.is_scala2(s) => 1 << SCALA2X,
            Kind::Def => 1 << METHOD,
            Kind::Package => 1 << PACKAGE | 1 << JAVA_DEFINED | bit(OBJECT) | bit(FINAL),
            Kind::Param if in_class => 0,
            Kind::Param | Kind::TypeParam => 1 << PARAM,
            _ => 0,
        }
    }
}

/// Symbol `name` of `kind` with `flags`, owned by `owner` but none of its declarations.
fn local(u: &mut Universe, name: &str, kind: Kind, owner: SymbolId, flags: Flags) -> SymbolId {
    let s = u.define(Symbol {
        name: name.to_owned(),
        kind,
        owner: None,
        flags,
        target: None,
        is_extern: false,
        derived: false,
        decls: Vec::new(),
        tree: None,
    });
    u.set_owner(s, owner);
    s
}
