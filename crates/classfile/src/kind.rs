/// The kind of a JVM value: its field descriptor's first letter, arrays and classes as one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Boolean,
    Byte,
    Char,
    Short,
    Int,
    Long,
    Float,
    Double,
    Reference,
    Void,
}

impl Kind {
    /// Determines the kind of field or result descriptor `desc`; panics on another string.
    pub fn of(desc: &str) -> Kind {
        desc.bytes()
            .next()
            .and_then(Kind::of_letter)
            .unwrap_or_else(|| panic!("no field descriptor: {desc:?}"))
    }

    /// Determines the kind of descriptor letter `c`; `L` and `[` give `Reference`, none another
    /// letter.
    pub fn of_letter(c: u8) -> Option<Kind> {
        Some(match c {
            b'Z' => Kind::Boolean,
            b'B' => Kind::Byte,
            b'C' => Kind::Char,
            b'S' => Kind::Short,
            b'I' => Kind::Int,
            b'J' => Kind::Long,
            b'F' => Kind::Float,
            b'D' => Kind::Double,
            b'V' => Kind::Void,
            b'L' | b'[' => Kind::Reference,
            _ => return None,
        })
    }

    /// The descriptor of a primitive kind, `Ljava/lang/Object;` for `Reference`.
    pub fn descriptor(self) -> &'static str {
        match self {
            Kind::Boolean => "Z",
            Kind::Byte => "B",
            Kind::Char => "C",
            Kind::Short => "S",
            Kind::Int => "I",
            Kind::Long => "J",
            Kind::Float => "F",
            Kind::Double => "D",
            Kind::Reference => "Ljava/lang/Object;",
            Kind::Void => "V",
        }
    }

    /// The slots a value of this kind takes.
    pub fn slots(self) -> i32 {
        match self {
            Kind::Void => 0,
            Kind::Long | Kind::Double => 2,
            _ => 1,
        }
    }

    /// Determines the kind operations compute in: `Int` for the kinds narrower than `int`.
    pub fn widened(self) -> Kind {
        match self {
            Kind::Boolean | Kind::Byte | Kind::Char | Kind::Short => Kind::Int,
            k => k,
        }
    }

    /// The offset from the `int` instruction in the families of order `i l f d a` (loads, stores,
    /// returns, arithmetic).
    pub fn offset(self) -> u8 {
        match self.widened() {
            Kind::Long => 1,
            Kind::Float => 2,
            Kind::Double => 3,
            Kind::Reference => 4,
            _ => 0,
        }
    }

    /// The offset of `Int`, `Long`, `Float` or `Double` in the families of order `i l f d`, none
    /// for another kind.
    pub fn numeric_offset(self) -> Option<u8> {
        matches!(self, Kind::Int | Kind::Long | Kind::Float | Kind::Double).then(|| self.offset())
    }

    /// The offset from `iaload` / `iastore` of the array element instructions of this kind.
    pub fn element_offset(self) -> u8 {
        match self {
            Kind::Boolean | Kind::Byte => 5,
            Kind::Char => 6,
            Kind::Short => 7,
            k => k.offset(),
        }
    }

    /// The `newarray` type code of a primitive kind (JVMS §6.5), none for `Reference`.
    pub fn array_type(self) -> Option<u8> {
        Some(match self {
            Kind::Boolean => 4,
            Kind::Char => 5,
            Kind::Float => 6,
            Kind::Double => 7,
            Kind::Byte => 8,
            Kind::Short => 9,
            Kind::Int => 10,
            Kind::Long => 11,
            Kind::Reference | Kind::Void => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Kind;
    use crate::opcodes::*;

    const ALL: [Kind; 10] = [
        Kind::Boolean,
        Kind::Byte,
        Kind::Char,
        Kind::Short,
        Kind::Int,
        Kind::Long,
        Kind::Float,
        Kind::Double,
        Kind::Reference,
        Kind::Void,
    ];

    #[test]
    fn descriptors_round_trip() {
        for k in ALL {
            assert_eq!(Kind::of_letter(k.descriptor().as_bytes()[0]), Some(k));
            assert_eq!(Kind::of(k.descriptor()), k);
        }
        assert_eq!(Kind::of("[I"), Kind::Reference);
        assert_eq!(Kind::of_letter(b'x'), None);
    }

    #[test]
    #[should_panic(expected = "no field descriptor")]
    fn malformed_descriptor_panics() {
        Kind::of("java/lang/String");
    }

    #[test]
    fn families_reach_the_jvm_opcodes() {
        assert_eq!(ILOAD + Kind::Reference.offset(), 0x19);
        assert_eq!(ILOAD_0 + SHORT_VARS * Kind::Double.offset() + 3, 0x29);
        assert_eq!(ISTORE_0 + SHORT_VARS * Kind::Reference.offset(), 0x4b);
        assert_eq!(IALOAD + Kind::Short.element_offset(), 0x35);
        assert_eq!(IASTORE + Kind::Boolean.element_offset(), 0x54);
        assert_eq!(IASTORE + Kind::Char.element_offset(), 0x55);
        assert_eq!(IRETURN + Kind::Reference.offset(), 0xb0);
        assert_eq!(INEG + Kind::Double.offset(), 0x77);
        assert_eq!(IXOR + Kind::Long.offset(), 0x83);
        assert_eq!(CONVERSIONS[3][2], 0x90);
        assert_eq!(CONVERSIONS[1][0], 0x88);
        assert_eq!(Kind::Int.array_type(), Some(10));
        assert_eq!((IFNULL, IFNONNULL, GOTO_W, WIDE, ATHROW), (0xc6, 0xc7, 0xc8, 0xc4, 0xbf));
        assert_eq!((IF_ICMPEQ, IF_ACMPEQ, IFEQ, LCMP, DCMPL), (0x9f, 0xa5, 0x99, 0x94, 0x97));
    }
}
