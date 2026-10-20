use bollard::Docker;
use nu_plugin::{Plugin, PluginCommand};
use nu_protocol::{LabeledError, Value};
use std::sync::Mutex;
use tokio::runtime::Runtime;

use crate::commands::aliases::{
    DiffCommand, HistoryCommand, ImagesCommand, InfoCommand, PsCommand, TopCommand,
};
use crate::commands::container::{ContainerInspectCommand, ContainerLsCommand};
use crate::commands::detail::{ContainerDiffCommand, ContainerTopCommand, ImageHistoryCommand};
use crate::commands::image::{ImageInspectCommand, ImageLsCommand};
use crate::commands::network::{NetworkInspectCommand, NetworkLsCommand};
use crate::commands::plugin::{PluginInspectCommand, PluginLsCommand};
use crate::commands::search::SearchCommand;
use crate::commands::system::{SystemInfoCommand, VersionCommand};
use crate::commands::volume::{VolumeInspectCommand, VolumeLsCommand};

pub struct NudePlugin {
    pub rt: Runtime,
    docker: Mutex<Option<Docker>>,
}

impl NudePlugin {
    pub fn new() -> Self {
        Self {
            rt: Runtime::new().expect("failed to create tokio runtime"),
            docker: Mutex::new(None),
        }
    }

    pub fn docker(&self) -> anyhow::Result<Docker> {
        let mut guard = self.docker.lock().unwrap();
        if let Some(d) = guard.as_ref() {
            return Ok(d.clone());
        }
        let d = Docker::connect_with_local_defaults()?;
        *guard = Some(d.clone());
        Ok(d)
    }

    pub fn block_on_labeled(
        &self,
        fut: impl std::future::Future<Output = anyhow::Result<Value>>,
    ) -> Result<Value, LabeledError> {
        self.rt
            .block_on(fut)
            .map_err(|e| LabeledError::new(e.to_string()))
    }
}

impl Default for NudePlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for NudePlugin {
    fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    fn commands(&self) -> Vec<Box<dyn PluginCommand<Plugin = Self>>> {
        vec![
            Box::new(ContainerLsCommand),
            Box::new(ContainerInspectCommand),
            Box::new(ContainerDiffCommand),
            Box::new(ContainerTopCommand),
            Box::new(ImageLsCommand),
            Box::new(ImageInspectCommand),
            Box::new(SearchCommand),
            Box::new(ImageHistoryCommand),
            Box::new(NetworkLsCommand),
            Box::new(NetworkInspectCommand),
            Box::new(VolumeLsCommand),
            Box::new(VolumeInspectCommand),
            Box::new(PluginLsCommand),
            Box::new(PluginInspectCommand),
            Box::new(SystemInfoCommand),
            Box::new(VersionCommand),
            // aliases (docker's short forms)
            Box::new(PsCommand),
            Box::new(ImagesCommand),
            Box::new(TopCommand),
            Box::new(DiffCommand),
            Box::new(HistoryCommand),
            Box::new(InfoCommand),
        ]
    }
}
