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
    /// Determines the kind of field or result descriptor `desc`.
    pub fn of(desc: &str) -> Kind {
        Kind::of_letter(desc.as_bytes()[0]).unwrap_or(Kind::Reference)
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
