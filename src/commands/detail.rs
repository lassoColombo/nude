use bollard::models::{
    ChangeType, ContainerTopResponse, FilesystemChange, ImageHistoryResponseItem,
};
use bollard::query_parameters::{TopOptions, TopOptionsBuilder};
use chrono::{DateTime, TimeDelta, Utc};
use nu_plugin::{DynamicCompletionCall, EngineInterface, EvaluatedCall, SimplePluginCommand};
use nu_protocol::{
    engine::ArgType, DynamicSuggestion, LabeledError, Record, Signature, Span, SyntaxShape, Value,
};

use crate::commands::{container, image};
use crate::completers;
use crate::helpers::{epoch_date, full_value, short_id, str_list, str_opt};
use crate::output::OutputFormat;
use crate::plugin::NudePlugin;
use crate::scaffold;

pub struct ContainerDiffCommand;

const DIFF_NAME: &str = "nude container diff";

impl SimplePluginCommand for ContainerDiffCommand {
    type Plugin = NudePlugin;

    fn name(&self) -> &str {
        DIFF_NAME
    }

    fn description(&self) -> &str {
        "Filesystem changes to a container since it started"
    }

    fn signature(&self) -> Signature {
        scaffold::subcommand_signature(DIFF_NAME, "Container name or ID")
    }

    fn run(
        &self,
        plugin: &Self::Plugin,
        _engine: &EngineInterface,
        call: &EvaluatedCall,
        _input: &Value,
    ) -> Result<Value, LabeledError> {
        plugin.block_on_labeled(run_diff(plugin, call))
    }

    #[allow(
        deprecated,
        reason = "ExperimentalMarker gates an experimental API we opt into"
    )]
    fn get_dynamic_completion(
        &self,
        plugin: &Self::Plugin,
        _engine: &EngineInterface,
        _call: DynamicCompletionCall,
        arg_type: ArgType,
        _experimental: nu_protocol::engine::ExperimentalMarker,
    ) -> Option<Vec<DynamicSuggestion>> {
        completers::ref_and_output(plugin, arg_type, container::complete_names)
    }
}

async fn run_diff(plugin: &NudePlugin, call: &EvaluatedCall) -> anyhow::Result<Value> {
    let name: String = call.req(0)?;
    let span = call.head;
    let fmt = scaffold::output_format(call, false)?;
    // `None` = no changes → an empty list, not an error.
    let changes = plugin
        .docker()?
        .container_changes(&name)
        .await?
        .unwrap_or_default();
    let rows = changes
        .iter()
        .map(|c| match fmt {
            OutputFormat::Full => full_value(c, span),
            _ => diff_row(c, span),
        })
        .collect();
    Ok(Value::list(rows, span))
}

fn diff_row(c: &FilesystemChange, span: Span) -> Value {
    let mut rec = Record::new();
    rec.push("path", str_opt(Some(c.path.as_str()), span));
    rec.push("kind", change_kind(c.kind, span));
    Value::record(rec, span)
}

fn change_kind(kind: ChangeType, span: Span) -> Value {
    let s = match kind {
        ChangeType::_0 => "modified",
        ChangeType::_1 => "added",
        ChangeType::_2 => "deleted",
    };
    Value::string(s, span)
}

pub struct ContainerTopCommand;

const TOP_NAME: &str = "nude container top";

// The `ps` formats we ask the host for. Every field is whitespace-free so the daemon's column
// split stays aligned, and `args` — the only one with spaces — is last, because the daemon folds
// every overhanging field into the final column.
const PS_COMPACT: &str = "-eo pid,ppid,user,pcpu,pmem,rss,stat,etimes,time,args";
const PS_WIDE: &str =
    "-eo pid,ppid,pgid,user,pcpu,pmem,rss,vsz,stat,nice,nlwp,tty,etimes,time,args";

/// `ps` title → nude column + how to type it. Covers what we ask for above and docker's default
/// `-ef` (UID PID PPID C STIME TTY TIME CMD); any other title — including one `--ps-args` asks
/// for — falls through as a lowercased string column.
const PS_COLUMNS: &[(&str, &str, Cell)] = &[
    ("PID", "pid", Cell::Int),
    ("PPID", "ppid", Cell::Int),
    ("PGID", "pgid", Cell::Int),
    ("UID", "user", Cell::Str),
    ("USER", "user", Cell::Str),
    ("C", "cpu", Cell::Float),
    ("%CPU", "cpu", Cell::Float),
    ("%MEM", "mem", Cell::Float),
    ("RSS", "rss", Cell::Kib),
    ("VSZ", "vsz", Cell::Kib),
    ("NI", "nice", Cell::Int),
    ("NLWP", "threads", Cell::Int),
    ("S", "state", Cell::State),
    ("STAT", "state", Cell::State),
    ("ELAPSED", "started", Cell::Started),
    ("TIME", "cpu_time", Cell::Duration),
    ("TT", "tty", Cell::Tty),
    ("TTY", "tty", Cell::Tty),
    ("CMD", "command", Cell::Str),
    ("COMMAND", "command", Cell::Str),
];

