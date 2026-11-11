// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Definitions.scala

use crate::symbols::{Kind, SymbolId, Universe};

/// The well-known symbols: the packages `scala` and `java.lang` and the classes without TASTy,
/// entered with the universe; `Predef`, `Unit` and `String`, entered by `enter_library_defn`;
/// and those each copy resolves on its first use.
#[derive(Clone, Copy)]
pub struct Defn {
    pub scala: SymbolId,
    pub java_lang: SymbolId,
    pub any: SymbolId,
    pub any_kind: SymbolId,
    pub matchable: SymbolId,
    pub nothing: SymbolId,
    pub null: SymbolId,
    pub singleton: SymbolId,
    pub any_ref: SymbolId,
    pub object: SymbolId,
    /// dotty's `<special-ops>.<FromJavaObject>`, the alias of `Object` that Java signatures have
    /// for it and that every type conforms to.
    pub from_java_object: SymbolId,
    /// `scala.AnyVal`, from the library's TASTy.
    pub any_val: Option<SymbolId>,
    /// The `Predef` object, its value; the root package until `enter_library_defn` has run.
    pub predef: SymbolId,
    /// The root package until `enter_library_defn` has run.
    pub unit: SymbolId,
    /// The root package until `enter_library_defn` has run.
    pub string: SymbolId,
    pub on_first_use: OnFirstUse,
}

/// The well-known symbols a copy of `Defn` resolves on first use.
#[derive(Clone, Copy)]
pub struct OnFirstUse {
    throwable: Option<SymbolId>,
    array: Option<SymbolId>,
    main_annot: Option<SymbolId>,
    command_line_parser: Option<SymbolId>,
}

impl Defn {
    /// The placeholder until the universe has entered its builtins.
    pub const PENDING: Defn = {
        let r = SymbolId(0);
        Defn {
            scala: r,
            java_lang: r,
            any: r,
            any_kind: r,
            matchable: r,
            nothing: r,
            null: r,
            singleton: r,
            any_ref: r,
            object: r,
            from_java_object: r,
            any_val: None,
            predef: r,
            unit: r,
            string: r,
            on_first_use: OnFirstUse {
                throwable: None,
                array: None,
                main_annot: None,
                command_line_parser: None,
            },
        }
    };

    pub fn throwable(&mut self, u: &mut Universe) -> Result<SymbolId, String> {
        resolved(&mut self.on_first_use.throwable, u, self.java_lang, "Throwable", Kind::Class)
    }

    pub fn array(&mut self, u: &mut Universe) -> Result<SymbolId, String> {
        resolved(&mut self.on_first_use.array, u, self.scala, "Array", Kind::Class)
    }

    pub fn main_annot(&mut self, u: &mut Universe) -> Result<SymbolId, String> {
        resolved(&mut self.on_first_use.main_annot, u, self.scala, "main", Kind::Class)
    }

    /// The `scala.util.CommandLineParser` object, its value.
    pub fn command_line_parser(&mut self, u: &mut Universe) -> Result<SymbolId, String> {
        let slot = &mut self.on_first_use.command_line_parser;
        if let Some(s) = *slot {
            return Ok(s);
        }
        let util = u.package("scala.util").ok_or("package scala.util missing from the library")?;
        resolved(slot, u, util, "CommandLineParser", Kind::Val)
    }
}

impl Universe {
    /// The well-known symbols, those resolved on first use not yet resolved.
    pub fn defn(&self) -> Defn {
        self.defn
    }

    /// Enters `Predef`, `Unit` and `String` into the well-known symbols, looked up anew, and
    /// returns them.
    pub fn enter_library_defn(&mut self) -> Result<Defn, String> {
        let d = self.defn;
        let predef = self.library_member(d.scala, "Predef", Kind::Val)?;
        let unit = self.library_member(d.scala, "Unit", Kind::Class)?;
        let object = self.library_member(d.java_lang, "Object", Kind::Class)?;
        debug_assert_eq!(object, d.object);
        let string = self.library_member(d.java_lang, "String", Kind::Class)?;
        self.defn = Defn { predef, unit, string, ..d };
        Ok(self.defn)
    }

    /// The first declaration `name` of `kind` of `owner`, else why there is none.
    pub fn library_member(&mut self, owner: SymbolId, name: &str, kind: Kind) -> Result<SymbolId, String> {
        let found = self.decl(owner, name).map_err(|e| e.to_string())?;
        found
            .into_iter()
            .find(|&s| self.symbol(s).kind == kind)
            .ok_or_else(|| format!("{}.{name} missing from the library", self.full_name(owner)))
    }
}

/// `slot`, else the member `name` of `kind` of `owner`, kept in `slot`.
fn resolved(
    slot: &mut Option<SymbolId>,
    u: &mut Universe,
    owner: SymbolId,
    name: &str,
    kind: Kind,
) -> Result<SymbolId, String> {
    if let Some(s) = *slot {
        return Ok(s);
    }
    let s = u.library_member(owner, name, kind)?;
    *slot = Some(s);
    Ok(s)
}
