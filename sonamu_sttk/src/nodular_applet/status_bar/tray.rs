//! The tray (StatusNotifierItem): Sonamu serves `org.kde.StatusNotifierWatcher` (replacing DMS's) and is its host.
//! Items' properties are read every few seconds; their menus (`com.canonical.dbusmenu`) when opened.

use sonamu_ui::ui_element::Image;
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread,
    time::Duration,
};
use zbus::{
    Connection, OwnedObjectPath, OwnedVariant, Proxy, Variant, connection, interface,
    message::Header, object_server::SignalEmitter, proxy::CacheProperties,
};

const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_PATH: &str = "/StatusNotifierWatcher";
const ITEM: &str = "org.kde.StatusNotifierItem";
const MENU: &str = "com.canonical.dbusmenu";
/// How often items are read again (their icons may change)
const INTERVAL: Duration = Duration::from_secs(2);
/// Icon pixmaps up to this size are preferred
const ICON_SIZE: i32 = 48;

#[derive(Clone)]
pub struct Item {
    /// Its bus name and object path
    pub service: String,
    pub path: String,
    pub icon_name: String,
    pub pixmap: Option<Image>,
    /// Its DBusMenu's path
    pub menu: Option<String>,
    /// Only has a menu: clicking opens it
    pub item_is_menu: bool,
    /// Nothing to show right now: not drawn
    pub passive: bool,
}

#[derive(Debug, Clone)]
pub struct MenuEntry {
    pub id: i32,
    pub label: String,
    pub enabled: bool,
    pub separator: bool,
    /// Under this many submenus
    pub depth: usize,
    /// A checkbox's or radio's state
    pub checked: Option<bool>,
}

/// An item's open menu
#[derive(Clone)]
pub struct Menu {
    pub service: String,
    pub path: String,
    pub entries: Vec<MenuEntry>,
}

enum Command {
    Activate(String, String),
    SecondaryActivate(String, String),
    /// Reads its menu into `Tray::menu`
    OpenMenu(String, String),
    MenuClicked(String, String, i32),
}

pub struct Tray {
    pub items: Arc<Mutex<Vec<Item>>>,
    pub menu: Arc<Mutex<Option<Menu>>>,
    commands: Sender<Command>,
}

impl Tray {
    /// Serves the watcher (logs and shows nothing if the name is taken, eg by DMS); `changed` after any change
    pub fn start(changed: Arc<dyn Fn() + Send + Sync>) -> Self {
        let items = Arc::new(Mutex::new(Vec::new()));
        let menu = Arc::new(Mutex::new(None));
        let (commands, receiver) = mpsc::channel();
        {
            let (items, menu) = (Arc::clone(&items), Arc::clone(&menu));
            thread::spawn(move || serve(&items, &menu, &receiver, &*changed));
        }
        Self {
            items,
            menu,
            commands,
        }
    }

    /// Left click: its menu if it's only a menu, else `Activate`
    pub fn activate(&self, item: &Item) {
        let command = if item.item_is_menu && item.menu.is_some() {
            Command::OpenMenu(item.service.clone(), item.menu.clone().unwrap_or_default())
        } else {
            Command::Activate(item.service.clone(), item.path.clone())
        };
        let _ = self.commands.send(command);
    }

    /// Middle click
    pub fn secondary_activate(&self, item: &Item) {
        let _ = self.commands.send(Command::SecondaryActivate(
            item.service.clone(),
            item.path.clone(),
        ));
    }

    /// Right click: reads its menu (shown once read)
    pub fn open_menu(&self, item: &Item) {
        if let Some(menu) = &item.menu {
            let _ = self
                .commands
                .send(Command::OpenMenu(item.service.clone(), menu.clone()));
        }
    }

    pub fn close_menu(&self) {
        *self.menu.lock().unwrap() = None;
    }

    pub fn click_menu(&self, id: i32) {
        if let Some(menu) = self.menu.lock().unwrap().take() {
            let _ = self
                .commands
                .send(Command::MenuClicked(menu.service, menu.path, id));
        }
    }
}

/// Registered items: bus name and path
type Registered = Arc<Mutex<Vec<(String, String)>>>;

