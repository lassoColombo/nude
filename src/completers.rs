use std::collections::{BTreeMap, BTreeSet, HashMap};

use nu_plugin::DynamicCompletionCall;
use nu_protocol::{
    DynamicSuggestion,
    ast::{Call, Expr, Expression},
    engine::ArgType,
};

use crate::output::OutputFormat;
use crate::plugin::NudePlugin;

/// Expands to a command's `get_dynamic_completion`. The nu-plugin method is experimental (it
/// takes an `ExperimentalMarker`), so the one place that opts in is here.
///
/// ```ignore
/// dynamic_completion!(|plugin, call, arg_type| completers::flags(arg_type, |flag| match flag {
///     "status" => Some(completers::from_pairs(STATUS)),
///     _ => None,
/// }));
/// ```
macro_rules! dynamic_completion {
    (|$plugin:ident, $call:ident, $arg_type:ident| $body:expr) => {
        #[allow(
            deprecated,
            reason = "ExperimentalMarker gates an experimental API we opt into"
        )]
        #[allow(unused_variables)]
        fn get_dynamic_completion(
            &self,
            $plugin: &Self::Plugin,
            _engine: &nu_plugin::EngineInterface,
            $call: nu_plugin::DynamicCompletionCall,
            $arg_type: nu_protocol::engine::ArgType,
            _experimental: nu_protocol::engine::ExperimentalMarker,
        ) -> Option<Vec<nu_protocol::DynamicSuggestion>> {
            $body
        }
    };
}
pub(crate) use dynamic_completion;

pub fn from_pairs(pairs: &[(&str, &str)]) -> Vec<DynamicSuggestion> {
    pairs
        .iter()
        .map(|&(value, desc)| DynamicSuggestion {
            value: value.to_string(),
            description: (!desc.is_empty()).then(|| desc.to_string()),
            ..Default::default()
        })
        .collect()
}

/// Flag completion with `--output`/`-o` handled once for everyone; `resource` sees the rest.
pub fn flags(
    arg_type: ArgType,
    resource: impl FnOnce(&str) -> Option<Vec<DynamicSuggestion>>,
) -> Option<Vec<DynamicSuggestion>> {
    match arg_type {
        ArgType::Flag(name) => match name.as_ref() {
            "output" | "o" => Some(from_pairs(OutputFormat::ALL)),
            other => resource(other),
        },
        _ => None,
    }
}

/// The inspect / sub-verb shape: a resource ref in position 0, plus `--output`.
pub fn ref_and_output(
    plugin: &NudePlugin,
    arg_type: ArgType,
    name_completer: fn(&NudePlugin) -> Option<Vec<DynamicSuggestion>>,
) -> Option<Vec<DynamicSuggestion>> {
    match arg_type {
        ArgType::Positional(0) => name_completer(plugin),
        other => flags(other, |_| None),
    }
}

pub fn expr_as_str(expr: &Expression) -> Option<&str> {
    match &expr.expr {
        Expr::String(s) | Expr::RawString(s) | Expr::GlobPattern(s, _) => Some(s.as_str()),
        _ => None,
    }
}

pub fn flag_str<'a>(call: &'a Call, name: &str) -> Option<&'a str> {
    call.named_iter()
        .find(|(n, _, _)| n.item == name)
        .and_then(|(_, _, expr)| expr.as_ref())
        .and_then(expr_as_str)
}

pub fn flag_prefix<'a>(call: &'a DynamicCompletionCall, name: &str) -> &'a str {
    let raw = flag_str(&call.call, name).unwrap_or_default();
    if call.strip {
        &raw[..raw.len().saturating_sub(1)]
    } else {
        raw
    }
}

pub fn label_filter(
    typed: &str,
    labels: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<DynamicSuggestion> {
    let (committed, last) = match typed.rfind(',') {
        Some(i) => typed.split_at(i + 1),
        None => ("", typed),
    };

    match last.split_once('=') {
        Some((key, val)) => match labels.get(key) {
            Some(values) if !val.is_empty() && values.contains(val) => {
                vec![pair(format!("{typed},"))]
            }
            Some(values) => values
                .iter()
                .map(|v| pair(format!("{committed}{key}={v}")))
                .collect(),
            None => Vec::new(),
        },
        None => labels
            .keys()
            .map(|k| pair(format!("{committed}{k}=")))
            .collect(),
    }
}

pub fn label_filter_from<'a>(
    typed: &str,
    maps: impl IntoIterator<Item = &'a HashMap<String, String>>,
) -> Vec<DynamicSuggestion> {
    let mut labels: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for map in maps {
        for (k, v) in map {
            labels.entry(k.clone()).or_default().insert(v.clone());
        }
    }
    label_filter(typed, &labels)
}

pub fn complete_labels<'a>(
    call: &DynamicCompletionCall,
    maps: impl IntoIterator<Item = &'a HashMap<String, String>>,
) -> Option<Vec<DynamicSuggestion>> {
    Some(label_filter_from(flag_prefix(call, "labels"), maps))
}

fn pair(value: String) -> DynamicSuggestion {
    DynamicSuggestion {
        value,
        append_whitespace: false,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels() -> BTreeMap<String, BTreeSet<String>> {
        BTreeMap::from([
            (
                "app".to_string(),
                BTreeSet::from(["web".to_string(), "db".to_string()]),
            ),
            ("env".to_string(), BTreeSet::from(["prod".to_string()])),
        ])
    }

    fn values(suggestions: &[DynamicSuggestion]) -> Vec<&str> {
        suggestions.iter().map(|s| s.value.as_str()).collect()
    }

    #[test]
    fn nothing_typed_offers_every_key() {
        assert_eq!(values(&label_filter("", &labels())), ["app=", "env="]);
    }

    #[test]
    fn after_key_eq_offers_that_key_values() {
        assert_eq!(
            values(&label_filter("app=", &labels())),
            ["app=db", "app=web"]
        );
    }

    #[test]
    fn partial_value_still_offers_values_for_nushell_to_filter() {
        assert_eq!(
            values(&label_filter("app=w", &labels())),
            ["app=db", "app=web"]
        );
    }

    #[test]
    fn exact_value_offers_the_separating_comma() {
        assert_eq!(values(&label_filter("app=web", &labels())), ["app=web,"]);
    }

    #[test]
    fn next_key_keeps_the_committed_pairs() {
        assert_eq!(
            values(&label_filter("env=prod,ap", &labels())),
            ["env=prod,app=", "env=prod,env="]
        );
    }

    #[test]
    fn comma_after_a_committed_pair_keeps_the_prefix() {
        assert_eq!(
            values(&label_filter("env=prod,app=web", &labels())),
            ["env=prod,app=web,"]
        );
    }

    #[test]
    fn unknown_key_offers_nothing() {
        assert!(label_filter("nope=", &labels()).is_empty());
    }

    #[test]
    fn candidates_never_end_the_token_with_a_space() {
        assert!(
            label_filter("app=", &labels())
                .iter()
                .all(|s| !s.append_whitespace)
        );
    }

    #[test]
    fn output_flag_is_completed_for_everyone() {
        let got = flags(ArgType::Flag("o".into()), |_| None).unwrap();
        assert_eq!(values(&got), ["compact", "wide", "full"]);
        assert!(flags(ArgType::Flag("other".into()), |_| None).is_none());
        assert!(flags(ArgType::Positional(0), |_| None).is_none());
    }
}
