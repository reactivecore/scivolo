//! Scala compiler options read into a `Config`.

use super::{Config, Features, KindProjector, SourceVersion};

/// The `-language:` features that only silence feature warnings.
const WARNING_ONLY_FEATURES: [&str; 4] =
    ["implicitConversions", "reflectiveCalls", "higherKinds", "existentials"];

/// The `-language:` features that make every top-level definition experimental, in the order
/// of `scala.language.experimental` (dotty's `experimentalAutoEnableFeatures`), as dotty's
/// `checkAndAdaptExperimentalImports` names them.
const VIRAL_FEATURES: [(Features, &str); 10] = [
    (Features::NAMED_TYPE_ARGUMENTS, "-language:experimental.experimental.namedTypeArguments"),
    (Features::GENERIC_NUMBER_LITERALS, "-language:experimental.experimental.genericNumberLiterals"),
    (Features::ERASED_DEFINITIONS, "-language:experimental.experimental.erasedDefinitions"),
    (Features::SAFER_EXCEPTIONS, "-language:experimental.experimental.saferExceptions"),
    (Features::PURE_FUNCTIONS, "-language:experimental.experimental.pureFunctions"),
    (Features::INTO, "-language:experimental.experimental.into"),
    (Features::MODULARITY, "-language:experimental.experimental.modularity"),
    (Features::MULTI_SPREADS, "-language:experimental.experimental.multiSpreads"),
    (Features::SUB_CASES, "-language:experimental.experimental.subCases"),
    (Features::RELAXED_LAMBDA_SYNTAX, "-language:experimental.experimental.relaxedLambdaSyntax"),
];

impl Config {
    /// The settings given by scalac options such as `-source:future`, `-no-indent`,
    /// `-Xkind-projector`, `-Werror` and `-language:experimental.erasedDefinitions`; other
    /// options are ignored.
    pub fn from_options(options: &[&str]) -> Config {
        Config::read(options, |_| {})
    }

    /// The settings given by `options`, or the first option the frontend does not implement.
    pub fn checked(options: &[&str]) -> Result<Config, String> {
        let mut unknown = None;
        let cfg = Config::read(options, |o| {
            unknown.get_or_insert_with(|| o.to_owned());
        });
        unknown.map_or(Ok(cfg), Err)
    }

    fn read(options: &[&str], mut unknown: impl FnMut(&str)) -> Config {
        let (mut cfg, mut experimental) = (Config::default(), false);
        let mut args = options.iter().copied().peekable();
        while let Some(arg) = args.next() {
            let (key, colon_value) = match arg.split_once(':') {
                Some((k, v)) => (k, Some(v)),
                None => (arg, None),
            };
            let key = key.strip_prefix('-').filter(|k| k.starts_with('-')).unwrap_or(key);
            let mut value = || colon_value.or_else(|| args.next_if(|a| !a.starts_with('-'))).unwrap_or("");
            match key {
                "-source" => {
                    let v = value();
                    match SourceVersion::parse(v) {
                        Some(sv) if sv.is_scala2() => unknown(arg),
                        Some(sv) if !v.ends_with("-migration") => cfg.source_version = sv,
                        Some(sv) if v != "3.1-migration" => {
                            cfg.source_version = sv;
                            unknown(arg)
                        }
                        _ => unknown(arg),
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
                "-experimental" => experimental = true,
                "-Yexplicit-nulls" => cfg.explicit_nulls = true,
                "-language" => {
                    for name in value().split(',') {
                        match Features::named(name) {
                            Some(feature) => cfg.features.insert(feature),
                            None if WARNING_ONLY_FEATURES.contains(&name) => {}
                            None => unknown(arg),
                        }
                    }
                }
                "-Xkind-projector" => match colon_value {
                    None | Some("") => cfg.kind_projector = KindProjector::Asterisks,
                    Some("underscores") => cfg.kind_projector = KindProjector::Underscores,
                    Some("disable") => cfg.kind_projector = KindProjector::Disabled,
                    Some(_) => unknown(arg),
                },
                "-Werror" | "-Xfatal-warnings" => {
                    cfg.warnings.fatal = true;
                    unknown(arg)
                }
                _ => {
                    if !cfg.warnings.apply(key, colon_value) {
                        unknown(arg)
                    }
                }
            }
        }
        let viral = VIRAL_FEATURES.iter().find(|(f, _)| cfg.features.contains(*f));
        cfg.experimental = viral.map(|&(_, why)| why).or(experimental.then_some("-experimental"));
        cfg
    }
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn names_the_first_option_not_implemented() {
        let bad: [&[&str]; 7] = [
            &["-source"],
            &["-source:3.0-migration"],
            &["-source:nonsense"],
            &["-Xkind-projector:other"],
            &["-Werror"],
            &["-Wconf:any:e"],
            &["-deprecation"],
        ];
        for options in bad {
            assert_eq!(Config::checked(options).err().as_deref(), Some(options[0]), "{options:?}");
        }
        let good = ["-nowarn", "-language:implicitConversions,postfixOps", "-Xkind-projector:disable"];
        assert!(Config::checked(&good).is_ok());
        assert!(Config::checked(&["-Yexplicit-nulls"]).unwrap().explicit_nulls);
        assert!(Config::checked(&["-Wnonunit-statement", "-source", "3.9"]).is_ok());
    }

    #[test]
    fn experimental_settings() {
        let why = |options: &[&str]| Config::checked(options).unwrap().experimental;
        assert_eq!(why(&["-experimental"]), Some("-experimental"));
        let viral = ["-language:experimental.modularity,experimental.erasedDefinitions", "-experimental"];
        assert_eq!(why(&viral), Some("-language:experimental.experimental.erasedDefinitions"));
        assert_eq!(why(&["-language:experimental.captureChecking"]), None);
    }

    #[test]
    fn a_missing_value_leaves_the_next_option() {
        assert!(Config::from_options(&["-source", "-nowarn"]).warnings.nowarn);
        assert_eq!(Config::checked(&["-source", "-nowarn"]).err().as_deref(), Some("-source"));
    }
}
