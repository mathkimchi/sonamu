//! The first page of a PDF, rendered with hayro on a worker thread (eg the file manager's preview).

use hayro::{
    PixmapSettings, RenderCache, RenderSettings, hayro_interpret::InterpreterSettings,
    hayro_syntax::Pdf, vello_cpu::color::palette::css::WHITE,
};
use sonamu_common::{sap::packets::StandardEvent, utils::tree::world_tree::WorldTree};
use sonamu_sttk::{
    creatable_applet::CreatableNodularApplet,
    nodular_applet::{AppletContext, StandardApplet},
    standard_keybinds::handle_standard_keybinds,
};
use sonamu_ui::{
    display_units::DisplayContainerSize,
    ui_element::{Image, Style, UIElement, UITree},
    ui_event::UIEvent,
};
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
};

/// Longest side of a rendered page, in px
const MAX_SIDE: f32 = 4096.0;

pub struct PdfPreviewApplet {
    file: PathBuf,
    /// The page, or why it can't be shown; `None` until the first render finishes
    page: Option<Result<Image, String>>,
    /// The size the page is rendered for
    size: DisplayContainerSize,
    /// The latest render; older ones finishing later are dropped
    job: u64,
    result_sender: mpsc::Sender<(u64, Result<Image, String>)>,
    results: mpsc::Receiver<(u64, Result<Image, String>)>,
    hook: AppletContext,
}
impl CreatableNodularApplet<PathBuf> for PdfPreviewApplet {
    fn new(file: PathBuf, hook: AppletContext) -> Self {
        let (result_sender, results) = mpsc::channel();
        Self {
            file,
            page: None,
            // rendered once the node sends a size
            size: DisplayContainerSize::new(0, 0),
            job: 0,
            result_sender,
            results,
            hook,
        }
    }
}
impl PdfPreviewApplet {
    /// Renders the page to fit `self.size`, on a worker thread
    fn render(&mut self) {
        self.job += 1;
        let job = self.job;
        let file = self.file.clone();
        let size = self.size;
        let sender = self.result_sender.clone();
        let waker = self.hook.waker();
        std::thread::spawn(move || {
            // hayro may panic on PDFs it doesn't handle
            let page = std::panic::catch_unwind(|| render_first_page(&file, size))
                .unwrap_or_else(|_| Err("The PDF renderer crashed".to_string()));
            let _ = sender.send((job, page));
            waker.wake();
        });
    }
}

/// The first page, as big as fits in `size`
fn render_first_page(file: &Path, size: DisplayContainerSize) -> Result<Image, String> {
    let data = std::fs::read(file).map_err(|err| format!("Can't read it: {err}"))?;
    let pdf = Pdf::new(data).map_err(|err| format!("Can't read it: {err:?}"))?;
    let pages = pdf.pages();
    let page = pages.first().ok_or("It has no pages")?;
    let (width, height) = page.render_dimensions();
    let scale = (size.width as f32 / width)
        .min(size.height as f32 / height)
        .min(MAX_SIDE / width.max(height));
    let pixmap = hayro::render(
        page,
        &RenderCache::new(),
        &InterpreterSettings::default(),
        &RenderSettings::default(),
        &PixmapSettings {
            x_scale: scale,
            y_scale: scale,
            bg_color: WHITE,
        },
    );
    // premultiplied, but opaque on white, so the same as straight
    Image::new(
        pixmap.width().into(),
        pixmap.height().into(),
        pixmap.data_as_u8_slice().to_vec(),
    )
    .ok_or_else(|| "The PDF renderer gave a bad image".to_string())
}

impl StandardApplet for PdfPreviewApplet {
    fn handle_standard_event(&mut self, standard_event: StandardEvent) {
        let StandardEvent::UIEvent(ui_event) = standard_event else {
            return;
        };
        if handle_standard_keybinds(&ui_event, &self.hook) {
            return;
        }
        if let UIEvent::WindowResized(size) = ui_event
            && size != self.size
        {
            self.size = size;
            self.render();
        }
    }

    /// A render finished
    fn wake(&mut self) {
        while let Ok((job, page)) = self.results.try_recv() {
            if job == self.job {
                self.page = Some(page);
                self.hook.damage_window();
            }
        }
    }

    /// The page centered at its rendered size
    fn get_window_standard_applet(&self, container_size: DisplayContainerSize) -> UITree {
        match &self.page {
            Some(Ok(image)) => {
                let [width, height] = image.size().map(|side| side as f32);
                let left = (container_size.width as f32 - width).max(0.0) / 2.0;
                let top = (container_size.height as f32 - height).max(0.0) / 2.0;
                UITree::leaf_styled(
                    UIElement::Image(image.clone()),
                    Style::absolute([left, top], [width, height]),
                )
            }
            Some(Err(err)) => UITree::from(err.clone()),
            None => UITree::from("Rendering...".to_string()),
        }
    }

    fn get_treeview(&self) -> WorldTree<String> {
        WorldTree::Base(self.file.file_name().map_or_else(
            || self.file.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        ))
    }
}
