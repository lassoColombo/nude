use bollard::models::Plugin;
use bollard::query_parameters::ListPluginsOptionsBuilder;
use nu_plugin::{EngineInterface, EvaluatedCall, SimplePluginCommand};
use nu_protocol::{DynamicSuggestion, LabeledError, Record, Signature, Span, Value};

use crate::completers::{self, dynamic_completion};
use crate::helpers::{full_value, opt_short_id, str_list, str_opt};
use crate::output::OutputFormat;
use crate::plugin::NudePlugin;
use crate::scaffold::{self, CommandSpec, FilterArg};

pub struct PluginLsCommand;
pub struct PluginInspectCommand;

const LS: &str = "nude plugin ls";
const INSPECT: &str = "nude plugin inspect";

const FILTERS: &[FilterArg] = &[
    FilterArg::string(
        "capability",
        "Capability: volumedriver|networkdriver|ipamdriver|authz|logdriver|metricscollector",
    ),
    FilterArg::switch("enabled", "Only enabled plugins"),
];

impl SimplePluginCommand for PluginLsCommand {
    type Plugin = NudePlugin;

    fn name(&self) -> &str {
        LS
    }

    fn description(&self) -> &str {
        "List managed plugins"
    }

    fn signature(&self) -> Signature {
        scaffold::list_signature(&CommandSpec {
            cmd: LS,
            noun: "plugin",
            all_help: None,
            show_labels: false,
            filters: FILTERS,
        })
    }

    fn run(
        &self,
        plugin: &Self::Plugin,
        _engine: &EngineInterface,
        call: &EvaluatedCall,
        _input: &Value,
    ) -> Result<Value, LabeledError> {
        plugin.block_on_labeled(run_list(plugin, call))
    }

    dynamic_completion!(
        |plugin, call, arg_type| completers::flags(arg_type, |flag| match flag {
            "capability" => Some(completers::from_pairs(CAPABILITIES)),
            _ => None,
        })
    );
}

impl SimplePluginCommand for PluginInspectCommand {
    type Plugin = NudePlugin;

    fn name(&self) -> &str {
        INSPECT
    }

    fn description(&self) -> &str {
        "Inspect a managed plugin by name"
    }

    fn signature(&self) -> Signature {
        scaffold::inspect_signature(INSPECT, "Plugin name", None)
    }

    fn run(
        &self,
        plugin: &Self::Plugin,
        _engine: &EngineInterface,
        call: &EvaluatedCall,
        _input: &Value,
    ) -> Result<Value, LabeledError> {
        plugin.block_on_labeled(run_inspect(plugin, call))
    }

    dynamic_completion!(|plugin, call, arg_type| completers::ref_and_output(
        plugin,
        arg_type,
        complete_names
    ));
}

const CAPABILITIES: &[(&str, &str)] = &[
    ("volumedriver", "Provides a volume driver"),
    ("networkdriver", "Provides a network driver"),
    ("ipamdriver", "Provides an IPAM driver"),
    ("authz", "Authorization plugin"),
    ("logdriver", "Provides a logging driver"),
    ("metricscollector", "Exposes metrics to collect"),
];

fn all_plugins(plugin: &NudePlugin) -> Option<Vec<Plugin>> {
    let docker = plugin.docker().ok()?;
    let opts = ListPluginsOptionsBuilder::new().build();
    plugin.rt.block_on(docker.list_plugins(Some(opts))).ok()
}

fn complete_names(plugin: &NudePlugin) -> Option<Vec<DynamicSuggestion>> {
    Some(
        all_plugins(plugin)?
            .iter()
            .map(|p| DynamicSuggestion {
                value: p.name.clone(),
                description: Some(p.config.description.clone()),
                ..Default::default()
            })
            .collect(),
    )
}

async fn run_list(plugin: &NudePlugin, call: &EvaluatedCall) -> anyhow::Result<Value> {
    let span = call.head;
    let fmt = scaffold::list_format(call)?;

    let filters = scaffold::collect_filters(call, FILTERS)?;
    let opts = ListPluginsOptionsBuilder::new().filters(&filters).build();
    let plugins = plugin.docker()?.list_plugins(Some(opts)).await?;
    let rows = plugins
        .iter()
        .map(|p| match fmt {
            OutputFormat::Compact => compact(p, span),
            OutputFormat::Wide => wide(p, span),
            OutputFormat::Full => full_value(p, span),
        })
        .collect();
    Ok(Value::list(rows, span))
}

async fn run_inspect(plugin: &NudePlugin, call: &EvaluatedCall) -> anyhow::Result<Value> {
    let name: String = call.req(0)?;
    let span = call.head;
    let fmt = scaffold::inspect_format(call)?;

    let p = plugin.docker()?.inspect_plugin(&name).await?;
    Ok(match fmt {
        OutputFormat::Full => full_value(&p, span),
        _ => wide(&p, span),
    })
}

fn compact(p: &Plugin, span: Span) -> Value {
    let mut rec = Record::new();
    rec.push("name", str_opt(p.name.as_str(), span));
    rec.push("id", opt_short_id(p.id.as_deref(), span));
    rec.push("enabled", Value::bool(p.enabled, span));
    rec.push("reference", str_opt(p.plugin_reference.as_deref(), span));
    rec.push("description", str_opt(p.config.description.as_str(), span));
    Value::record(rec, span)
}

fn wide(p: &Plugin, span: Span) -> Value {
    let c = &p.config;
    let mut rec = Record::new();
    rec.push("name", str_opt(p.name.as_str(), span));
    rec.push("id", opt_short_id(p.id.as_deref(), span));
    rec.push("enabled", Value::bool(p.enabled, span));
    rec.push("reference", str_opt(p.plugin_reference.as_deref(), span));
    rec.push("description", str_opt(c.description.as_str(), span));
    rec.push("documentation", str_opt(c.documentation.as_str(), span));
    rec.push("capabilities", str_list(Some(&c.interface.types), span));
    rec.push("socket", str_opt(c.interface.socket.as_str(), span));
    rec.push("entrypoint", str_list(Some(&c.entrypoint), span));
    rec.push("work_dir", str_opt(c.work_dir.as_str(), span));
    rec.push("env", str_list(Some(&p.settings.env), span));
    rec.push("args", str_list(Some(&p.settings.args), span));
    Value::record(rec, span)
}
