use bollard::models::ImageSearchResponseItem;
use bollard::query_parameters::SearchImagesOptionsBuilder;
use nu_plugin::{EngineInterface, EvaluatedCall, SimplePluginCommand};
use nu_protocol::{LabeledError, Record, Signature, Span, Value};

use crate::completers::{self, dynamic_completion};
use crate::helpers::{full_value, opt_bool, opt_int, str_opt};
use crate::output::OutputFormat;
use crate::plugin::NudePlugin;
use crate::scaffold::{self, FilterArg, Shape};

pub struct SearchCommand;

const NAME: &str = "nude search";

const FILTERS: &[FilterArg] = &[
    FilterArg::int("stars", "Only images with at least this many stars"),
    FilterArg {
        flag: "official",
        api_key: "is-official",
        shape: Shape::Switch,
        help: "Only official images (disabled-only: `| where not official`)",
    },
];

impl SimplePluginCommand for SearchCommand {
    type Plugin = NudePlugin;

    fn name(&self) -> &str {
        NAME
    }

    fn description(&self) -> &str {
        "Search Docker Hub for images"
    }

    fn signature(&self) -> Signature {
        scaffold::search_signature(
            NAME,
            "Search term (matched against Docker Hub image names & descriptions)",
            FILTERS,
        )
    }

    fn run(
        &self,
        plugin: &Self::Plugin,
        _engine: &EngineInterface,
        call: &EvaluatedCall,
        _input: &Value,
    ) -> Result<Value, LabeledError> {
        plugin.block_on_labeled(run(plugin, call))
    }

    dynamic_completion!(|plugin, call, arg_type| completers::flags(arg_type, |_| None));
}

async fn run(plugin: &NudePlugin, call: &EvaluatedCall) -> anyhow::Result<Value> {
    let term: String = call.req(0)?;
    let span = call.head;
    let fmt = scaffold::list_format(call)?;
    let filters = scaffold::collect_filters(call, FILTERS)?;

    let mut builder = SearchImagesOptionsBuilder::new()
        .term(&term)
        .filters(&filters);
    if let Some(limit) = call.get_flag::<i64>("limit")? {
        if !(1..=100).contains(&limit) {
            anyhow::bail!("--limit must be between 1 and 100 (Docker Hub's cap), got {limit}");
        }
        builder = builder.limit(limit as i32);
    }
    let results = plugin.docker()?.search_images(builder.build()).await?;

    let rows = results
        .iter()
        .map(|r| match fmt {
            OutputFormat::Full => full_value(r, span),
            _ => compact(r, span),
        })
        .collect();
    Ok(Value::list(rows, span))
}

fn compact(r: &ImageSearchResponseItem, span: Span) -> Value {
    let mut rec = Record::new();
    rec.push("name", str_opt(r.name.as_deref(), span));
    rec.push("description", str_opt(r.description.as_deref(), span));
    rec.push("stars", opt_int(r.star_count, span));
    rec.push("official", opt_bool(r.is_official, span));
    Value::record(rec, span)
}
