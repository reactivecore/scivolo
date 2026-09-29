//! Flag sets with dotty's bit layout (`core/Flags.scala`): bits 0 and 1 say whether a set
//! applies to terms, types or both, bits 2..63 are flags.

#[derive(Clone, Copy, PartialEq, Eq, Hash, Default, Debug)]
pub struct FlagSet(u64);

const TERMS: u64 = 1;
const TYPES: u64 = 2;
const KINDS: u64 = TERMS | TYPES;

const fn both(bit: u32) -> FlagSet {
    FlagSet(KINDS | 1 << bit)
}

const fn term(bit: u32) -> FlagSet {
    FlagSet(TERMS | 1 << bit)
}

const fn ty(bit: u32) -> FlagSet {
    FlagSet(TYPES | 1 << bit)
}

pub const EMPTY: FlagSet = FlagSet(0);
pub const PRIVATE: FlagSet = both(2);
pub const PROTECTED: FlagSet = both(3);
pub const OVERRIDE: FlagSet = both(4);
pub const DEFERRED: FlagSet = both(5);
pub const FINAL: FlagSet = both(6);
pub const METHOD: FlagSet = term(7);
pub const PARAM: FlagSet = both(8);
pub const TERM_PARAM: FlagSet = term(8);
pub const TYPE_PARAM: FlagSet = ty(8);
pub const IMPLICIT: FlagSet = both(9);
pub const LAZY: FlagSet = term(10);
pub const TRAIT: FlagSet = ty(10);
pub const SEALED: FlagSet = ty(11);
pub const ACCESSOR: FlagSet = term(11);
pub const MUTABLE: FlagSet = term(12);
pub const OPEN: FlagSet = ty(12);
pub const LOCAL: FlagSet = both(13);
pub const PARAM_ACCESSOR: FlagSet = term(14);
pub const INTO: FlagSet = ty(14);
pub const MODULE: FlagSet = both(15);
pub const PACKAGE: FlagSet = both(16);
pub const CASE: FlagSet = both(17);
pub const SYNTHETIC: FlagSet = both(18);
pub const INLINE: FlagSet = both(19);
pub const COVARIANT: FlagSet = ty(20);
pub const CONTRAVARIANT: FlagSet = ty(21);
pub const ABS_OVERRIDE: FlagSet = term(22);
pub const ABSTRACT: FlagSet = both(23);
pub const CASE_ACCESSOR: FlagSet = term(25);
pub const HAS_DEFAULT: FlagSet = term(27);
pub const IMPURE: FlagSet = ty(27);
pub const EXTENSION_METHOD: FlagSet = term(28);
pub const GIVEN: FlagSet = both(29);
pub const JAVA_STATIC: FlagSet = both(31);
pub const TRANSPARENT: FlagSet = both(39);
pub const ENUM: FlagSet = both(40);
pub const ERASED: FlagSet = both(42);
pub const OPAQUE: FlagSet = both(43);
pub const INFIX: FlagSet = both(44);
pub const TRACKED: FlagSet = both(46);
pub const MACRO: FlagSet = both(58);

pub const ACCESS_FLAGS: FlagSet = LOCAL.or(PRIVATE).or(PROTECTED);
pub const GIVEN_OR_IMPLICIT: FlagSet = GIVEN.or(IMPLICIT);
pub const PRIVATE_LOCAL: FlagSet = LOCAL.or(PRIVATE);
pub const DEFERRED_GIVEN_FLAGS: FlagSet = DEFERRED.or(GIVEN).or(HAS_DEFAULT);
pub const ENUM_CASE: FlagSet = CASE.or(ENUM);
pub const LOCAL_PARAM_ACCESSOR: FlagSet = LOCAL.or(PARAM_ACCESSOR).or(PRIVATE);
pub const SYNTHETIC_TERM_PARAM: FlagSet = SYNTHETIC.or(TERM_PARAM);

/// The flags source modifiers can set (dotty's `ModifierFlags`).
pub const MODIFIER_FLAGS: FlagSet = {
    let common = PRIVATE.or(PROTECTED).or(FINAL).or(CASE).or(IMPLICIT).or(GIVEN).or(OVERRIDE);
    let common = common.or(JAVA_STATIC).or(TRANSPARENT).or(ERASED);
    let types = common.to_type_flags().or(ABSTRACT).or(SEALED).or(OPAQUE).or(OPEN).or(INTO);
    let terms = common.to_term_flags().or(INLINE).or(ABS_OVERRIDE).or(LAZY).or(TRACKED);
    let extra = MODULE.or(PARAM).or(SYNTHETIC).or(PACKAGE).or(LOCAL);
    FlagSet(types.0 | terms.0 | extra.0 | MUTABLE.0 | TRAIT.0 | KINDS)
};