#[derive(Clone, Copy)]
enum Cell {
    Str,
    Int,
    Float,
    /// `ps` reports RSS/VSZ in KiB.
    Kib,
    /// `[[DD-]HH:]MM:SS[.ff]` or bare seconds.
    Duration,
    /// Seconds since the process started, turned into the instant it started at.
    Started,
    State,
    Tty,
}

impl Cell {
    fn value(self, raw: &str, now: DateTime<Utc>, span: Span) -> Value {
        let nothing = || Value::nothing(span);
        match self {
            Self::Str => str_opt(Some(raw), span),
            Self::Int => raw
                .parse()
                .map_or_else(|_| nothing(), |n| Value::int(n, span)),
            Self::Float => raw
                .parse()
                .map_or_else(|_| nothing(), |n| Value::float(n, span)),
            Self::Kib => raw.parse::<i64>().map_or_else(
                |_| nothing(),
                |kib| Value::filesize(kib.saturating_mul(1024), span),
            ),
            Self::Duration => ps_seconds(raw).map_or_else(nothing, |ns| {
                Value::duration(ns.num_nanoseconds().unwrap_or(0), span)
            }),
            Self::Started => ps_seconds(raw)
                .and_then(|ago| now.checked_sub_signed(ago))
                .map_or_else(nothing, |dt| Value::date(dt.fixed_offset(), span)),
            // The first letter is the state; the rest are modifier flags (`s`, `l`, `+`, …).
            Self::State => match raw.chars().next() {
                Some('R') => Value::string("running", span),
                Some('S') => Value::string("sleeping", span),
                Some('D') => Value::string("waiting", span),
                Some('I') => Value::string("idle", span),
                Some('T') => Value::string("stopped", span),
                Some('t') => Value::string("tracing", span),
                Some('Z') => Value::string("zombie", span),
                Some('X') => Value::string("dead", span),
                _ => nothing(),
            },
            Self::Tty => match raw {
                "?" | "-" => nothing(),
                tty => str_opt(Some(tty), span),
            },
        }
    }
}

impl SimplePluginCommand for ContainerTopCommand {
    type Plugin = NudePlugin;

    fn name(&self) -> &str {
        TOP_NAME
    }

    fn description(&self) -> &str {
        "Running processes in a container (like `docker top`)"
    }

    fn signature(&self) -> Signature {
        scaffold::subcommand_signature(TOP_NAME, "Container name or ID (must be running)").named(
            "ps-args",
            SyntaxShape::String,
            "Arguments for the host's `ps` (default: a typed column set; docker's own is `-ef`)",
            None,
        )
    }

    fn run(
        &self,
        plugin: &Self::Plugin,
        _engine: &EngineInterface,
        call: &EvaluatedCall,
        _input: &Value,
    ) -> Result<Value, LabeledError> {
        plugin.block_on_labeled(run_top(plugin, call))
    }

    #[allow(
        deprecated,
        reason = "ExperimentalMarker gates an experimental API we opt into"
    )]
    fn get_dynamic_completion(
        &self,
        plugin: &Self::Plugin,
        _engine: &EngineInterface,
        _call: DynamicCompletionCall,
        arg_type: ArgType,
        _experimental: nu_protocol::engine::ExperimentalMarker,
    ) -> Option<Vec<DynamicSuggestion>> {
        // Free-form `ps` flags: nothing to suggest, but don't fall back to file completion.
        if matches!(&arg_type, ArgType::Flag(f) if f.as_ref() == "ps-args") {
            return Some(Vec::new());
        }
        completers::ref_and_output(plugin, arg_type, container::complete_names)
    }
}

async fn run_top(plugin: &NudePlugin, call: &EvaluatedCall) -> anyhow::Result<Value> {
    let name: String = call.req(0)?;
    let span = call.head;
    let fmt = scaffold::output_format(call, false)?;
    let custom: Option<String> = call.get_flag("ps-args")?;
    let docker = plugin.docker()?;

    // `full` is verbatim `docker top`, so it keeps the daemon's own default columns.
    let args = match (custom.as_deref(), fmt) {
        (custom @ Some(_), _) => custom,
        (None, OutputFormat::Full) => None,
        (None, OutputFormat::Wide) => Some(PS_WIDE),
        (None, OutputFormat::Compact) => Some(PS_COMPACT),
    };

    let mut top = docker.top_processes(&name, ps_options(args)).await;
    if top.is_err() && custom.is_none() && args.is_some() {
        // A host `ps` without procps' `-o` support (busybox, …) rejects our format; fall back to
        // docker's default columns, typed as far as they allow.
        top = docker.top_processes(&name, None).await;
    }
    let top = top?;

    Ok(match fmt {
        OutputFormat::Full => full_value(&top, span),
        _ => top_table(&top, span),
    })
}

