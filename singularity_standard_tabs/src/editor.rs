use singularity_common::utils::tree::world_tree::WorldTreePath;
use singularity_sar::applet::{BasicApplet, BasicRunnerHook};
use singularity_sttk::creatable_applet::CreatableNodularApplet;
use singularity_sttk::nodular_applet::recursive_node_applet::RecursiveNodeApplet;
use singularity_sttk::nodular_applet::{
    AppletSpawner, AppletSpawnerTrait, NodularAppletInitializer,
};
use singularity_sttk::standard_keybinds::handle_standard_keybinds;
use singularity_sttk::{
    components::text_box::TextBox,
    nodular_applet::{NodularApplet, NodularRunnerHook},
};
use sonamu_ui::display_units::DisplayContainerSize;
use sonamu_ui::ui_event::{KeyModifiers, KeyTrait as _};
use sonamu_ui::{color::Color, ui_element::UIElement, ui_event::UIEvent};
use std::path::PathBuf;

/// Currently Just treats everything like plaintext.
/// This is just the textbox but with a wrapper to work with files.
///
/// TODO: debugger with lldb
///
/// I don't actually know how text editors are usually coded,
/// but I think they don't actually modify the file directly until save,
/// instead having a temporary duplicate file with unsaved changes.
/// I am going to store the temporary data in rust for now.
///
/// NOTE: I want to mention again that this is just a minimal proof of concept
/// because I realized the text editor rabbit hole goes much deeper than I care for at the moment.
/// I don't care about efficiency or even usability.
///
/// NOTE: different types of positions:
/// - absolude display position: where it would be on the display buffer
/// - logical position: where it would be on `temp_text_lines[row]`'s `column`-th character
/// - relative position: depends on what it is relative to, probably relative to text area
pub struct TextEditorApplet {
    file_path: PathBuf,

    textbox: TextBox,
    focused: bool,

    hook: Box<dyn NodularRunnerHook>,
}
impl<P> CreatableNodularApplet<P> for TextEditorApplet
where
    P: AsRef<std::path::Path>,
    PathBuf: std::convert::From<P>,
{
    fn new(file_path: P, hook: Box<dyn NodularRunnerHook + 'static>) -> Self {
        let textbox = TextBox::new(Self::get_content(&file_path));
        let file_path = PathBuf::from(file_path);

        Self {
            file_path,
            textbox,
            focused: true,
            hook,
        }
    }
}
impl TextEditorApplet {
    pub fn get_applet_spawner() -> AppletSpawner {
        struct EditorSpawner;
        impl AppletSpawnerTrait for EditorSpawner {
            fn create_initializer(&self, args: &[&str]) -> Option<NodularAppletInitializer> {
                let file_path = args.first()?;

                Some(RecursiveNodeApplet::boxed_get_boxed_initializer(
                    TextEditorApplet::get_boxed_initiator(file_path.to_string()),
                ))
            }

            fn duplicate(&self) -> AppletSpawner {
                Box::new(Self)
            }
        }
        Box::new(EditorSpawner)
    }

    fn get_content(file_path: impl AsRef<std::path::Path>) -> String {
        std::fs::read_to_string(&file_path).unwrap()
    }

    fn get_title(&self) -> String {
        self.file_path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string()
    }

    fn save_to_file(&self) {
        // let new_path = if self.save_to_temp {
        //     self.file_path.to_str().unwrap().to_string()
        //         + ".temp"
        //         + &std::time::SystemTime::now()
        //             .duration_since(std::time::UNIX_EPOCH)
        //             .unwrap()
        //             .as_millis()
        //             .to_string()
        // } else {
        //     self.file_path.to_str().unwrap().to_string()
        // };

        std::fs::write(&self.file_path, self.textbox.get_text_as_string()).unwrap();
    }
}
impl BasicApplet for TextEditorApplet {
    fn handle_ui_event(&mut self, ui_event: UIEvent) {
        if handle_standard_keybinds(&ui_event, &self.hook) {
            return;
        }

        if let UIEvent::KeyPress(key, KeyModifiers::CTRL) = &ui_event
            && key.to_char() == Some('s')
        {
            self.save_to_file();

            return;
        }

        self.textbox.handle_event(ui_event);

        self.hook.damage_window();
        // self.hook.damage_treeview();
    }

    fn get_window(&self, _container_size: DisplayContainerSize) -> UIElement {
        let cursor_color = if self.focused {
            Color::LIGHT_YELLOW
        } else {
            Color::MEDIUM_GRAY
        };

        self.textbox
            .render_grid_with_color((Color::BLACK, cursor_color))
            .element()
            .fill_bg(Color::BLACK)
    }
}
impl NodularApplet for TextEditorApplet {
    fn handle_nodular_event(
        &mut self,
        nodular_event: singularity_sttk::nodular_applet::NodularEvent,
    ) {
        match nodular_event {
            singularity_sttk::nodular_applet::NodularEvent::Highlighted(_) => todo!(),
            singularity_sttk::nodular_applet::NodularEvent::Focused(focus) => {
                self.focused = focus;
                println!("Yay focus {focus}!");
                println!("The text is: {}", &self.textbox.get_text_as_string());

                self.hook.damage_window();
            }
        }
    }

    fn get_treeview(&self) -> singularity_common::utils::tree::world_tree::WorldTree<String> {
        singularity_common::utils::tree::world_tree::WorldTree::Base(self.get_title())
    }

