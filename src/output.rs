#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    #[default]
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
        match s {
            "compact" => Ok(Self::Compact),
            "wide" => Ok(Self::Wide),
            "full" => Ok(Self::Full),
            _ => anyhow::bail!("unknown output format `{s}`: expected compact, wide or full"),
        }
    }
}
