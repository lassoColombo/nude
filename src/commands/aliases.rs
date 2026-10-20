use nu_plugin::{DynamicCompletionCall, EngineInterface, EvaluatedCall, SimplePluginCommand};
use nu_protocol::{DynamicSuggestion, LabeledError, Signature, Value, engine::ArgType};

use crate::commands::container::ContainerLsCommand;
use crate::commands::detail::{ContainerDiffCommand, ContainerTopCommand, ImageHistoryCommand};
use crate::commands::image::ImageLsCommand;
use crate::commands::system::SystemInfoCommand;
use crate::plugin::NudePlugin;

macro_rules! alias_command {
    ($alias:ident => $canonical:expr, $name:literal, $desc:literal) => {
        pub struct $alias;

        impl SimplePluginCommand for $alias {
            type Plugin = NudePlugin;

            fn name(&self) -> &str {
                $name
            }

            fn description(&self) -> &str {
                $desc
            }

            fn signature(&self) -> Signature {
                let mut sig = $canonical.signature();
                sig.name = $name.to_string();
                sig
            }

            fn run(
                &self,
                plugin: &Self::Plugin,
                engine: &EngineInterface,
                call: &EvaluatedCall,
                input: &Value,
            ) -> Result<Value, LabeledError> {
                $canonical.run(plugin, engine, call, input)
            }

            #[allow(
                deprecated,
                reason = "ExperimentalMarker gates an experimental API we opt into"
            )]
            fn get_dynamic_completion(
                &self,
                plugin: &Self::Plugin,
                engine: &EngineInterface,
                call: DynamicCompletionCall,
                arg_type: ArgType,
                experimental: nu_protocol::engine::ExperimentalMarker,
            ) -> Option<Vec<DynamicSuggestion>> {
                $canonical.get_dynamic_completion(plugin, engine, call, arg_type, experimental)
            }
        }
    };
}

alias_command!(PsCommand      => ContainerLsCommand,   "nude ps",      "List containers (alias of `container ls`)");
alias_command!(ImagesCommand  => ImageLsCommand,       "nude images",  "List images (alias of `image ls`)");
alias_command!(TopCommand     => ContainerTopCommand,  "nude top",     "Running processes in a container (alias of `container top`)");
alias_command!(DiffCommand    => ContainerDiffCommand, "nude diff",    "Filesystem changes to a container (alias of `container diff`)");
alias_command!(HistoryCommand => ImageHistoryCommand,  "nude history", "Layer history of an image (alias of `image history`)");
alias_command!(InfoCommand    => SystemInfoCommand,    "nude info",    "Daemon-wide info (alias of `system info`)");