fn serve(
    items: &Mutex<Vec<Item>>,
    menu: &Mutex<Option<Menu>>,
    commands: &Receiver<Command>,
    changed: &(dyn Fn() + Send + Sync),
) {
    let registered: Registered = Arc::default();
    let watcher = Watcher {
        registered: Arc::clone(&registered),
    };
    let connection = zbus::block_on(
        connection::Builder::session()
            .name(WATCHER)
            .serve_at(WATCHER_PATH, watcher)
            .build(),
    );
    let connection = match connection {
        Ok(connection) => connection,
        Err(err) => {
            log::warn!("No tray: couldn't serve {WATCHER}: {err}");
            return;
        }
    };
    // items started before (eg after a restart) register again when they see a host
    if let Ok(emitter) = SignalEmitter::new(&connection, WATCHER_PATH) {
        let _ = zbus::block_on(Watcher::status_notifier_host_registered(&emitter));
    }
    loop {
        match commands.recv_timeout(INTERVAL) {
            Ok(command) => run(&connection, &command, menu),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        let list = registered.lock().unwrap().clone();
        let read: Vec<Item> = list
            .iter()
            .filter_map(|(service, path)| read_item(&connection, service, path))
            .collect();
        // gone ones (their properties can't be read) are dropped
        registered.lock().unwrap().retain(|(service, path)| {
            read.iter()
                .any(|item| &item.service == service && &item.path == path)
                || !list.contains(&(service.clone(), path.clone()))
        });
        *items.lock().unwrap() = read;
        changed();
    }
}

fn proxy<'a>(
    connection: &Connection,
    service: &'a str,
    path: &'a str,
    interface: &'a str,
) -> Option<Proxy<'a>> {
    zbus::block_on(
        zbus::proxy::Builder::new(connection)
            .destination(service)
            .path(path)
            .interface(interface)
            .cache_properties(CacheProperties::No)
            .build(),
    )
    .ok()
}

fn run(connection: &Connection, command: &Command, menu: &Mutex<Option<Menu>>) {
    let result = match command {
        Command::Activate(service, path) | Command::SecondaryActivate(service, path) => {
            let method = if matches!(command, Command::Activate(..)) {
                "Activate"
            } else {
                "SecondaryActivate"
            };
            proxy(connection, service, path, ITEM)
                .map(|proxy| zbus::block_on(proxy.call_method(method, &(0i32, 0i32))).map(|_| ()))
        }
        Command::OpenMenu(service, path) => {
            let entries = proxy(connection, service, path, MENU).and_then(|proxy| {
                // lets it fill the menu in first; may not be there
                let _ = zbus::block_on(proxy.call_method("AboutToShow", &(0i32,)));
                read_menu(&proxy)
            });
            *menu.lock().unwrap() = entries.map(|entries| Menu {
                service: service.clone(),
                path: path.clone(),
                entries,
            });
            return;
        }
        Command::MenuClicked(service, path, id) => {
            proxy(connection, service, path, MENU).map(|proxy| {
                zbus::block_on(proxy.call_method("Event", &(*id, "clicked", Variant::I32(0), 0u32)))
                    .map(|_| ())
            })
        }
    };
    if let Some(Err(err)) = result {
        log::warn!("Tray item call failed: {err}");
    }
}

/// `None` once it's gone
fn read_item(connection: &Connection, service: &str, path: &str) -> Option<Item> {
    let proxy = proxy(connection, service, path, ITEM)?;
    let get_string = |name: &str| zbus::block_on(proxy.get_property::<String>(name)).ok();
    // the one property every item has; failing means it's gone
    let icon_name = match zbus::block_on(proxy.get_property::<String>("IconName")) {
        Ok(name) => name,
        Err(zbus::Error::MethodError(..) | zbus::Error::FDO(_)) if get_string("Id").is_some() => {
            String::new()
        }
        Err(_) => return None,
    };
    let pixmap = zbus::block_on(proxy.get_property::<Vec<(i32, i32, Vec<u8>)>>("IconPixmap"))
        .ok()
        .and_then(pixmap);
    Some(Item {
        service: service.to_string(),
        path: path.to_string(),
        icon_name,
        pixmap,
        menu: zbus::block_on(proxy.get_property::<OwnedObjectPath>("Menu"))
            .ok()
            .map(|path| path.to_string())
            .filter(|path| path != "/"),
        item_is_menu: zbus::block_on(proxy.get_property::<bool>("ItemIsMenu")).unwrap_or(false),
        passive: get_string("Status").as_deref() == Some("Passive"),
    })
}

/// The pixmap nearest `ICON_SIZE` (largest under it, else the smallest), ARGB32 in network order to RGBA
fn pixmap(pixmaps: Vec<(i32, i32, Vec<u8>)>) -> Option<Image> {
    let (width, height, argb) = pixmaps
        .into_iter()
        .filter(|(w, h, data)| *w > 0 && *h > 0 && usize::try_from(*w * *h * 4) == Ok(data.len()))
        .min_by_key(|(w, _, _)| if *w <= ICON_SIZE { ICON_SIZE - w } else { *w })?;
    let rgba = argb
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|[a, r, g, b]| [*r, *g, *b, *a])
        .collect();
    Image::new(width.unsigned_abs(), height.unsigned_abs(), rgba)
}

fn read_menu(proxy: &Proxy<'_>) -> Option<Vec<MenuEntry>> {
    type Layout = (i32, HashMap<String, OwnedVariant>, Vec<OwnedVariant>);
    let (_, (_, _, children)): (u32, Layout) =
        zbus::block_on(proxy.call("GetLayout", &(0i32, -1i32, Vec::<&str>::new())))
            .inspect_err(|err| log::warn!("Couldn't read a tray menu: {err}"))
            .ok()?;
    let mut entries = Vec::new();
    for child in &children {
        menu_entries(child, 0, &mut entries);
    }
    Some(entries)
}

