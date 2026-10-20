use bollard::models::{Network, NetworkInspect};
use bollard::query_parameters::ListNetworksOptionsBuilder;
use futures::future::join_all;
use nu_plugin::{EngineInterface, EvaluatedCall, SimplePluginCommand};
use nu_protocol::{DynamicSuggestion, LabeledError, Record, Signature, Span, Value};

use crate::completers::{self, dynamic_completion};
use crate::helpers::{
    full_value, opt_bool, opt_rfc3339, opt_short_id, push_labels, skip_vanished, str_map, str_opt,
};
use crate::output::OutputFormat;
use crate::plugin::NudePlugin;
use crate::scaffold::{self, CommandSpec, FilterArg};

pub struct NetworkLsCommand;
pub struct NetworkInspectCommand;

const LS: &str = "nude network ls";
const INSPECT: &str = "nude network inspect";

const FILTERS: &[FilterArg] = &[
    FilterArg::string(
        "driver",
        "Network driver: bridge|host|overlay|macvlan|ipvlan|none",
    ),
    FilterArg::string("id", "Network id prefix"),
    FilterArg::string("name", "Name substring (use `inspect` for an exact lookup)"),
    FilterArg::string("scope", "Scope: local|global|swarm"),
    FilterArg::string("type", "Type: builtin|custom"),
    FilterArg::switch("dangling", "Only networks not used by any container"),
    FilterArg::labels(),
];

impl SimplePluginCommand for NetworkLsCommand {
    type Plugin = NudePlugin;

    fn name(&self) -> &str {
        LS
    }

    fn description(&self) -> &str {
        "List networks"
    }

    fn signature(&self) -> Signature {
        scaffold::list_signature(&CommandSpec {
            cmd: LS,
            noun: "network",
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
            // Closed enums → static lists.
            "driver" => Some(completers::from_pairs(DRIVERS)),
            "scope" => Some(completers::from_pairs(SCOPES)),
            "type" => Some(completers::from_pairs(TYPES)),
            // Network references → live names/ids.
            "name" => complete_names(plugin),
            "id" => complete_ids(plugin),
            "labels" => completers::complete_labels(
                &call,
                all_networks(plugin)?
                    .iter()
                    .filter_map(|n| n.labels.as_ref()),
            ),
            // dangling has no useful completion.
            _ => None,
        })
    );
}

impl SimplePluginCommand for NetworkInspectCommand {
    type Plugin = NudePlugin;

    fn name(&self) -> &str {
        INSPECT
    }

    fn description(&self) -> &str {
        "Inspect a network by name/ID"
    }

