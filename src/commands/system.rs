use bollard::models::{SystemInfo, SystemVersion};
use nu_plugin::{EngineInterface, EvaluatedCall, SimplePluginCommand};
use nu_protocol::{LabeledError, Record, Signature, Span, Value};

use crate::completers::{self, dynamic_completion};
use crate::helpers::{
    enum_opt, full_value, opt_bool, opt_filesize, opt_int, opt_rfc3339, str_list, str_opt,
};
use crate::output::OutputFormat;
use crate::plugin::NudePlugin;
use crate::scaffold;

pub struct SystemInfoCommand;

const INFO_NAME: &str = "nude system info";

impl SimplePluginCommand for SystemInfoCommand {
    type Plugin = NudePlugin;

    fn name(&self) -> &str {
        INFO_NAME
    }

    fn description(&self) -> &str {
        "Daemon-wide info: versions, counts, driver, host resources"
    }

    fn signature(&self) -> Signature {
        scaffold::singleton_signature(INFO_NAME)
    }

    fn run(
        &self,
        plugin: &Self::Plugin,
        _engine: &EngineInterface,
        call: &EvaluatedCall,
        _input: &Value,
    ) -> Result<Value, LabeledError> {
        plugin.block_on_labeled(run_info(plugin, call))
    }

    dynamic_completion!(|plugin, call, arg_type| completers::flags(arg_type, |_| None));
}

async fn run_info(plugin: &NudePlugin, call: &EvaluatedCall) -> anyhow::Result<Value> {
    let span = call.head;
    let fmt = scaffold::list_format(call)?;
    let info = plugin.docker()?.info().await?;
    Ok(match fmt {
        OutputFormat::Full => full_value(&info, span),
        OutputFormat::Compact => info_record(&info, span, false),
        OutputFormat::Wide => info_record(&info, span, true),
    })
}

fn info_record(i: &SystemInfo, span: Span, wide: bool) -> Value {
    let mut rec = Record::new();
    rec.push("name", str_opt(i.name.as_deref(), span));
    rec.push("server_version", str_opt(i.server_version.as_deref(), span));
    rec.push("containers", opt_int(i.containers, span));
    rec.push("containers_running", opt_int(i.containers_running, span));
    rec.push("containers_paused", opt_int(i.containers_paused, span));
    rec.push("containers_stopped", opt_int(i.containers_stopped, span));
    rec.push("images", opt_int(i.images, span));
    rec.push("driver", str_opt(i.driver.as_deref(), span));
    rec.push(
        "operating_system",
        str_opt(i.operating_system.as_deref(), span),
    );
    rec.push("os_type", str_opt(i.os_type.as_deref(), span));
    rec.push("architecture", str_opt(i.architecture.as_deref(), span));
    rec.push("kernel_version", str_opt(i.kernel_version.as_deref(), span));
    rec.push("ncpu", opt_int(i.ncpu, span));
    rec.push("mem_total", opt_filesize(i.mem_total, span));
    if wide {
        rec.push(
            "docker_root_dir",
            str_opt(i.docker_root_dir.as_deref(), span),
        );
        rec.push("logging_driver", str_opt(i.logging_driver.as_deref(), span));
        rec.push("cgroup_driver", enum_opt(i.cgroup_driver.as_ref(), span));
        rec.push("cgroup_version", enum_opt(i.cgroup_version.as_ref(), span));
        rec.push(
            "default_runtime",
            str_opt(i.default_runtime.as_deref(), span),
        );
        rec.push("live_restore", opt_bool(i.live_restore_enabled, span));
        rec.push("debug", opt_bool(i.debug, span));
        rec.push("experimental", opt_bool(i.experimental_build, span));
        rec.push(
            "index_server",
            str_opt(i.index_server_address.as_deref(), span),
        );
        rec.push(
            "security_options",
            str_list(i.security_options.as_ref(), span),
        );
        rec.push("warnings", str_list(i.warnings.as_ref(), span));
        rec.push("labels", str_list(i.labels.as_ref(), span));
    }
    Value::record(rec, span)
}

pub struct VersionCommand;

const VERSION_NAME: &str = "nude version";

impl SimplePluginCommand for VersionCommand {
    type Plugin = NudePlugin;

    fn name(&self) -> &str {
        VERSION_NAME
    }

    fn description(&self) -> &str {
        "Daemon version: engine, API, Go, OS/arch, and components"
    }

    fn signature(&self) -> Signature {
        scaffold::singleton_signature(VERSION_NAME)
    }

    fn run(
        &self,
        plugin: &Self::Plugin,
        _engine: &EngineInterface,
        call: &EvaluatedCall,
        _input: &Value,
    ) -> Result<Value, LabeledError> {
        plugin.block_on_labeled(run_version(plugin, call))
    }

    dynamic_completion!(|plugin, call, arg_type| completers::flags(arg_type, |_| None));
}

async fn run_version(plugin: &NudePlugin, call: &EvaluatedCall) -> anyhow::Result<Value> {
    let span = call.head;
    let fmt = scaffold::list_format(call)?;
    let v = plugin.docker()?.version().await?;
    Ok(match fmt {
        OutputFormat::Full => full_value(&v, span),
        OutputFormat::Compact => version_record(&v, span, false),
        OutputFormat::Wide => version_record(&v, span, true),
    })
}

fn version_record(v: &SystemVersion, span: Span, wide: bool) -> Value {
    let mut rec = Record::new();
    rec.push("version", str_opt(v.version.as_deref(), span));
    rec.push("api_version", str_opt(v.api_version.as_deref(), span));
    if wide {
        rec.push(
            "min_api_version",
            str_opt(v.min_api_version.as_deref(), span),
        );
    }
    rec.push("os", str_opt(v.os.as_deref(), span));
    rec.push("arch", str_opt(v.arch.as_deref(), span));
    rec.push("kernel_version", str_opt(v.kernel_version.as_deref(), span));
    rec.push("go_version", str_opt(v.go_version.as_deref(), span));
    rec.push("git_commit", str_opt(v.git_commit.as_deref(), span));
    rec.push("build_time", opt_rfc3339(v.build_time.as_deref(), span));
    rec.push("experimental", opt_bool(v.experimental, span));
    if wide {
        rec.push("components", components_value(v, span));
    }
    Value::record(rec, span)
}

fn components_value(v: &SystemVersion, span: Span) -> Value {
    let rows = v
        .components
        .as_ref()
        .map(|cs| {
            cs.iter()
                .map(|c| {
                    let mut r = Record::new();
                    r.push("name", Value::string(&c.name, span));
                    r.push("version", Value::string(&c.version, span));
                    Value::record(r, span)
                })
                .collect()
        })
        .unwrap_or_default();
    Value::list(rows, span)
}
