mod commands;
mod completers;
mod helpers;
mod output;
mod plugin;
mod scaffold;

use nu_plugin::{MsgPackSerializer, serve_plugin};
use plugin::NudePlugin;

fn main() {
    serve_plugin(&NudePlugin::new(), MsgPackSerializer {});
}