/// A layout node `(ia{sv}av)` and its children, in order
fn menu_entries(node: &Variant<'_>, depth: usize, entries: &mut Vec<MenuEntry>) {
    let node = match node {
        Variant::Variant(inner) => inner,
        node => node,
    };
    let Variant::Structure(structure) = node else {
        return;
    };
    let [
        Variant::I32(id),
        Variant::Dict(properties),
        Variant::Array(children),
    ] = structure.fields()
    else {
        return;
    };
    let mut property = HashMap::new();
    for (key, value) in properties.iter() {
        let value = match value {
            Variant::Variant(inner) => &**inner,
            value => value,
        };
        if let Variant::Str(key) = key {
            property.insert(key.as_str().to_string(), value.clone());
        }
    }
    let string = |name: &str| match property.get(name) {
        Some(Variant::Str(value)) => Some(value.as_str().to_string()),
        _ => None,
    };
    if matches!(property.get("visible"), Some(Variant::Bool(false))) {
        return;
    }
    let checked = string("toggle-type")
        .filter(|kind| !kind.is_empty())
        .map(|_| matches!(property.get("toggle-state"), Some(Variant::I32(1))));
    entries.push(MenuEntry {
        id: *id,
        // `_` marks a mnemonic, `__` is an underscore
        label: string("label")
            .unwrap_or_default()
            .replace("__", "\u{0}")
            .replace('_', "")
            .replace('\u{0}', "_"),
        enabled: !matches!(property.get("enabled"), Some(Variant::Bool(false))),
        separator: string("type").as_deref() == Some("separator"),
        depth,
        checked,
    });
    for child in children.iter() {
        menu_entries(child, depth + 1, entries);
    }
}

struct Watcher {
    registered: Registered,
}

#[interface(name = "org.kde.StatusNotifierWatcher")]
impl Watcher {
    /// `service` is a bus name (path `/StatusNotifierItem`) or, as Ayatana's do, an object path on the sender
    async fn register_status_notifier_item(
        &self,
        service: &str,
        #[zbus(header)] header: Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) {
        let (bus, path) = if service.starts_with('/') {
            let sender = header.sender().map(ToString::to_string).unwrap_or_default();
            (sender, service.to_string())
        } else {
            (service.to_string(), "/StatusNotifierItem".to_string())
        };
        let entry = (bus.clone(), path.clone());
        {
            let mut registered = self.registered.lock().unwrap();
            if registered.contains(&entry) {
                return;
            }
            registered.push(entry);
        }
        let _ = Self::status_notifier_item_registered(&emitter, &format!("{bus}{path}")).await;
    }

    #[expect(clippy::unused_self, reason = "a D-Bus method")]
    fn register_status_notifier_host(&self, service: &str) {
        log::debug!("Another tray host: {service}");
    }

    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        self.registered
            .lock()
            .unwrap()
            .iter()
            .map(|(bus, path)| format!("{bus}{path}"))
            .collect()
    }

    #[zbus(property)]
    #[expect(clippy::unused_self, reason = "a D-Bus property")]
    fn is_status_notifier_host_registered(&self) -> bool {
        true
    }

    #[zbus(property)]
    #[expect(clippy::unused_self, reason = "a D-Bus property")]
    fn protocol_version(&self) -> i32 {
        0
    }

    #[zbus(signal)]
    async fn status_notifier_item_registered(
        emitter: &SignalEmitter<'_>,
        service: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn status_notifier_host_registered(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}

/// The menu popup's width in characters
const MENU_COLUMNS: usize = 32;

fn menu_lines(menu: Option<&Menu>) -> usize {
    menu.map_or(1, |menu| menu.entries.len().max(1))
}

pub fn menu_size(menu: Option<&Menu>) -> [f32; 2] {
    super::Lines::size(menu_lines(menu), MENU_COLUMNS)
}

/// Its entries (submenus' indented under them), or `…` while it's read
pub fn draw_menu(
    settings: &sonamu_common::settings::Settings,
    menu: Option<&Menu>,
) -> Vec<sonamu_ui::ui_element::UITree> {
    let palette = &settings.palette;
    let lines = super::Lines::new();
    let Some(menu) = menu.filter(|menu| !menu.entries.is_empty()) else {
        return vec![lines.text(0, 0, vec![("…".to_string(), palette.inactive)])];
    };
    menu.entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            if entry.separator {
                return lines.text(
                    index,
                    0,
                    vec![("─".repeat(MENU_COLUMNS), palette.raised(30))],
                );
            }
            let check = match entry.checked {
                Some(true) => "✓ ",
                Some(false) => "  ",
                None => "",
            };
            let label = format!("{}{check}{}", "  ".repeat(entry.depth), entry.label);
            let color = if entry.enabled {
                palette.fg
            } else {
                palette.inactive
            };
            lines.text(
                index,
                0,
                vec![(super::notification_list::cut(&label, MENU_COLUMNS), color)],
            )
        })
        .collect()
}

/// The clickable entry on `line`
pub fn menu_click(menu: Option<&Menu>, line: usize) -> Option<i32> {
    let entry = menu?.entries.get(line)?;
    (entry.enabled && !entry.separator).then_some(entry.id)
}
