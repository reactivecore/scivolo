/// The warning settings: `-Werror`, `-nowarn` and the `-W` lints.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct Warnings {
    /// `-Werror` or `-Xfatal-warnings`.
    pub fatal: bool,
    /// `-nowarn`.
    pub nowarn: bool,
    pub lints: Lints,
}

/// The `-W` lints the typer implements.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct Lints(u8);

impl Lints {
    /// The lint a `-W<name>` option names.
    pub fn named(_name: &str) -> Option<Lints> {
        None
    }

    pub fn contains(self, l: Lints) -> bool {
        self.0 & l.0 == l.0
    }

    pub fn insert(&mut self, l: Lints) {
        self.0 |= l.0;
    }
}

impl Warnings {
    /// Applies option `key` (with its value after `:`, if any); `false` if it is no warning
    /// option this type knows.
    pub fn apply(&mut self, key: &str, colon_value: Option<&str>) -> bool {
        match (key, colon_value) {
            ("-Werror" | "-Xfatal-warnings", None) => self.fatal = true,
            ("-nowarn", None) => self.nowarn = true,
            _ => match key.strip_prefix("-W").filter(|_| colon_value.is_none()).and_then(Lints::named) {
                Some(lint) => self.lints.insert(lint),
                None => return false,
            },
        }
        true
    }
}
