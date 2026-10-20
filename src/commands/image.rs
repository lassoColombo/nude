use bollard::models::{ImageInspect, ImageSummary};
use bollard::query_parameters::ListImagesOptionsBuilder;
use futures::future::join_all;
use nu_plugin::{EngineInterface, EvaluatedCall, SimplePluginCommand};
use nu_protocol::{DynamicSuggestion, LabeledError, Record, Signature, Span, Value};

use crate::completers::{self, dynamic_completion};
use crate::helpers::{
    epoch_date, full_value, opt_filesize, opt_rfc3339, opt_short_id, push_labels, short_id,
    skip_vanished, str_list, str_opt,
};
use crate::output::OutputFormat;
use crate::plugin::NudePlugin;
use crate::scaffold::{self, CommandSpec, FilterArg};

pub struct ImageLsCommand;
pub struct ImageInspectCommand;

const LS: &str = "nude image ls";
const INSPECT: &str = "nude image inspect";

const FILTERS: &[FilterArg] = &[
    FilterArg::string(
        "reference",
        "Reference pattern (name[:tag], e.g. `nginx` or `ngin*`)",
    ),
    FilterArg::string("before", "Created before this image (name or id)"),
    FilterArg::string("since", "Created since this image (name or id)"),
    FilterArg::switch("dangling", "Only dangling (untagged) images"),
    FilterArg::labels(),
];

impl SimplePluginCommand for ImageLsCommand {
    type Plugin = NudePlugin;

    fn name(&self) -> &str {
        LS
    }

    fn description(&self) -> &str {
        "List images"
    }

    fn signature(&self) -> Signature {
        scaffold::list_signature(&CommandSpec {
            cmd: LS,
            noun: "image",
            all_help: Some("Include intermediate (layer) images"),
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
            "reference" | "before" | "since" => complete_refs(plugin),
            "labels" =>
                completers::complete_labels(&call, all_images(plugin)?.iter().map(|i| &i.labels),),
            _ => None,
        })
    );
}

impl SimplePluginCommand for ImageInspectCommand {
    type Plugin = NudePlugin;

    fn name(&self) -> &str {
        INSPECT
    }

    fn description(&self) -> &str {
        "Inspect an image by name/ID"
    }

    fn signature(&self) -> Signature {
        scaffold::inspect_signature(INSPECT, "Image reference (repo:tag) or ID", Some("image"))
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
        complete_refs
    ));
}

pub(crate) fn complete_refs(plugin: &NudePlugin) -> Option<Vec<DynamicSuggestion>> {
    let docker = plugin.docker().ok()?;
    let opts = ListImagesOptionsBuilder::default().build();
    let images = plugin.rt.block_on(docker.list_images(Some(opts))).ok()?;
    Some(
        images
            .iter()
            .flat_map(|i| i.repo_tags.iter())
            .filter(|t| t.as_str() != "<none>:<none>")
            .map(|value| DynamicSuggestion {
                value: value.clone(),
                ..Default::default()
            })
            .collect(),
    )
}

fn all_images(plugin: &NudePlugin) -> Option<Vec<ImageSummary>> {
    let docker = plugin.docker().ok()?;
    let opts = ListImagesOptionsBuilder::default().all(true).build();
    plugin.rt.block_on(docker.list_images(Some(opts))).ok()
}

async fn run_list(plugin: &NudePlugin, call: &EvaluatedCall) -> anyhow::Result<Value> {
    let all: bool = call.has_flag("all")?;
    let show_labels = call.has_flag("show-labels")?;
    let span = call.head;
    let fmt = scaffold::list_format(call)?;

    let docker = plugin.docker()?;
    let filters = scaffold::collect_filters(call, FILTERS)?;
    let opts = ListImagesOptionsBuilder::default()
        .all(all)
        .filters(&filters)
        .build();
    let summaries = docker.list_images(Some(opts)).await?;
    if fmt == OutputFormat::Compact {
        let rows = summaries
            .iter()
            .flat_map(|i| compact_rows(i, span, show_labels))
            .collect();
        return Ok(Value::list(rows, span));
    }

    let ids: Vec<String> = summaries.into_iter().map(|i| i.id).collect();
    let inspected = skip_vanished(join_all(ids.iter().map(|id| docker.inspect_image(id))).await)?;
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

    let resp = plugin.docker()?.inspect_image(&name).await?;
    Ok(match fmt {
        OutputFormat::Full => full_value(&resp, span),
        _ => wide(&resp, span, show_labels),
    })
}