    fn get_focus_path(&self) -> singularity_common::utils::tree::world_tree::WorldTreePath {
        // WorldTreePath::new_empty()
        WorldTreePath::root()
    }
}

/*
use singularity_macros::EventPacketUnion;
use singularity_sap::{
    byte_stream::{ByteReaderWrapper, ByteStream, CombinedByteStream},
    packet::{EventPacketUnion, PacketTypeId, PacketUnion},
    standard_packets::display_packets::{
        CloseWarningEvent, DisplayEvent, FocusedEvent, RequestChangeName, RequestUpdateWindow,
        SessionStorageQuery, UnfocusedEvent,
    },
    universal_stream::universal_client_stream::UniversalClientStream,
};
use singularity_sttk::components::text_box::TextBox;
use sonamu_ui::{color::Color, ui_element::UIElement, ui_event::KeyModifiers};
use std::{io::Stdout, path::PathBuf};

#[derive(EventPacketUnion)]
pub enum Event {
    #[sub_union]
    DisplayEvent(DisplayEvent),
}

/// Currently Just treats everything like plaintext.
/// This is just the textbox but with a wrapper to work with files.
///
/// TODO debugger with lldb
///
/// I don't actually know how text editors are usually coded,
/// but I think they don't actually modify the file directly until save,
/// instead having a temporary duplicate file with unsaved changes.
/// I am going to store the temporary data in rust for now.
///
/// NOTE: I want to mention again that this is just a minimal proof of concept
/// because I realized the text editor rabbit hole goes much deeper than I care for at the moment.
/// I don't care about efficiency or even usability.
///
/// NOTE: different types of positions:
/// - absolude display position: where it would be on the display buffer
/// - logical position: where it would be on temp_text_lines[row]'s column-th character
/// - relative position: depends on what it is relative to, probably relative to text area
pub struct Editor {
    file_path: PathBuf,

    text_box: TextBox,

    /// debug purpose
    /// TODO remove
    save_to_temp: bool,
}
impl Editor {
    pub fn new<P>(
        file_path: P,
        client_stream: &mut UniversalClientStream<impl ByteStream, Event>,
    ) -> Self
    where
        P: AsRef<std::path::Path>,
        PathBuf: std::convert::From<P>,
    {
        let text_box = TextBox::new(Self::get_content(&file_path));
        let file_path = PathBuf::from(file_path);

        client_stream.send_request(RequestChangeName::new(
            &file_path.file_name().unwrap().to_str().unwrap(),
        ));

        Self {
            file_path,
            text_box,
            save_to_temp: false,
        }
    }

    fn get_content(file_path: impl AsRef<std::path::Path>) -> String {
        std::fs::read_to_string(&file_path).unwrap()
    }

    fn save_to_file(&self) {
        let new_path = if self.save_to_temp {
            self.file_path.to_str().unwrap().to_string()
                + ".temp"
                + &std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis()
                    .to_string()
        } else {
            self.file_path.to_str().unwrap().to_string()
        };

        std::fs::write(new_path, self.text_box.get_text_as_string()).unwrap();
    }

    pub fn initialize_tab(
        client_stream: &mut UniversalClientStream<impl ByteStream, Event>,
    ) -> Self {
        Self::new(
            serde_json::from_value::<String>(client_stream.query(SessionStorageQuery).unwrap().0)
                .unwrap(),
            client_stream,
        )
    }

    pub fn render_tab(&mut self) -> UIElement {
        // highlight cursor
        let cursor_fg = Color::BLACK;
        // TODO: focus
        // let cursor_bg = if manager_handler.focus {
        //     Color::LIGHT_YELLOW
        // } else {
        //     Color::CYAN
        // };
        let cursor_bg = Color::LIGHT_YELLOW;

        UIElement::CharGrid(self.text_box.render_grid_with_color((cursor_fg, cursor_bg)))
            .fill_bg(Color::DARK_GRAY)
            .bordered(Color::LIGHT_GREEN)
    }

    pub fn handle_tab_event(&mut self, event: DisplayEvent) {
        match event {
            DisplayEvent::UIEvent(ref ui_event) => match ui_event {
                sonamu_ui::ui_event::UIEvent::KeyPress(key, KeyModifiers::CTRL)
                    if key.raw_code == 31 =>
                {
                    self.save_to_file();
                }
                _ => {
                    self.text_box.handle_event(event);
                }
            },
            DisplayEvent::Focused(FocusedEvent) => {}
            DisplayEvent::Unfocused(UnfocusedEvent) => {}
            DisplayEvent::Resize(_) => {}
            DisplayEvent::Close(CloseWarningEvent) => {}
        }
    }
}

fn main() {
    let mut client_stream: UniversalClientStream<
        CombinedByteStream<ByteReaderWrapper, Stdout>,
        Event,
    > = UniversalClientStream::new(CombinedByteStream::take_from_stdio());

    let mut editor = Editor::initialize_tab(&mut client_stream);

    // update window on start and when there is an event
    client_stream.send_request(RequestUpdateWindow {
        contents: editor.render_tab(),
    });

    loop {
        let events = client_stream.wait_read_events();
        for Event::DisplayEvent(event) in events {
            editor.handle_tab_event(event);
        }

        // update window on start and when there is an event
        client_stream.send_request(RequestUpdateWindow {
            contents: editor.render_tab(),
        });
    }
}
*/