fn ps_options(args: Option<&str>) -> Option<TopOptions> {
    args.map(|args| TopOptionsBuilder::default().ps_args(args).build())
}

fn top_table(top: &ContainerTopResponse, span: Span) -> Value {
    let titles = top.titles.as_deref().unwrap_or_default();
    // One reference instant for the whole table, so sibling rows agree on `started`.
    let now = Utc::now();
    let rows = top
        .processes
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(|process| {
            let mut rec = Record::new();
            for (i, title) in titles.iter().enumerate() {
                let (col, cell) = ps_column(title);
                let raw = process.get(i).map_or("", |s| s.trim());
                rec.push(col, cell.value(raw, now, span));
            }
            Value::record(rec, span)
        })
        .collect();
    Value::list(rows, span)
}

fn ps_column(title: &str) -> (String, Cell) {
    let title = title.to_ascii_uppercase();
    PS_COLUMNS.iter().find(|(ps, ..)| *ps == title).map_or_else(
        || (title.to_lowercase(), Cell::Str),
        |&(_, col, cell)| (col.to_string(), cell),
    )
}

/// `ps` durations are either bare seconds (`etimes`) or a `[[DD-]HH:]MM:SS[.ff]` clock (`time`).
fn ps_seconds(raw: &str) -> Option<TimeDelta> {
    let (days, clock) = match raw.split_once('-') {
        Some((days, clock)) => (days.parse::<f64>().ok()?, clock),
        None => (0.0, raw),
    };
    let mut secs = 0.0;
    let mut units = 0;
    for part in clock.split(':') {
        secs = secs * 60.0 + part.parse::<f64>().ok()?;
        units += 1;
    }
    if !(1..=3).contains(&units) {
        return None;
    }
    TimeDelta::try_milliseconds(((days * 86_400.0 + secs) * 1000.0).round() as i64)
}

pub struct ImageHistoryCommand;

const HISTORY_NAME: &str = "nude image history";

impl SimplePluginCommand for ImageHistoryCommand {
    type Plugin = NudePlugin;

    fn name(&self) -> &str {
        HISTORY_NAME
    }

    fn description(&self) -> &str {
        "Layer history of an image (like `docker history`)"
    }

    fn signature(&self) -> Signature {
        scaffold::subcommand_signature(HISTORY_NAME, "Image reference (repo:tag) or ID")
    }

    fn run(
        &self,
        plugin: &Self::Plugin,
        _engine: &EngineInterface,
        call: &EvaluatedCall,
        _input: &Value,
    ) -> Result<Value, LabeledError> {
        plugin.block_on_labeled(run_history(plugin, call))
    }

    #[allow(
        deprecated,
        reason = "ExperimentalMarker gates an experimental API we opt into"
    )]
    fn get_dynamic_completion(
        &self,
        plugin: &Self::Plugin,
        _engine: &EngineInterface,
        _call: DynamicCompletionCall,
        arg_type: ArgType,
        _experimental: nu_protocol::engine::ExperimentalMarker,
    ) -> Option<Vec<DynamicSuggestion>> {
        completers::ref_and_output(plugin, arg_type, image::complete_refs)
    }
}

async fn run_history(plugin: &NudePlugin, call: &EvaluatedCall) -> anyhow::Result<Value> {
    let name: String = call.req(0)?;
    let span = call.head;
    let fmt = scaffold::output_format(call, false)?;
    let history = plugin.docker()?.image_history(&name).await?;
    let rows = history
        .iter()
        .map(|h| match fmt {
            OutputFormat::Full => full_value(h, span),
            OutputFormat::Wide => history_row(h, span, true),
            OutputFormat::Compact => history_row(h, span, false),
        })
        .collect();
    Ok(Value::list(rows, span))
}

fn history_row(h: &ImageHistoryResponseItem, span: Span, wide: bool) -> Value {
    let mut rec = Record::new();
    rec.push("id", history_id(&h.id, span));
    rec.push("created", epoch_date(h.created, span));
    rec.push("created_by", str_opt(Some(h.created_by.as_str()), span));
    rec.push("size", Value::filesize(h.size, span));
    rec.push("comment", str_opt(Some(h.comment.as_str()), span));
    if wide {
        rec.push("tags", str_list(Some(&h.tags), span));
    }
    Value::record(rec, span)
}

