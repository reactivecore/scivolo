//! What the compiler settings and language imports decide about syntax.

/// The `-source` version, as dotty's `SourceVersion`: `3.minor` or `future`, possibly in
/// its `-migration` variant, which counts as the version itself in comparisons.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SourceVersion {
    /// The minor version; `FUTURE` for `future`.
    minor: u8,
    pub migrating: bool,
}

impl SourceVersion {
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

    /// `3.x`, `3.x-migration`, `future` or `future-migration`.
    pub fn parse(s: &str) -> Option<Self> {
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

    pub fn enables_better_fors(self) -> bool {
        self.is_at_least(Self::V3_8)
    }

    pub fn requires_new_syntax(self) -> bool {
        self.is_at_least(Self::FUTURE_VERSION)
    }
}

/// Language features that change scanning or parsing.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct Features(u16);

impl Features {
    pub const ERASED_DEFINITIONS: Features = Features(1);
    pub const MODULARITY: Features = Features(1 << 1);
    pub const POSTFIX_OPS: Features = Features(1 << 2);
    pub const CAPTURE_CHECKING: Features = Features(1 << 3);
    pub const SYMBOL_LITERALS: Features = Features(1 << 4);
    pub const RELAXED_LAMBDA_SYNTAX: Features = Features(1 << 5);
    pub const SUB_CASES: Features = Features(1 << 6);
    pub const MULTI_SPREADS: Features = Features(1 << 7);
    pub const PURE_FUNCTIONS: Features = Features(1 << 8);
    pub const INTO: Features = Features(1 << 9);

    /// The feature a `-language:` option or `import scala.language.` path names.
    pub fn named(name: &str) -> Option<Features> {
        Some(match name {
            "experimental.erasedDefinitions" => Features::ERASED_DEFINITIONS,
            "experimental.modularity" => Features::MODULARITY,
            "postfixOps" => Features::POSTFIX_OPS,
            "experimental.captureChecking" | "experimental.separationChecking" | "experimental.safe" => {
                Features::CAPTURE_CHECKING
            }
            "deprecated.symbolLiterals" => Features::SYMBOL_LITERALS,
            "experimental.relaxedLambdaSyntax" => Features::RELAXED_LAMBDA_SYNTAX,
            "experimental.subCases" => Features::SUB_CASES,
            "experimental.multiSpreads" => Features::MULTI_SPREADS,
            "experimental.pureFunctions" => Features::PURE_FUNCTIONS,
            "experimental.into" => Features::INTO,
            _ => return None,
        })
    }

    pub fn contains(self, f: Features) -> bool {
        self.0 & f.0 == f.0
    }

    pub fn insert(&mut self, f: Features) {
        self.0 |= f.0;
    }
}

/// The settings of one compilation unit that the scanner reads.
#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub source_version: SourceVersion,
    /// `-no-indent` or `-old-syntax`.
    pub no_indent: bool,
    /// `-indent`.
    pub indent: bool,
    /// `-old-syntax`.
    pub old_syntax: bool,
    /// `-new-syntax`.
    pub new_syntax: bool,
    pub features: Features,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            source_version: SourceVersion::V3_9,
            no_indent: false,
            indent: false,
            old_syntax: false,
            new_syntax: false,
            features: Features::default(),
        }
    }
}

impl Config {
    /// The settings given by scalac options such as `-source:future`, `-no-indent` and
    /// `-language:experimental.erasedDefinitions`; other options are ignored.
    pub fn from_options(options: &[&str]) -> Config {
        let mut cfg = Config::default();
        let mut args = options.iter().copied();
        while let Some(arg) = args.next() {
            let (key, value) = match arg.split_once(':') {
                Some((k, v)) => (k, Some(v)),
                None => (arg, None),
            };
            let key = key.strip_prefix('-').filter(|k| k.starts_with('-')).unwrap_or(key);
            let mut value = || value.or_else(|| args.next()).unwrap_or("");
            match key {
                "-source" => {
                    let v = value();
                    if v != "3.1-migration"
                        && let Some(v) = SourceVersion::parse(v)
                    {
                        cfg.source_version = v;
                    }
                }
                "-no-indent" | "-noindent" => cfg.no_indent = true,
                "-old-syntax" => {
                    cfg.no_indent = true;
                    cfg.old_syntax = true;
                }
                "-new-syntax" => cfg.new_syntax = true,
                "-indent" => cfg.indent = true,
                "-language" => {
                    for feature in value().split(',').filter_map(Features::named) {
                        cfg.features.insert(feature);
                    }
                }
                _ => {}
            }
        }
        cfg
    }

    /// Whether indentation is significant, as dotty's `indentSyntax` with `allowIndent`.
    pub fn indent_syntax(&self) -> bool {
        !(self.no_indent || self.source_version.migrate_to_3() && !self.indent)
    }
}
