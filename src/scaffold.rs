use std::collections::HashMap;

use nu_plugin::EvaluatedCall;
use nu_protocol::{Category, Signature, SyntaxShape, Type};

use crate::helpers::split_csv;
use crate::output::OutputFormat;

pub enum Shape {
    Str,
    Int,
    Switch,
    Labels,
}

pub struct FilterArg {
    pub flag: &'static str,
    pub api_key: &'static str,
    pub shape: Shape,
    pub help: &'static str,
}

impl FilterArg {
    pub const fn string(flag: &'static str, help: &'static str) -> Self {
        Self {
            flag,
            api_key: flag,
            shape: Shape::Str,
            help,
        }
    }
    pub const fn int(flag: &'static str, help: &'static str) -> Self {
        Self {
            flag,
            api_key: flag,
            shape: Shape::Int,
            help,
        }
    }
    pub const fn switch(flag: &'static str, help: &'static str) -> Self {
        Self {
            flag,
            api_key: flag,
            shape: Shape::Switch,
            help,
        }
    }
    pub const fn labels() -> Self {
        Self {
            flag: "labels",
            api_key: "label",
            shape: Shape::Labels,
            help: "Labels, comma-separated: `key` or `key=value` (e.g. `a=b,c=d`)",
        }
    }
}

pub struct CommandSpec {
    pub cmd: &'static str,
    pub noun: &'static str,
    pub all_help: Option<&'static str>,
    pub show_labels: bool,
    pub filters: &'static [FilterArg],
}

const LIST_OUTPUT_HELP: &str = "Output format: compact | wide | full  (default: compact)";
const INSPECT_OUTPUT_HELP: &str = "Output format: wide | full  (default: wide)";

/// The skeleton every nude command shares: `--output`/`-o` and the `docker` category.
fn base(cmd: &str, output_help: &str, output: Type) -> Signature {
    Signature::build(cmd)
        .named("output", SyntaxShape::String, output_help, Some('o'))
        .input_output_types(vec![(Type::Nothing, output)])
        .category(Category::Custom("docker".to_string()))
}

/// `nude <resource> ls`: `--all` (when the endpoint has one), `--show-labels`, and the filters.
pub fn list_signature(spec: &CommandSpec) -> Signature {
    let mut sig = base(spec.cmd, LIST_OUTPUT_HELP, Type::table());
    if let Some(help) = spec.all_help {
        sig = sig.switch("all", help, Some('a'));
    }
    if spec.show_labels {
        sig = sig.switch(
            "show-labels",
            format!(
                "Enrich each {} with a `labels` column (compact/wide; full always has them)",
                spec.noun
            ),
            None,
        );
    }
    add_filters(sig, spec.filters)
}

/// `nude <resource> inspect <name>`: one required ref; `--show-labels` when the noun has labels.
pub fn inspect_signature(cmd: &str, ref_help: &str, show_labels_noun: Option<&str>) -> Signature {
    let mut sig = base(cmd, INSPECT_OUTPUT_HELP, Type::record()).required(
        "name",
        SyntaxShape::String,
        ref_help,
    );
    if let Some(noun) = show_labels_noun {
        sig = sig.switch(
            "show-labels",
            format!("Enrich the {noun} with a `labels` column (wide; full always has them)"),
            None,
        );
    }
    sig
}

/// `nude search <term>`: a required term, `--limit`, and the registry filters.
pub fn search_signature(cmd: &str, term_help: &str, filters: &[FilterArg]) -> Signature {
    let sig = base(cmd, LIST_OUTPUT_HELP, Type::table())
        .required("term", SyntaxShape::String, term_help)
        .named(
            "limit",
            SyntaxShape::Int,
            "Maximum number of results, 1..=100 (default 25)",
            None,
        );
    add_filters(sig, filters)
}

/// `nude system info` / `nude version`: one object, no arguments beyond `--output`.
pub fn singleton_signature(cmd: &str) -> Signature {
    base(cmd, LIST_OUTPUT_HELP, Type::record())
}

/// `nude container top <name>` and friends: a required object ref, a table out.
pub fn subcommand_signature(cmd: &str, ref_help: &str) -> Signature {
    base(cmd, LIST_OUTPUT_HELP, Type::table()).required("name", SyntaxShape::String, ref_help)
}

fn add_filters(mut sig: Signature, filters: &[FilterArg]) -> Signature {
    for f in filters {
        sig = match f.shape {
            Shape::Switch => sig.switch(f.flag, f.help, None),
            Shape::Int => sig.named(f.flag, SyntaxShape::Int, f.help, None),
            Shape::Str | Shape::Labels => sig.named(f.flag, SyntaxShape::String, f.help, None),
        };
    }
    sig
}

pub fn collect_filters(
    call: &EvaluatedCall,
    filters: &[FilterArg],
) -> anyhow::Result<HashMap<String, Vec<String>>> {
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    for f in filters {
        match f.shape {
            Shape::Str => {
                if let Some(v) = call.get_flag::<String>(f.flag)? {
                    out.insert(f.api_key.to_string(), vec![v]);
                }
            }
            Shape::Int => {
                if let Some(n) = call.get_flag::<i64>(f.flag)? {
                    out.insert(f.api_key.to_string(), vec![n.to_string()]);
                }
            }
            Shape::Switch => {
                if call.has_flag(f.flag)? {
                    out.insert(f.api_key.to_string(), vec!["true".to_string()]);
                }
            }
            Shape::Labels => {
                if let Some(s) = call.get_flag::<String>(f.flag)? {
                    let values = split_csv(&s);
                    if !values.is_empty() {
                        out.insert(f.api_key.to_string(), values);
                    }
                }
            }
        }
    }
    Ok(out)
}

fn parse_output(call: &EvaluatedCall) -> anyhow::Result<Option<OutputFormat>> {
    call.get_flag::<String>("output")?
        .map(|s| s.parse())
        .transpose()
}

/// `--output` for list-shaped commands: defaults to `compact`, rejects unknown names.
pub fn list_format(call: &EvaluatedCall) -> anyhow::Result<OutputFormat> {
    Ok(parse_output(call)?.unwrap_or(OutputFormat::Compact))
}

/// `--output` for single-object commands: defaults to `wide`; there is no compact view of one
/// inspect payload, so asking for it is an error rather than a silent `wide`.
pub fn inspect_format(call: &EvaluatedCall) -> anyhow::Result<OutputFormat> {
    match parse_output(call)? {
        None => Ok(OutputFormat::Wide),
        Some(OutputFormat::Compact) => {
            anyhow::bail!("inspect has no compact output: use wide (the default) or full")
        }
        Some(fmt) => Ok(fmt),
    }
}
