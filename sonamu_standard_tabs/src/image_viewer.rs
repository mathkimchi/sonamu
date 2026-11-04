use image::{ImageReader, RgbaImage};
use sonamu_common::sap::packets::StandardEvent;
use sonamu_sttk::nodular_applet::{AppletContext, StandardApplet};
use sonamu_sttk::nodular_applet::{AppletSpawner, AppletSpawnerTrait, NodularAppletInitializer};
use sonamu_sttk::standard_keybinds::handle_standard_keybinds;
use sonamu_ui::display_units::DisplayContainerSize;
use sonamu_ui::ui_element::UITree;
use std::path::PathBuf;

pub struct ImageViewerApplet {
    image_path: PathBuf,
    /// Why it couldn't be loaded otherwise
    image: Result<RgbaImage, String>,

    focused: bool,

    hook: AppletContext,
}
impl ImageViewerApplet {
    pub fn new<P>(image_path: P, hook: AppletContext) -> Self
    where
        P: AsRef<std::path::Path>,
        PathBuf: From<P>,
    {
        let image = ImageReader::open(&image_path)
            .map_err(|err| err.to_string())
            .and_then(|reader| reader.decode().map_err(|err| err.to_string()))
            .map(|image| image.to_rgba8());

        Self {
            image_path: image_path.into(),
            image,
            focused: true,
            hook,
        }
    }
    pub fn get_initiator<P>(file_path: P) -> impl FnOnce(AppletContext) -> Self
    where
        P: AsRef<std::path::Path>,
        PathBuf: From<P>,
    {
        |hook: AppletContext| Self::new(file_path, hook)
    }
    pub fn get_boxed_initiator<P>(image_path: P) -> NodularAppletInitializer
    where
        P: AsRef<std::path::Path> + Send + 'static,
        PathBuf: From<P>,
    {
        Box::new(|hook: AppletContext| Box::new(Self::new(image_path, hook)))
    }
    #[must_use]
    pub fn get_applet_spawner() -> AppletSpawner {
        struct ImageViewerSpawner;
        impl AppletSpawnerTrait for ImageViewerSpawner {
            fn create_initializer(&self, args: &[&str]) -> Option<NodularAppletInitializer> {
                let file_path = args.first()?;

                Some(ImageViewerApplet::get_boxed_initiator(
                    file_path.to_string(),
                ))
            }

            fn duplicate(&self) -> AppletSpawner {
                Box::new(Self)
            }
        }
        Box::new(ImageViewerSpawner)
    }

    fn get_title(&self) -> String {
        self.image_path.file_name().map_or_else(
            || self.image_path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        )
    }
}
impl StandardApplet for ImageViewerApplet {
    fn handle_standard_event(&mut self, standard_event: StandardEvent) {
        match standard_event {
            StandardEvent::UIEvent(ui_event) => {
                if handle_standard_keybinds(&ui_event, &self.hook) {
                    return;
                }

                self.hook.damage_window();
                // self.hook.damage_treeview();
            }
            StandardEvent::FocusChanged(focus) => {
                self.focused = focus;
                println!("Yay focus {focus}!");

                self.hook.damage_window();
            }
            _ => {}
        }
    }

    fn get_window_standard_applet(&self, _container_size: DisplayContainerSize) -> UITree {
        // UIElement::Image(self.image.clone())
        // TODO: draw the image; applets can't make textures (no wgpu device), so just describe it
        let text = match &self.image {
            Ok(image) => format!(
                "{}: {}x{} (images can't be drawn yet)",
                self.get_title(),
                image.width(),
                image.height()
            ),
            Err(err) => format!("Can't open {}: {err}", self.get_title()),
        };
        UITree::from(text)
    }

    fn get_treeview(&self) -> sonamu_common::utils::tree::world_tree::WorldTree<String> {
        sonamu_common::utils::tree::world_tree::WorldTree::Base(self.get_title())
    }

    fn save(&self) -> Option<sonamu_common::sap::session::SavedApplet> {
        sonamu_sttk::session::saved_with_path("image_viewer", &self.image_path)
    }
}
