#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Compact,
    Wide,
    Full,
}

impl OutputFormat {
    pub const ALL: &'static [(&'static str, &'static str)] = &[
        ("compact", "Minimal columns from the list endpoint"),
        ("wide", "Rich columns from the inspect endpoint"),
        ("full", "The raw inspect payload, converted verbatim"),
    ];
}

impl std::str::FromStr for OutputFormat {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match Self::ALL.iter().position(|(name, _)| *name == s) {
            Some(0) => Ok(Self::Compact),
            Some(1) => Ok(Self::Wide),
            Some(2) => Ok(Self::Full),
            _ => anyhow::bail!("unknown output format `{s}`: expected compact, wide or full"),
        }
    }
}