fn split_ref(reference: &str) -> (Option<&str>, Option<&str>) {
    let (repo, tag) = reference.rsplit_once(':').unwrap_or((reference, ""));
    (clean(repo), clean(tag))
}

fn clean(part: &str) -> Option<&str> {
    (!part.is_empty() && part != "<none>").then_some(part)
}

fn compact_rows(img: &ImageSummary, span: Span, show_labels: bool) -> Vec<Value> {
    let refs: Vec<&str> = if img.repo_tags.is_empty() {
        vec!["<none>:<none>"]
    } else {
        img.repo_tags.iter().map(String::as_str).collect()
    };
    let id = short_id(&img.id);
    refs.iter()
        .map(|r| {
            let (repository, tag) = split_ref(r);
            let mut rec = Record::new();
            rec.push("repository", str_opt(repository, span));
            rec.push("tag", str_opt(tag, span));
            rec.push("id", str_opt(id.as_str(), span));
            rec.push("created", epoch_date(img.created, span));
            rec.push("size", Value::filesize(img.size, span));
            push_labels(&mut rec, show_labels, Some(&img.labels), span);
            Value::record(rec, span)
        })
        .collect()
}

fn wide(img: &ImageInspect, span: Span, show_labels: bool) -> Value {
    let config = img.config.as_ref();

    let mut rec = Record::new();
    rec.push("id", opt_short_id(img.id.as_deref(), span));
    rec.push("repo_tags", str_list(img.repo_tags.as_ref(), span));
    rec.push("repo_digests", str_list(img.repo_digests.as_ref(), span));
    rec.push("size", opt_filesize(img.size, span));
    rec.push("created", opt_rfc3339(img.created.as_deref(), span));
    rec.push("architecture", str_opt(img.architecture.as_deref(), span));
    rec.push("os", str_opt(img.os.as_deref(), span));
    rec.push("author", str_opt(img.author.as_deref(), span));
    rec.push(
        "command",
        str_list(config.and_then(|c| c.cmd.as_ref()), span),
    );
    rec.push(
        "entrypoint",
        str_list(config.and_then(|c| c.entrypoint.as_ref()), span),
    );
    rec.push("env", str_list(config.and_then(|c| c.env.as_ref()), span));
    rec.push(
        "exposed_ports",
        str_list(config.and_then(|c| c.exposed_ports.as_ref()), span),
    );
    rec.push(
        "working_dir",
        str_opt(config.and_then(|c| c.working_dir.as_deref()), span),
    );
    rec.push(
        "layers",
        str_list(img.root_fs.as_ref().and_then(|r| r.layers.as_ref()), span),
    );
    push_labels(
        &mut rec,
        show_labels,
        config.and_then(|c| c.labels.as_ref()),
        span,
    );
    Value::record(rec, span)
}

#[cfg(test)]
mod tests {
    use super::split_ref;

    #[test]
    fn splits_repository_and_tag_at_the_last_colon() {
        assert_eq!(split_ref("nginx:latest"), (Some("nginx"), Some("latest")));
        assert_eq!(
            split_ref("localhost:5000/team/img:v1.2"),
            (Some("localhost:5000/team/img"), Some("v1.2"))
        );
    }

    #[test]
    fn none_placeholder_and_empties_become_none() {
        assert_eq!(split_ref("<none>:<none>"), (None, None));
        assert_eq!(split_ref("repo:<none>"), (Some("repo"), None));
        assert_eq!(split_ref("<none>:latest"), (None, Some("latest")));
    }
}
