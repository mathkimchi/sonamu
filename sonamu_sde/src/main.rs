use singularity_sar::runner::AppletRunner;
use singularity_standard_tabs::{
    code_editor::CodeEditorApplet, file_manager::FileManagerApplet, image_viewer::ImageViewerApplet,
};
use singularity_sttk::{
    command_hub::CommandHubApplet,
    man_page::ManPageApplet,
    nodular_applet::root_node_applet::{self, RootNodeApplet},
    session,
};
use std::collections::BTreeMap;
use tracing_subscriber::EnvFilter;

fn main() {
    init_logging();
    singularity_common::settings::load_and_watch();

    AppletRunner::run(
        RootNodeApplet::get_initializer(
            CommandHubApplet::get_boxed_initiator(),
            BTreeMap::from_iter(vec![
                (
                    "command_hub".to_string(),
                    CommandHubApplet::get_applet_spawner(),
                ),
                // (
                //     "text_editor".to_string(),
                //     TextEditorApplet::get_applet_spawner(),
                // ),
                (
                    "code_editor".to_string(),
                    CodeEditorApplet::get_applet_spawner(),
                ),
                (
                    "file_manager".to_string(),
                    FileManagerApplet::get_applet_spawner(),
                ),
                (
                    "image_viewer".to_string(),
                    ImageViewerApplet::get_applet_spawner(),
                ),
                ("help".to_string(), ManPageApplet::get_applet_spawner()),
            ]),
            session::session_path(),
        ),
        root_node_applet::is_keybind,
    );
}

fn init_logging() {
    tracing_subscriber::fmt()
        // .with_max_level(tracing_subscriber::filter::LevelFilter::DEBUG)
        .with_env_filter(EnvFilter::new("warn,singularity=trace,sonamu=trace"))
        .init();

    // if let Ok(env_filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
    //     tracing_subscriber::fmt().with_env_filter(env_filter).init();
    // } else {
    //     tracing_subscriber::fmt().init();
    // }
}