/// Each flag's name for terms and for types (dotty's `flagName`).
const NAMES: [(u32, &str, &str); 58] = [
    (2, "private", "private"),
    (3, "protected", "protected"),
    (4, "override", "override"),
    (5, "<deferred>", "<deferred>"),
    (6, "final", "final"),
    (7, "<method>", "<method>"),
    (8, "<param>", "<param>"),
    (9, "implicit", "implicit"),
    (10, "lazy", "<trait>"),
    (11, "<accessor>", "sealed"),
    (12, "mutable", "open"),
    (13, "<local>", "<local>"),
    (14, "<paramaccessor>", "into"),
    (15, "module", "module"),
    (16, "<package>", "<package>"),
    (17, "case", "case"),
    (18, "<synthetic>", "<synthetic>"),
    (19, "inline", "inline"),
    (20, "<outer accessor>", "<covariant>"),
    (21, "<label>", "<contravariant>"),
    (22, "abstract override", "interface"),
    (23, "abstract", "abstract"),
    (24, "<stable>", "<stable>"),
    (25, "<caseaccessor>", "<scala-2-tasty>"),
    (26, "<super-param-alias>", "<scala-2.x>"),
    (27, "<hasdefault>", "<impure>"),
    (28, "<extension>", "<extension>"),
    (29, "given", "given"),
    (30, "<java>", "<java>"),
    (31, "<static>", "<static>"),
    (32, "<captured>", "<noinits>"),
    (33, "<artifact>", "<artifact>"),
    (34, "<bridge>", "<bridge>"),
    (35, "<inline proxy>", "<inline proxy>"),
    (36, "<synchronized>", "<synchronized>"),
    (37, "<varargs>", "<java-annotation>"),
    (38, "<defaultmethod>", "<defaultmethod>"),
    (39, "transparent", "transparent"),
    (40, "enum", "enum"),
    (41, "exported", "exported"),
    (42, "erased", "erased"),
    (43, "opaque", "opaque"),
    (44, "infix", "infix"),
    (45, "<invisible>", "<invisible>"),
    (46, "tracked", "tracked"),
    (49, "<non-member>", "<non-member>"),
    (50, "<touched>", "<touched>"),
    (51, "<lifted>", "<lifted>"),
    (52, "<mixedin>", "<mixedin>"),
    (53, "<specialized>", "<specialized>"),
    (54, "<selfname>", "<selfname>"),
    (55, "<existential>", "<existential>"),
    (56, "<children-queried>", "<children-queried>"),
    (57, "<modulevar>/<cc>", "<modulevar>/<cc>"),
    (58, "<macro>", "<macro>"),
    (59, "<has-default-params>", "<has-default-params>"),
    (60, "<no-default-params>", "<provisional>"),
    (61, "<permanent>", "<permanent>"),
];

impl FlagSet {
    /// The union; the result applies to the kinds both sets apply to.
    pub const fn or(self, y: FlagSet) -> FlagSet {
        if self.0 == 0 {
            return y;
        }
        if y.0 == 0 {
            return self;
        }
        FlagSet((self.0 & y.0 & KINDS) | ((self.0 | y.0) & !KINDS))
    }

    pub const fn and(self, y: FlagSet) -> FlagSet {
        FlagSet(self.0 & y.0)
    }

    pub const fn and_not(self, y: FlagSet) -> FlagSet {
        let kinds = self.0 & KINDS;
        if kinds & y.0 == 0 { self } else { FlagSet(kinds | (self.0 & !y.0 & !KINDS)) }
    }

    /// Whether the sets share a kind and a flag.
    pub const fn is_one_of(self, flags: FlagSet) -> bool {
        let fs = self.0 & flags.0;
        fs & KINDS != 0 && fs & !KINDS != 0
    }

    pub const fn is(self, flag: FlagSet) -> bool {
        self.is_one_of(flag)
    }

    pub const fn is_all_of(self, flags: FlagSet) -> bool {
        let fs = self.0 & flags.0;
        (fs & KINDS != 0 || flags.0 == 0) && fs >> 2 == flags.0 >> 2
    }

    pub const fn is_empty(self) -> bool {
        self.0 & !KINDS == 0
    }

    pub const fn is_term_flags(self) -> bool {
        self.0 & TERMS != 0
    }

    pub const fn is_type_flags(self) -> bool {
        self.0 & TYPES != 0
    }

    pub const fn to_type_flags(self) -> FlagSet {
        if self.0 == 0 { self } else { FlagSet(self.0 & !KINDS | TYPES) }
    }

    pub const fn to_term_flags(self) -> FlagSet {
        if self.0 == 0 { self } else { FlagSet(self.0 & !KINDS | TERMS) }
    }

    /// The names of the set flags, as dotty's `flagStrings("")`.
    pub fn flag_strings(self) -> Vec<String> {
        let mut out = Vec::new();
        for (bit, term_name, type_name) in NAMES {
            if self.0 & 1 << bit == 0 {
                continue;
            }
            let t = if self.0 & TERMS != 0 { term_name } else { "" };
            let y = if self.0 & TYPES != 0 { type_name } else { "" };
            out.extend(
                [t, y]
                    .into_iter()
                    .take(if t == y { 1 } else { 2 })
                    .filter(|s| !s.is_empty())
                    .map(String::from),
            );
        }
        if self.is(LOCAL) {
            out.retain(|s| s != "<local>");
            for s in &mut out {
                if s == "private" || s == "protected" {
                    s.push_str("[this]");
                }
            }
        }
        out
    }

    pub fn flags_string(self) -> String {
        self.flag_strings().join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_follow_dotty() {
        assert_eq!(PRIVATE_LOCAL.flags_string(), "private[this]");
        assert_eq!(LAZY.or(FINAL).flags_string(), "final lazy");
        assert_eq!(TRAIT.or(ABSTRACT).flags_string(), "<trait> abstract");
        assert_eq!(COVARIANT.or(LOCAL).flags_string(), "<covariant>");
        assert!(MODIFIER_FLAGS.is(LAZY) && MODIFIER_FLAGS.is(TRAIT) && !MODIFIER_FLAGS.is(METHOD));
    }
}
