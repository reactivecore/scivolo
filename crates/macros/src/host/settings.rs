/// The compiler options macros see: `-Yretain-trees`, `-Xread-docs` and the values of
/// `-Xmacro-settings`.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct Settings {
    pub retain_trees: bool,
    pub read_docs: bool,
    pub macro_settings: Vec<String>,
}

const MACRO_SETTINGS: &str = "-Xmacro-settings:";
const RETAIN_TREES: &str = "-Yretain-trees";
const READ_DOCS: &str = "-Xread-docs";

impl Settings {
    /// The settings given by `options`; other options are ignored.
    pub fn from_options(options: &[&str]) -> Settings {
        let values = options.iter().filter_map(|o| o.strip_prefix(MACRO_SETTINGS));
        Settings {
            retain_trees: options.contains(&RETAIN_TREES),
            read_docs: options.contains(&READ_DOCS),
            macro_settings: values
                .flat_map(|v| v.split(','))
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
                .collect(),
        }
    }

    /// Whether option `o` is one `from_options` reads.
    pub fn reads(o: &str) -> bool {
        o == RETAIN_TREES || o == READ_DOCS || o.starts_with(MACRO_SETTINGS)
    }
}

#[cfg(test)]
mod tests {
    use super::Settings;

    #[test]
    fn reads_retain_trees() {
        assert!(Settings::from_options(&["-deprecation", "-Yretain-trees"]).retain_trees);
        assert_eq!(Settings::from_options(&["-deprecation"]), Settings::default());
    }

    #[test]
    fn splits_and_appends_macro_settings() {
        let s = Settings::from_options(&["-Xmacro-settings:a,b", "-Xmacro-settings:", "-Xmacro-settings:c"]);
        assert_eq!(s.macro_settings, ["a", "b", "c"]);
        assert!(!s.retain_trees);
    }

    #[test]
    fn names_its_options() {
        assert!(Settings::reads("-Yretain-trees") && Settings::reads("-Xmacro-settings:a"));
        assert!(Settings::reads("-Xread-docs") && Settings::from_options(&["-Xread-docs"]).read_docs);
        assert!(!Settings::reads("-Xmacro-settings") && !Settings::reads("-Werror"));
    }
}