    fn signature(&self) -> Signature {
        scaffold::inspect_signature(INSPECT, "Network name or ID", Some("network"))
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

const DRIVERS: &[(&str, &str)] = &[
    ("bridge", "Single-host bridge (the default)"),
    ("host", "The host's own network stack"),
    ("overlay", "Multi-host swarm overlay"),
    ("macvlan", "MAC-addressed virtual interfaces"),
    ("ipvlan", "IP-addressed virtual interfaces"),
    ("none", "No networking"),
];

const SCOPES: &[(&str, &str)] = &[
    ("local", "Confined to one host"),
    ("global", "Across all swarm nodes"),
    ("swarm", "Swarm-scoped"),
];

const TYPES: &[(&str, &str)] = &[
    ("builtin", "Docker's predefined networks"),
    ("custom", "User-created networks"),
];

fn all_networks(plugin: &NudePlugin) -> Option<Vec<Network>> {
    let docker = plugin.docker().ok()?;
    let opts = ListNetworksOptionsBuilder::default().build();
    plugin.rt.block_on(docker.list_networks(Some(opts))).ok()
}

pub(crate) fn complete_names(plugin: &NudePlugin) -> Option<Vec<DynamicSuggestion>> {
    Some(
        all_networks(plugin)?
            .iter()
            .filter_map(|n| {
                Some(DynamicSuggestion {
                    value: n.name.clone()?,
                    description: n.driver.clone(),
                    ..Default::default()
                })
            })
            .collect(),
    )
}

fn complete_ids(plugin: &NudePlugin) -> Option<Vec<DynamicSuggestion>> {
    Some(
        all_networks(plugin)?
            .iter()
            .filter_map(|n| {
                Some(DynamicSuggestion {
                    value: n.id.clone()?,
                    description: n.name.clone(),
                    ..Default::default()
                })
            })
            .collect(),
    )
}

async fn run_list(plugin: &NudePlugin, call: &EvaluatedCall) -> anyhow::Result<Value> {
    let show_labels = call.has_flag("show-labels")?;
    let span = call.head;
    let fmt = scaffold::list_format(call)?;

    let docker = plugin.docker()?;
    let filters = scaffold::collect_filters(call, FILTERS)?;
    let opts = ListNetworksOptionsBuilder::default()
        .filters(&filters)
        .build();
    let networks = docker.list_networks(Some(opts)).await?;
    if fmt == OutputFormat::Compact {
        let rows = networks
            .iter()
            .map(|n| compact(n, span, show_labels))
            .collect();
        return Ok(Value::list(rows, span));
    }

    let ids: Vec<String> = networks.into_iter().filter_map(|n| n.id).collect();
    let inspected =
        skip_vanished(join_all(ids.iter().map(|id| docker.inspect_network(id, None))).await)?;
    let rows = inspected
        .iter()
        .map(|r| match fmt {
            OutputFormat::Full => full_value(r, span),
            _ => wide(r, span, show_labels),
        })
        .collect();
    Ok(Value::list(rows, span))
}

async fn run_inspect(plugin: &NudePlugin, call: &EvaluatedCall) -> anyhow::Result<Value> {
    let name: String = call.req(0)?;
    let show_labels = call.has_flag("show-labels")?;
    let span = call.head;
    let fmt = scaffold::inspect_format(call)?;

    let resp = plugin.docker()?.inspect_network(&name, None).await?;
    Ok(match fmt {
        OutputFormat::Full => full_value(&resp, span),
        _ => wide(&resp, span, show_labels),
    })
}

fn compact(n: &Network, span: Span, show_labels: bool) -> Value {
    let mut rec = Record::new();
    rec.push("name", str_opt(n.name.as_deref(), span));
    rec.push("id", opt_short_id(n.id.as_deref(), span));
    rec.push("driver", str_opt(n.driver.as_deref(), span));
    rec.push("scope", str_opt(n.scope.as_deref(), span));
    rec.push("internal", opt_bool(n.internal, span));
    rec.push("created", opt_rfc3339(n.created.as_deref(), span));
    push_labels(&mut rec, show_labels, n.labels.as_ref(), span);
    Value::record(rec, span)
}

fn wide(n: &NetworkInspect, span: Span, show_labels: bool) -> Value {
    let mut rec = Record::new();
    rec.push("name", str_opt(n.name.as_deref(), span));
    rec.push("id", opt_short_id(n.id.as_deref(), span));
    rec.push("driver", str_opt(n.driver.as_deref(), span));
    rec.push("scope", str_opt(n.scope.as_deref(), span));
    rec.push("internal", opt_bool(n.internal, span));
    rec.push("attachable", opt_bool(n.attachable, span));
    rec.push("ingress", opt_bool(n.ingress, span));
    rec.push("created", opt_rfc3339(n.created.as_deref(), span));
    rec.push("ipam", ipam_value(n, span));
    rec.push("options", str_map(n.options.as_ref(), span));
    rec.push("containers", containers_value(n, span));
    push_labels(&mut rec, show_labels, n.labels.as_ref(), span);
    Value::record(rec, span)
}

fn ipam_value(n: &NetworkInspect, span: Span) -> Value {
    let Some(ipam) = n.ipam.as_ref() else {
        return Value::nothing(span);
    };
    let configs = ipam
        .config
        .as_ref()
        .map(|cs| {
            cs.iter()
                .map(|c| {
                    let mut r = Record::new();
                    r.push("subnet", str_opt(c.subnet.as_deref(), span));
                    r.push("gateway", str_opt(c.gateway.as_deref(), span));
                    r.push("ip_range", str_opt(c.ip_range.as_deref(), span));
                    Value::record(r, span)
                })
                .collect()
        })
        .unwrap_or_default();
    let mut rec = Record::new();
    rec.push("driver", str_opt(ipam.driver.as_deref(), span));
    rec.push("config", Value::list(configs, span));
    Value::record(rec, span)
}

/// Connected containers in name order (the daemon keys them by id in a map).
fn containers_value(n: &NetworkInspect, span: Span) -> Value {
    let Some(map) = n.containers.as_ref() else {
        return Value::list(vec![], span);
    };
    let mut entries: Vec<_> = map.values().collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    let rows = entries
        .into_iter()
        .map(|e| {
            let mut r = Record::new();
            r.push("name", str_opt(e.name.as_deref(), span));
            r.push("ipv4", str_opt(e.ipv4_address.as_deref(), span));
            r.push("ipv6", str_opt(e.ipv6_address.as_deref(), span));
            r.push("mac", str_opt(e.mac_address.as_deref(), span));
            Value::record(r, span)
        })
        .collect();
    Value::list(rows, span)
}
