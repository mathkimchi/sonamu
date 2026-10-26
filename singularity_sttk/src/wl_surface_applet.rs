use crate::{
    creatable_applet::CreatableNodularApplet,
    nodular_applet::{AppletContext, NodeRequest, StandardApplet},
    session,
    standard_keybinds::standard_keybind_request,
};
use singularity_common::{
    sap::{
        packets::{NewWlSurface, StandardEvent, WlSurfaceEvent, WlSurfaceId, WlSurfaceRequest},
        session::{SavedApplet, title_without_url},
    },
    utils::tree::world_tree::WorldTree,
};
use sonamu_ui::{
    display_units::DisplayContainerSize,
    ui_element::{UIElement, UITree},
    ui_event::UIEvent,
};

/// Shows a wl surface (a wl client's toplevel) and forwards input to it.
///
/// The tree's holder tracks which node shows which surface,
/// and closes the node once the surface is destroyed.
pub struct WlSurfaceApplet {
    surface_id: WlSurfaceId,
    wl_request_queue: calloop::channel::Sender<(WlSurfaceId, WlSurfaceRequest)>,
    hook: AppletContext,
    /// The toplevel's title, empty until the client sets one
    title: String,
    /// Of the client, to save how to start it again
    pid: Option<u32>,
    app_id: Option<String>,
}
impl CreatableNodularApplet<NewWlSurface> for WlSurfaceApplet {
    fn new(surface: NewWlSurface, hook: AppletContext) -> Self {
        hook.listen_wl_surface(surface.wl_event_queue);
        Self {
            surface_id: surface.surface_id,
            wl_request_queue: surface.wl_request_queue,
            hook,
            title: String::new(),
            pid: surface.pid,
            app_id: surface.app_id,
        }
    }
}
impl StandardApplet for WlSurfaceApplet {
    fn handle_standard_event(&mut self, standard_event: StandardEvent) {
        let StandardEvent::UIEvent(ui_event) = standard_event else {
            return;
        };

        let wl_event = match standard_keybind_request(&ui_event) {
            // the tile closes once the client destroys its toplevel (`WlSurfaceEvent::Destroyed`),
            // so the client may refuse (eg to ask about unsaved changes)
            Some(NodeRequest::Close) => WlSurfaceRequest::Close,
            Some(request) => {
                self.hook.request(request);
                return;
            }
            None => match ui_event {
                // the runner sends keys straight to the client
                UIEvent::Key { .. } => return,
                UIEvent::Mouse(mouse_event) => WlSurfaceRequest::Mouse(mouse_event),
                UIEvent::WindowResized(new_size) => WlSurfaceRequest::Resize(new_size),
                // the other swipes aren't forwarded to clients yet
                UIEvent::Swipe { .. } => return,
            },
        };
        // the compositor may be gone while shutting down
        let _ = self.wl_request_queue.send((self.surface_id, wl_event));
    }

    fn handle_wl_surface_event(&mut self, event: WlSurfaceEvent) {
        match event {
            WlSurfaceEvent::Focus => self.hook.request(NodeRequest::FocusSelf),
            // like an applet closing itself
            WlSurfaceEvent::Destroyed => self.hook.close(),
            WlSurfaceEvent::Title(title) => {
                self.title = title;
                self.hook.damage_treeview();
            }
        }
    }

    /// Asks the client; the tile closes once it destroys its toplevel
    fn close_requested(&mut self) -> bool {
        let _ = self
            .wl_request_queue
            .send((self.surface_id, WlSurfaceRequest::Close));
        false
    }

    fn get_window_standard_applet(&self, _container_size: DisplayContainerSize) -> UITree {
        UITree::leaf(UIElement::Subsurface(self.surface_id.as_u64()))
    }

    fn get_treeview(&self) -> WorldTree<String> {
        WorldTree::Base(if self.title.is_empty() {
            "Wl Surface".to_string()
        } else {
            title_without_url(&self.title).to_string()
        })
    }

    /// Read from `/proc` on each publish (eg a title change), so the saved working directory may lag behind
    fn save(&self) -> Option<SavedApplet> {
        Some(SavedApplet::Wl(session::saved_wl(
            self.pid,
            self.app_id.clone(),
            &self.title,
        )))
    }
}
