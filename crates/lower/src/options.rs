use syntax::config::Config;

/// The settings that compiler options give the lowering: the frontend's, whether warnings are
/// fatal, and the macros'.
#[derive(Debug)]
pub struct Options {
    pub config: Config,
    /// `-Werror`, `-Xfatal-warnings` or a `-Wconf` option is given that does not only silence or
    /// keep warnings.
    pub fatal: bool,
    pub macros: macros::Settings,
}

impl Options {
    /// The settings given by `options`, which ignore options without effect on the code
    /// dotty generates that the frontend does not implement; else the first option the
    /// frontend does not implement.
    pub fn checked(options: &[&str]) -> Result<Options, String> {
        let fatal = options.iter().any(|o| ["-Werror", "-Xfatal-warnings"].contains(o) || fatal_wconf(o));
        let macros = macros::Settings::from_options(options);
        let rest: Vec<&str> = options
            .iter()
            .copied()
            .filter(|o| !macros::Settings::reads(o) && (!no_code_effect(o) || Config::checked(&[o]).is_ok()))
            .collect();
        Ok(Options { config: Config::checked(&rest)?, fatal, macros })
    }
}

/// Whether `o` is a `-Wconf` option with a rule whose action is not `silent`, `info`, `verbose`
/// or `warning`.
fn fatal_wconf(o: &str) -> bool {
    let Some(rules) = o.strip_prefix("-Wconf") else { return false };
    let kept = ["s", "silent", "i", "info", "v", "verbose", "w", "warning"];
    rules
        .strip_prefix(':')
        .is_none_or(|r| r.split(',').any(|rule| !kept.contains(&rule.rsplit(':').next().unwrap_or(""))))
}

/// Whether option `o` cannot change the code of a program dotty compiles.
fn no_code_effect(o: &str) -> bool {
    matches!(
        o,
        "-Werror"
            | "-Xfatal-warnings"
            | "-deprecation"
            | "-feature"
            | "-unchecked"
            | "-Yretain-trees"
            | "-Xcheck-macros"
    ) || ["-W", "-opt", "-Ycheck"].iter().any(|p| o.starts_with(p))
}

#[cfg(test)]
mod tests {
    use super::Options;

    #[test]
    fn fatal_warnings() {
        for o in ["-Werror", "-Xfatal-warnings", "-Wconf:any:e", "-Wconf:msg=a:s,cat=deprecation:error"] {
            let options = Options::checked(&[o]).unwrap();
            assert!(options.fatal && !options.config.warnings.fatal, "{o}");
        }
        for o in ["-deprecation", "-Wconf:any:silent", "-Wconf:msg=a:s,any:w"] {
            assert!(!Options::checked(&[o]).unwrap().fatal, "{o}");
        }
    }

    #[test]
    fn ignores_options_without_code_effect() {
        let options = ["-experimental", "-feature", "-unchecked", "-Xcheck-macros", "-opt", "-Ycheck:all"];
        assert!(Options::checked(&options).is_ok());
        let lints = Options::checked(&["-Wunused:all", "-Wnonunit-statement"]).unwrap().config.warnings.lints;
        assert!(lints.contains(syntax::config::Lints::NONUNIT_STATEMENT));
    }

    #[test]
    fn reads_macro_settings() {
        let o = Options::checked(&["-Xmacro-settings:a,b", "-Yretain-trees"]).unwrap();
        assert!(o.macros.retain_trees);
        assert_eq!(o.macros.macro_settings, ["a", "b"]);
    }

    #[test]
    fn names_the_first_option_not_implemented() {
        assert_eq!(
            Options::checked(&["-deprecation", "-Xprint:typer", "-source:x"]).err().as_deref(),
            Some("-Xprint:typer")
        );
        assert_eq!(
            Options::checked(&["-source:3.0-migration"]).err().as_deref(),
            Some("-source:3.0-migration")
        );
    }
}
