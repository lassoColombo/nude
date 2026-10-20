use std::collections::BTreeSet;

use bollard::models::Volume;
use bollard::query_parameters::ListVolumesOptionsBuilder;
use nu_plugin::{EngineInterface, EvaluatedCall, SimplePluginCommand};
use nu_protocol::{DynamicSuggestion, LabeledError, Record, Signature, Span, Value};

use crate::completers::{self, dynamic_completion};
use crate::helpers::{
    enum_opt, full_value, opt_filesize, opt_int, opt_rfc3339, push_labels, str_map, str_opt,
};
use crate::output::OutputFormat;
use crate::plugin::NudePlugin;
use crate::scaffold::{self, CommandSpec, FilterArg};

pub struct VolumeLsCommand;
pub struct VolumeInspectCommand;

const LS: &str = "nude volume ls";
const INSPECT: &str = "nude volume inspect";

const FILTERS: &[FilterArg] = &[
    FilterArg::string("driver", "Volume driver (e.g. `local`, or a volume plugin)"),
    FilterArg::string("name", "Name substring (use `inspect` for an exact lookup)"),
    FilterArg::switch("dangling", "Only volumes not used by any container"),
    FilterArg::labels(),
];

impl SimplePluginCommand for VolumeLsCommand {
    type Plugin = NudePlugin;

    fn name(&self) -> &str {
        LS
    }

    fn description(&self) -> &str {
        "List volumes"
    }

    fn signature(&self) -> Signature {
        scaffold::list_signature(&CommandSpec {
            cmd: LS,
            noun: "volume",
            all_help: None,
            show_labels: true,
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
            "driver" => complete_drivers(plugin),
            "name" => complete_names(plugin),
            "labels" =>
                completers::complete_labels(&call, all_volumes(plugin)?.iter().map(|v| &v.labels),),
            _ => None,
        })
    );
}

impl SimplePluginCommand for VolumeInspectCommand {
    type Plugin = NudePlugin;

    fn name(&self) -> &str {
        INSPECT
    }

    fn description(&self) -> &str {
        "Inspect a volume by name"
    }

    fn signature(&self) -> Signature {
        scaffold::inspect_signature(INSPECT, "Volume name", Some("volume"))
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

fn all_volumes(plugin: &NudePlugin) -> Option<Vec<Volume>> {
    let docker = plugin.docker().ok()?;
    let opts = ListVolumesOptionsBuilder::default().build();
    let resp = plugin.rt.block_on(docker.list_volumes(Some(opts))).ok()?;
    Some(resp.volumes.unwrap_or_default())
}

pub(crate) fn complete_names(plugin: &NudePlugin) -> Option<Vec<DynamicSuggestion>> {
    Some(
        all_volumes(plugin)?
            .iter()
            .map(|v| DynamicSuggestion {
                value: v.name.clone(),
                description: Some(v.driver.clone()),
                ..Default::default()
            })
            .collect(),
    )
}

fn complete_drivers(plugin: &NudePlugin) -> Option<Vec<DynamicSuggestion>> {
    let drivers: BTreeSet<String> = all_volumes(plugin)?.into_iter().map(|v| v.driver).collect();
    Some(
        drivers
            .into_iter()
            .map(|value| DynamicSuggestion {
                value,
                ..Default::default()
            })
            .collect(),
    )
}

async fn run_list(plugin: &NudePlugin, call: &EvaluatedCall) -> anyhow::Result<Value> {
    let show_labels = call.has_flag("show-labels")?;
    let span = call.head;
    let fmt = scaffold::list_format(call)?;

    let filters = scaffold::collect_filters(call, FILTERS)?;
    let opts = ListVolumesOptionsBuilder::default()
        .filters(&filters)
        .build();
    let volumes = plugin
        .docker()?
        .list_volumes(Some(opts))
        .await?
        .volumes
        .unwrap_or_default();
    let rows = volumes
        .iter()
        .map(|v| match fmt {
            OutputFormat::Compact => compact(v, span, show_labels),
            OutputFormat::Wide => wide(v, span, show_labels),
            OutputFormat::Full => full_value(v, span),
        })
        .collect();
    Ok(Value::list(rows, span))
}

async fn run_inspect(plugin: &NudePlugin, call: &EvaluatedCall) -> anyhow::Result<Value> {
    let name: String = call.req(0)?;
    let show_labels = call.has_flag("show-labels")?;
    let span = call.head;
    let fmt = scaffold::inspect_format(call)?;

    let v = plugin.docker()?.inspect_volume(&name).await?;
    Ok(match fmt {
        OutputFormat::Full => full_value(&v, span),
        _ => wide(&v, span, show_labels),
    })
}

fn compact(v: &Volume, span: Span, show_labels: bool) -> Value {
    let mut rec = Record::new();
    rec.push("name", str_opt(v.name.as_str(), span));
    rec.push("driver", str_opt(v.driver.as_str(), span));
    rec.push("scope", enum_opt(v.scope.as_ref(), span));
    rec.push("created", opt_rfc3339(v.created_at.as_deref(), span));
    push_labels(&mut rec, show_labels, Some(&v.labels), span);
    Value::record(rec, span)
}

fn wide(v: &Volume, span: Span, show_labels: bool) -> Value {
    let mut rec = Record::new();
    rec.push("name", str_opt(v.name.as_str(), span));
    rec.push("driver", str_opt(v.driver.as_str(), span));
    rec.push("scope", enum_opt(v.scope.as_ref(), span));
    rec.push("mountpoint", str_opt(v.mountpoint.as_str(), span));
    rec.push("created", opt_rfc3339(v.created_at.as_deref(), span));
    rec.push("options", str_map(Some(&v.options), span));
    rec.push(
        "size",
        opt_filesize(v.usage_data.as_ref().map(|u| u.size), span),
    );
    rec.push(
        "refs",
        opt_int(v.usage_data.as_ref().map(|u| u.ref_count), span),
    );
    push_labels(&mut rec, show_labels, Some(&v.labels), span);
    Value::record(rec, span)
}
