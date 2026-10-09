//! What the compiler settings and language imports decide about syntax.

mod version;

pub use version::SourceVersion;

/// Language features that change scanning, parsing or typing.
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
    pub const STRICT_EQUALITY: Features = Features(1 << 10);
    pub const GENERIC_NUMBER_LITERALS: Features = Features(1 << 11);

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
            "strictEquality" => Features::STRICT_EQUALITY,
            "experimental.genericNumberLiterals" => Features::GENERIC_NUMBER_LITERALS,
            _ => return None,
        })
    }

    pub fn contains(self, f: Features) -> bool {
        self.0 & f.0 == f.0
    }

    pub fn insert(&mut self, f: Features) {
        self.0 |= f.0;
    }

    /// dotty's `ccEnabled`.
    pub fn capture_checking(self) -> bool {
        self.contains(Features::CAPTURE_CHECKING)
    }

    /// dotty's `pureFunsEnabled`: `->` is a pure function and `=>` an impure one.
    pub fn pure_functions(self) -> bool {
        self.contains(Features::PURE_FUNCTIONS) || self.capture_checking()
    }
}

/// `-Xkind-projector`: `*` (and with `underscores`, `_`) as type lambda placeholders.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum KindProjector {
    #[default]
    Disabled,
    Asterisks,
    Underscores,
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
    /// `-preview`.
    pub preview: bool,
    pub features: Features,
    pub kind_projector: KindProjector,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            source_version: SourceVersion::V3_9,
            no_indent: false,
            indent: false,
            old_syntax: false,
            new_syntax: false,
            preview: false,
            features: Features::default(),
            kind_projector: KindProjector::Disabled,
        }
    }
}

impl Config {
    /// The settings given by scalac options such as `-source:future`, `-no-indent`,
    /// `-Xkind-projector` and `-language:experimental.erasedDefinitions`; other options are
    /// ignored.
    pub fn from_options(options: &[&str]) -> Config {
        let mut cfg = Config::default();
        let mut args = options.iter().copied();
        while let Some(arg) = args.next() {
            let (key, colon_value) = match arg.split_once(':') {
                Some((k, v)) => (k, Some(v)),
                None => (arg, None),
            };
            let key = key.strip_prefix('-').filter(|k| k.starts_with('-')).unwrap_or(key);
            let mut value = || colon_value.or_else(|| args.next()).unwrap_or("");
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
                "-preview" => cfg.preview = true,
                "-indent" => cfg.indent = true,
                "-language" => {
                    for feature in value().split(',').filter_map(Features::named) {
                        cfg.features.insert(feature);
                    }
                }
                "-Xkind-projector" => {
                    cfg.kind_projector = match colon_value {
                        None | Some("") => KindProjector::Asterisks,
                        Some("underscores") => KindProjector::Underscores,
                        Some(_) => KindProjector::Disabled,
                    }
                }
                _ => {}
            }
        }
        cfg
    }

    /// Whether `for` comprehensions follow the rules of 3.8 (dotty's `enablesBetterFors`).
    pub fn enables_better_fors(&self) -> bool {
        let v = self.source_version;
        v.is_at_least(SourceVersion::V3_8) || v.is_at_least(SourceVersion::V3_7) && self.preview
    }

    /// Whether indentation is significant, as dotty's `indentSyntax` with `allowIndent`.
    pub fn indent_syntax(&self) -> bool {
        !(self.no_indent || self.source_version.migrate_to_3() && !self.indent)
    }
}
