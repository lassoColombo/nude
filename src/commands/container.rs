use bollard::models::{ContainerInspectResponse, ContainerSummary};
use bollard::query_parameters::ListContainersOptionsBuilder;
use futures::future::join_all;
use nu_plugin::{EngineInterface, EvaluatedCall, SimplePluginCommand};
use nu_protocol::{DynamicSuggestion, LabeledError, Record, Signature, Span, Value};

use crate::completers::{self, dynamic_completion};
use crate::helpers::{
    clean_name, enum_opt, full_value, opt_bool, opt_epoch, opt_int, opt_rfc3339, opt_short_id,
    push_labels, skip_vanished, str_list, str_opt,
};
use crate::output::OutputFormat;
use crate::plugin::NudePlugin;
use crate::scaffold::{self, CommandSpec, FilterArg};

pub struct ContainerLsCommand;
pub struct ContainerInspectCommand;

const LS: &str = "nude container ls";
const INSPECT: &str = "nude container inspect";

const FILTERS: &[FilterArg] = &[
    FilterArg::string(
        "status",
        "State: created|restarting|running|removing|paused|exited|dead",
    ),
    FilterArg::string("health", "Health: starting|healthy|unhealthy|none"),
    FilterArg::int("exited", "Exit code (matches stopped containers)"),
    FilterArg::string(
        "ancestor",
        "Created from this image (name[:tag], id, or digest)",
    ),
    FilterArg::string("before", "Created before this container (name or id)"),
    FilterArg::string("since", "Created since this container (name or id)"),
    FilterArg::string("name", "Name substring (use `inspect` for an exact lookup)"),
    FilterArg::string("id", "Container id prefix"),
    FilterArg::string("network", "Attached to this network (name or id)"),
    FilterArg::string("volume", "Uses this volume (name or mount destination)"),
    FilterArg::string(
        "publish",
        "Publishes this port (`port[/proto]` or `start-end[/proto]`)",
    ),
    FilterArg::string(
        "expose",
        "Exposes this port (`port[/proto]` or `start-end[/proto]`)",
    ),
    FilterArg::labels(),
    FilterArg::switch("is-task", "Only swarm service tasks"),
];

impl SimplePluginCommand for ContainerLsCommand {
    type Plugin = NudePlugin;

    fn name(&self) -> &str {
        LS
    }

    fn description(&self) -> &str {
        "List containers"
    }

    fn signature(&self) -> Signature {
        scaffold::list_signature(&CommandSpec {
            cmd: LS,
            noun: "container",
            all_help: Some("Include stopped containers"),
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
            "status" => Some(completers::from_pairs(STATUS)),
            "health" => Some(completers::from_pairs(HEALTH)),
            // Container references → live names/ids.
            "name" | "before" | "since" => complete_names(plugin),
            "id" => complete_ids(plugin),
            // Cross-references → the owning resource's completer.
            "ancestor" => crate::commands::image::complete_refs(plugin),
            "network" => crate::commands::network::complete_names(plugin),
            "volume" => crate::commands::volume::complete_names(plugin),
            "labels" => completers::complete_labels(
                &call,
                all_containers(plugin)?
                    .iter()
                    .filter_map(|c| c.labels.as_ref()),
            ),
            _ => None,
        })
    );
}

impl SimplePluginCommand for ContainerInspectCommand {
    type Plugin = NudePlugin;

    fn name(&self) -> &str {
        INSPECT
    }

    fn description(&self) -> &str {
        "Inspect a container by name/ID"
    }