fn history_id(id: &str, span: Span) -> Value {
    let id = id.strip_prefix("sha256:").unwrap_or(id);
    if id.is_empty() || id == "<missing>" {
        Value::nothing(span)
    } else {
        Value::string(short_id(id), span)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(title: &str, raw: &str) -> Value {
        let (_, cell) = ps_column(title);
        cell.value(raw, Utc::now(), Span::test_data())
    }

    fn columns(row: &Value) -> Vec<String> {
        row.as_record().unwrap().columns().cloned().collect()
    }

    fn row(titles: &[&str], values: &[&str]) -> Value {
        let top = ContainerTopResponse {
            titles: Some(titles.iter().map(|s| (*s).to_string()).collect()),
            processes: Some(vec![values.iter().map(|s| (*s).to_string()).collect()]),
        };
        top_table(&top, Span::test_data()).as_list().unwrap()[0].clone()
    }

    #[test]
    fn parses_both_ps_duration_shapes() {
        let secs = |raw| ps_seconds(raw).map(|d| d.num_milliseconds());
        assert_eq!(secs("00:00:03"), Some(3_000)); // TIME, hh:mm:ss
        assert_eq!(secs("12:34"), Some(754_000)); // TIME, mm:ss
        assert_eq!(secs("1-02:03:04"), Some(93_784_000)); // TIME with days
        assert_eq!(secs("15107"), Some(15_107_000)); // etimes, bare seconds
        assert_eq!(secs("04:11:47.50"), Some(15_107_500)); // fractional cputime
        assert_eq!(secs(""), None);
        assert_eq!(secs("?"), None);
        assert_eq!(secs("1:2:3:4"), None);
    }

    #[test]
    fn types_each_known_column() {
        let span = Span::test_data();
        assert_eq!(cell("PID", "853"), Value::int(853, span));
        assert_eq!(cell("%CPU", "1.5"), Value::float(1.5, span));
        assert_eq!(cell("RSS", "26272"), Value::filesize(26_272 * 1024, span));
        assert_eq!(
            cell("TIME", "00:00:03"),
            Value::duration(3_000_000_000, span)
        );
        // Tracing-stop `t` is a different state from stopped `T` — case matters.
        assert_eq!(cell("STAT", "Ssl").as_str().unwrap(), "sleeping");
        assert_eq!(cell("STAT", "t").as_str().unwrap(), "tracing");
        assert_eq!(cell("STAT", "T").as_str().unwrap(), "stopped");
    }

    #[test]
    fn started_is_now_minus_the_elapsed_seconds() {
        let now = Utc::now();
        let (_, cell) = ps_column("ELAPSED");
        let started = cell.value("60", now, Span::test_data());
        assert_eq!(
            (now.fixed_offset() - started.as_date().unwrap()).num_seconds(),
            60
        );
    }

    #[test]
    fn absent_values_become_nothing() {
        let span = Span::test_data();
        assert_eq!(cell("TT", "?"), Value::nothing(span));
        assert_eq!(cell("PID", ""), Value::nothing(span));
        assert_eq!(cell("TIME", "-"), Value::nothing(span));
        assert_eq!(cell("STAT", ""), Value::nothing(span));
    }

    #[test]
    fn compact_rows_are_primitives_only() {
        let row = row(
            &[
                "PID", "PPID", "USER", "%CPU", "%MEM", "RSS", "STAT", "ELAPSED", "TIME", "COMMAND",
            ],
            &[
                "853",
                "835",
                "postgres",
                "0.0",
                "0.3",
                "26272",
                "Ss",
                "15107",
                "00:00:03",
                "postgres: walwriter",
            ],
        );
        assert_eq!(
            columns(&row),
            [
                "pid", "ppid", "user", "cpu", "mem", "rss", "state", "started", "cpu_time",
                "command"
            ]
        );
        assert!(row
            .as_record()
            .unwrap()
            .values()
            .all(|v| !matches!(v, Value::List { .. } | Value::Record { .. })));
    }

    #[test]
    fn the_ef_fallback_is_typed_too() {
        // `ps -ef`: UID PID PPID C STIME TTY TIME CMD — `STIME` has no typed equivalent, so it
        // falls through as a string under its own name.
        let row = row(
            &["UID", "PID", "PPID", "C", "STIME", "TTY", "TIME", "CMD"],
            &[
                "999", "853", "835", "0", "17:52", "?", "00:01:02", "postgres",
            ],
        );
        assert_eq!(
            columns(&row),
            ["user", "pid", "ppid", "cpu", "stime", "tty", "cpu_time", "command"]
        );
        let rec = row.as_record().unwrap();
        assert_eq!(
            rec.get("cpu_time").unwrap(),
            &Value::duration(62_000_000_000, Span::test_data())
        );
        assert_eq!(rec.get("stime").unwrap().as_str().unwrap(), "17:52");
    }

    #[test]
    fn unknown_columns_keep_their_title_as_a_string() {
        let row = row(&["PID", "WCHAN"], &["1", "do_wait"]);
        assert_eq!(columns(&row), ["pid", "wchan"]);
    }
}
