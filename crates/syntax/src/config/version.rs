// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/config/SourceVersion.scala
/// The `-source` version, as dotty's `SourceVersion`: `3.minor`, `2.13` (between `3.10` and
/// `future`) or `future`, possibly in its `-migration` variant, which counts as the version
/// itself in comparisons.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SourceVersion {
    /// The minor version; `SCALA2` for `2.13`, `FUTURE` for `future`.
    minor: u8,
    pub migrating: bool,
}

impl SourceVersion {
    const SCALA2: u8 = 98;
    const FUTURE: u8 = 99;
    pub const V3_0: SourceVersion = SourceVersion::stable(0);
    pub const V3_2: SourceVersion = SourceVersion::stable(2);
    pub const V3_3: SourceVersion = SourceVersion::stable(3);
    pub const V3_4: SourceVersion = SourceVersion::stable(4);
    pub const V3_6: SourceVersion = SourceVersion::stable(6);
    pub const V3_7: SourceVersion = SourceVersion::stable(7);
    pub const V3_8: SourceVersion = SourceVersion::stable(8);
    pub const V3_9: SourceVersion = SourceVersion::stable(9);
    pub const FUTURE_VERSION: SourceVersion = SourceVersion::stable(Self::FUTURE);

    const fn stable(minor: u8) -> SourceVersion {
        SourceVersion { minor, migrating: false }
    }

    /// `3.x`, `3.x-migration`, `2.13`, `future` or `future-migration`.
    pub fn parse(s: &str) -> Option<Self> {
        if s == "2.13" {
            return Some(SourceVersion::stable(Self::SCALA2));
        }
        let (base, migrating) = s.strip_suffix("-migration").map_or((s, false), |b| (b, true));
        let minor = match base {
            "future" => Self::FUTURE,
            v => (0..=10u8).find(|m| v.strip_prefix("3.") == Some(&m.to_string()))?,
        };
        Some(SourceVersion { minor, migrating })
    }

    /// The `-migration` variant of this version.
    pub fn migrating_variant(self) -> SourceVersion {
        SourceVersion { migrating: true, ..self }
    }

    pub fn is_at_least(self, v: SourceVersion) -> bool {
        self.minor >= v.minor
    }

    /// `2.13`: the unit behaves as the Scala 2 standard library (dotty's `shouldBehaveAsScala2`).
    pub fn is_scala2(self) -> bool {
        self.minor == Self::SCALA2
    }

    pub fn migrate_to_3(self) -> bool {
        self.minor == 0 && self.migrating
    }

    pub fn enables_fewer_braces(self) -> bool {
        self.is_at_least(Self::V3_3)
    }

    pub fn enables_clause_interleaving(self) -> bool {
        self.is_at_least(Self::V3_6)
    }

    pub fn enables_new_givens(self) -> bool {
        self.is_at_least(Self::V3_6)
    }

    pub fn enables_named_tuples(self) -> bool {
        self.is_at_least(Self::V3_7)
    }

    pub fn requires_new_syntax(self) -> bool {
        self.is_at_least(Self::FUTURE_VERSION)
    }
}