    fn signature(&self) -> Signature {
        scaffold::inspect_signature(INSPECT, "Container name or ID", Some("container"))
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

const STATUS: &[(&str, &str)] = &[
    ("created", "Created but not started"),
    ("restarting", "Restarting"),
    ("running", "Running"),
    ("removing", "Being removed"),
    ("paused", "Paused"),
    ("exited", "Stopped after running"),
    ("dead", "Dead - the daemon could not remove it"),
];

const HEALTH: &[(&str, &str)] = &[
    ("starting", "Healthcheck grace period"),
    ("healthy", "Passing its healthcheck"),
    ("unhealthy", "Failing its healthcheck"),
    ("none", "No healthcheck configured"),
];

fn all_containers(plugin: &NudePlugin) -> Option<Vec<ContainerSummary>> {
    let docker = plugin.docker().ok()?;
    let opts = ListContainersOptionsBuilder::default().all(true).build();
    plugin.rt.block_on(docker.list_containers(Some(opts))).ok()
}

pub(crate) fn complete_names(plugin: &NudePlugin) -> Option<Vec<DynamicSuggestion>> {
    Some(
        all_containers(plugin)?
            .iter()
            .filter_map(|c| {
                Some(DynamicSuggestion {
                    value: clean_name(c.names.as_ref())?,
                    description: c.status.clone(),
                    ..Default::default()
                })
            })
            .collect(),
    )
}

fn complete_ids(plugin: &NudePlugin) -> Option<Vec<DynamicSuggestion>> {
    Some(
        all_containers(plugin)?
            .iter()
            .filter_map(|c| {
                Some(DynamicSuggestion {
                    value: c.id.clone()?,
                    description: clean_name(c.names.as_ref()),
                    ..Default::default()
                })
            })
            .collect(),
    )
}

async fn run_list(plugin: &NudePlugin, call: &EvaluatedCall) -> anyhow::Result<Value> {
    let all: bool = call.has_flag("all")?;
    let show_labels = call.has_flag("show-labels")?;
    let span = call.head;
    let fmt = scaffold::list_format(call)?;

    let docker = plugin.docker()?;
    let filters = scaffold::collect_filters(call, FILTERS)?;
    let opts = ListContainersOptionsBuilder::default()
        .all(all)
        .filters(&filters)
        .build();
    let summaries = docker.list_containers(Some(opts)).await?;
    if fmt == OutputFormat::Compact {
        let rows = summaries
            .iter()
            .map(|c| compact(c, span, show_labels))
            .collect();
        return Ok(Value::list(rows, span));
    }

    let ids: Vec<String> = summaries.into_iter().filter_map(|c| c.id).collect();
    let inspected =
        skip_vanished(join_all(ids.iter().map(|id| docker.inspect_container(id, None))).await)?;
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

    let resp = plugin.docker()?.inspect_container(&name, None).await?;
    Ok(match fmt {
        OutputFormat::Full => full_value(&resp, span),
        _ => wide(&resp, span, show_labels),
    })
}

fn compact(c: &ContainerSummary, span: Span, show_labels: bool) -> Value {
    let status = c.status.as_deref();
    let mut rec = Record::new();
    rec.push(
        "name",
        str_opt(clean_name(c.names.as_ref()).as_deref(), span),
    );
    rec.push("image", str_opt(c.image.as_deref(), span));
    rec.push("state", enum_opt(c.state.as_ref(), span));
    rec.push("health", str_opt(status_health(status), span));
    rec.push("exit_code", opt_int(status_exit_code(status), span));
    rec.push("created", opt_epoch(c.created, span));
    push_labels(&mut rec, show_labels, c.labels.as_ref(), span);
    Value::record(rec, span)
}

fn status_health(status: Option<&str>) -> Option<&'static str> {
    let s = status?;
    if s.contains("(healthy)") {
        Some("healthy")
    } else if s.contains("(unhealthy)") {
        Some("unhealthy")
    } else if s.contains("(health: starting)") {
        Some("starting")
    } else {
        None
    }
}

fn status_exit_code(status: Option<&str>) -> Option<i64> {
    let s = status?;
    let rest = s
        .strip_prefix("Exited (")
        .or_else(|| s.strip_prefix("Restarting ("))?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

fn wide(r: &ContainerInspectResponse, span: Span, show_labels: bool) -> Value {
    let config = r.config.as_ref();
    let state = r.state.as_ref();

    let mut rec = Record::new();
    rec.push(
        "name",
        str_opt(r.name.as_deref().map(|n| n.trim_start_matches('/')), span),
    );
    rec.push("id", opt_short_id(r.id.as_deref(), span));
    rec.push(
        "image",
        str_opt(config.and_then(|c| c.image.as_deref()), span),
    );
    rec.push(
        "state",
        enum_opt(state.and_then(|s| s.status.as_ref()), span),
    );
    rec.push(
        "health",
        enum_opt(
            state
                .and_then(|s| s.health.as_ref())
                .and_then(|h| h.status.as_ref()),
            span,
        ),
    );
    let running = state.and_then(|s| s.running).unwrap_or(false);
    rec.push(
        "exit_code",
        opt_int(
            (!running)
                .then(|| state.and_then(|s| s.exit_code))
                .flatten(),
            span,
        ),
    );
    rec.push(
        "command",
        str_list(config.and_then(|c| c.cmd.as_ref()), span),
    );
    rec.push("created", opt_rfc3339(r.created.as_deref(), span));
    rec.push(
        "started",
        opt_rfc3339(state.and_then(|s| s.started_at.as_deref()), span),
    );
    rec.push(
        "finished",
        opt_rfc3339(state.and_then(|s| s.finished_at.as_deref()), span),
    );
    rec.push("restarts", opt_int(r.restart_count, span));
    rec.push("ports", ports_value(r, span));
    let networks = sorted_networks(r);
    rec.push("ip", ip_value(&networks, span));
    rec.push("networks", networks_value(&networks, span));
    rec.push("mounts", mounts_value(r, span));
    push_labels(
        &mut rec,
        show_labels,
        config.and_then(|c| c.labels.as_ref()),
        span,
    );
    Value::record(rec, span)
}

fn port_int(s: &str, span: Span) -> Value {
    match s.parse::<i64>() {
        Ok(n) => Value::int(n, span),
        Err(_) => Value::string(s, span),
    }
}

/// One row per (container port, host binding), ordered by port number then protocol so the
/// table is stable across runs (the daemon hands us a map).
fn ports_value(r: &ContainerInspectResponse, span: Span) -> Value {
    let Some(ports) = r.network_settings.as_ref().and_then(|ns| ns.ports.as_ref()) else {
        return Value::list(vec![], span);
    };
    let mut keys: Vec<&String> = ports.keys().collect();
    keys.sort_by_key(|k| {
        let (port, proto) = k.split_once('/').unwrap_or((k.as_str(), ""));
        (port.parse::<u32>().unwrap_or(u32::MAX), proto.to_string())
    });

    let mut rows = Vec::new();
    for key in keys {
        let (port, proto) = key.split_once('/').unwrap_or((key.as_str(), ""));
        let row = |host_ip: Option<&str>, host_port: Option<&str>| {
            let mut rec = Record::new();
            rec.push("container_port", port_int(port, span));
            rec.push("proto", Value::string(proto, span));
            rec.push("host_ip", str_opt(host_ip, span));
            rec.push(
                "host_port",
                match host_port {
                    Some(p) if !p.is_empty() => port_int(p, span),
                    _ => Value::nothing(span),
                },
            );
            Value::record(rec, span)
        };
        match &ports[key] {
            Some(bs) if !bs.is_empty() => rows.extend(
                bs.iter()
                    .map(|b| row(b.host_ip.as_deref(), b.host_port.as_deref())),
            ),
            _ => rows.push(row(None, None)),
        }
    }
    Value::list(rows, span)
}

/// `(network name, ip)` pairs in name order: the daemon's map has no stable order, and `ip`
/// below picks the first, so without sorting a multi-homed container's ip could change per call.
fn sorted_networks(r: &ContainerInspectResponse) -> Vec<(&str, Option<&str>)> {
    let mut nets: Vec<(&str, Option<&str>)> = r
        .network_settings
        .as_ref()
        .and_then(|ns| ns.networks.as_ref())
        .map(|nets| {
            nets.iter()
                .map(|(name, e)| (name.as_str(), e.ip_address.as_deref()))
                .collect()
        })
        .unwrap_or_default();
    nets.sort_unstable_by_key(|(name, _)| *name);
    nets
}

fn ip_value(networks: &[(&str, Option<&str>)], span: Span) -> Value {
    let ip = networks
        .iter()
        .filter_map(|(_, ip)| *ip)
        .find(|s| !s.is_empty());
    str_opt(ip, span)
}

fn networks_value(networks: &[(&str, Option<&str>)], span: Span) -> Value {
    let names = networks
        .iter()
        .map(|(name, _)| Value::string(*name, span))
        .collect();
    Value::list(names, span)
}

fn mounts_value(r: &ContainerInspectResponse, span: Span) -> Value {
    let Some(mounts) = r.mounts.as_ref() else {
        return Value::list(vec![], span);
    };
    let rows = mounts
        .iter()
        .map(|m| {
            let mut rec = Record::new();
            rec.push("type", enum_opt(m.typ.as_ref(), span));
            rec.push("source", str_opt(m.source.as_deref(), span));
            rec.push("destination", str_opt(m.destination.as_deref(), span));
            rec.push("mode", str_opt(m.mode.as_deref(), span));
            rec.push("rw", opt_bool(m.rw, span));
            Value::record(rec, span)
        })
        .collect();
    Value::list(rows, span)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bollard::models::{EndpointSettings, NetworkSettings, PortBinding};
    use std::collections::HashMap;

    #[test]
    fn health_is_parsed_from_the_status_line() {
        assert_eq!(
            status_health(Some("Up 12 hours (healthy)")),
            Some("healthy")
        );
        assert_eq!(
            status_health(Some("Up 2 minutes (unhealthy)")),
            Some("unhealthy")
        );
        assert_eq!(
            status_health(Some("Up Less than a second (health: starting)")),
            Some("starting")
        );
        assert_eq!(status_health(Some("Up 12 hours")), None);
        assert_eq!(status_health(Some("Exited (0) 47 hours ago")), None);
        assert_eq!(status_health(None), None);
    }

    #[test]
    fn exit_code_is_parsed_from_the_status_line() {
        assert_eq!(status_exit_code(Some("Exited (0) 47 hours ago")), Some(0));
        assert_eq!(status_exit_code(Some("Exited (137) 3 days ago")), Some(137));
        assert_eq!(
            status_exit_code(Some("Restarting (1) 5 seconds ago")),
            Some(1)
        );
        assert_eq!(status_exit_code(Some("Up 12 hours (healthy)")), None);
        assert_eq!(status_exit_code(Some("Created")), None);
        assert_eq!(status_exit_code(None), None);
    }

    fn with_networks(nets: &[(&str, &str)]) -> ContainerInspectResponse {
        let networks: HashMap<String, EndpointSettings> = nets
            .iter()
            .map(|(name, ip)| {
                (
                    name.to_string(),
                    EndpointSettings {
                        ip_address: Some(ip.to_string()),
                        ..Default::default()
                    },
                )
            })
            .collect();
        ContainerInspectResponse {
            network_settings: Some(NetworkSettings {
                networks: Some(networks),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn ip_and_networks_follow_network_name_order() {
        let span = Span::test_data();
        let r = with_networks(&[("zeta", "10.0.0.9"), ("alpha", ""), ("beta", "10.0.0.2")]);
        let nets = sorted_networks(&r);
        assert_eq!(ip_value(&nets, span).as_str().unwrap(), "10.0.0.2");
        let names: Vec<String> = networks_value(&nets, span)
            .into_list()
            .unwrap()
            .into_iter()
            .map(|v| v.into_string().unwrap())
            .collect();
        assert_eq!(names, ["alpha", "beta", "zeta"]);
    }

    #[test]
    fn ports_are_ordered_numerically_and_unbound_ports_get_one_row() {
        let span = Span::test_data();
        let mut ports = HashMap::new();
        ports.insert(
            "443/tcp".to_string(),
            Some(vec![PortBinding {
                host_ip: Some("0.0.0.0".into()),
                host_port: Some("8443".into()),
            }]),
        );
        ports.insert("80/tcp".to_string(), None);
        ports.insert("53/udp".to_string(), Some(vec![]));
        let r = ContainerInspectResponse {
            network_settings: Some(NetworkSettings {
                ports: Some(ports),
                ..Default::default()
            }),
            ..Default::default()
        };
        let rows = ports_value(&r, span).into_list().unwrap();
        let col = |i: usize, c: &str| rows[i].as_record().unwrap().get(c).unwrap().clone();
        assert_eq!(rows.len(), 3);
        assert_eq!(col(0, "container_port"), Value::int(53, span));
        assert_eq!(col(1, "container_port"), Value::int(80, span));
        assert_eq!(col(1, "host_port"), Value::nothing(span));
        assert_eq!(col(2, "container_port"), Value::int(443, span));
        assert_eq!(col(2, "host_port"), Value::int(8443, span));
    }
}
