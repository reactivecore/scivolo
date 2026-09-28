//! What the compiler settings and language imports decide about syntax.

/// The `-source` version, as dotty's `SourceVersion`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum SourceVersion {
    V3_0Migration,
    V3_0,
    V3_1,
    V3_2,
    V3_3,
    V3_4,
    V3_5,
    V3_6,
    V3_7,
    V3_8,
    V3_9,
    Future,
}

impl SourceVersion {
    pub fn parse(s: &str) -> Option<Self> {
        use SourceVersion::*;
        Some(match s.trim_end_matches("-migration") {
            "3.0" if s.ends_with("-migration") => V3_0Migration,
            "3.0" => V3_0,
            "3.1" => V3_1,
            "3.2" => V3_2,
            "3.3" => V3_3,
            "3.4" => V3_4,
            "3.5" => V3_5,
            "3.6" => V3_6,
            "3.7" => V3_7,
            "3.8" => V3_8,
            "3.9" => V3_9,
            "future" => Future,
            _ => return None,
        })
    }

    pub fn migrate_to_3(self) -> bool {
        self == SourceVersion::V3_0Migration
    }

    pub fn enables_fewer_braces(self) -> bool {
        self >= SourceVersion::V3_3
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
    pub features: Features,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            source_version: SourceVersion::V3_9,
            no_indent: false,
            indent: false,
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
            let mut value = || value.or_else(|| args.next()).unwrap_or("");
            match key {
                "-source" => cfg.source_version = SourceVersion::parse(value()).unwrap_or(cfg.source_version),
                "-no-indent" | "-noindent" | "-old-syntax" => cfg.no_indent = true,
                "-indent" => cfg.indent = true,
                "-language" => {
                    for feature in value().split(',') {
                        cfg.features.insert(match feature {
                            "experimental.erasedDefinitions" => Features::ERASED_DEFINITIONS,
                            "experimental.modularity" => Features::MODULARITY,
                            "postfixOps" => Features::POSTFIX_OPS,
                            "experimental.captureChecking" => Features::CAPTURE_CHECKING,
                            _ => continue,
                        });
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
