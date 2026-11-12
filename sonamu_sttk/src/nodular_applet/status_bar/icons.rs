//! Freedesktop icons by name (or path), loaded on a thread: the GTK icon theme (and what it inherits), then
//! hicolor, then pixmaps; PNG, or SVG rasterized with resvg

use sonamu_ui::ui_element::Image;
use std::{
    collections::{HashMap, HashSet},
    env, fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        mpsc::{self, Sender},
    },
    thread,
};

/// Rasterized at this many px square (drawn smaller)
const SIZE: u32 = 48;

/// Loaded icons, by name; requests go to the loading thread
pub struct Icons {
    /// `None`: there is none
    loaded: Arc<Mutex<HashMap<String, Option<Image>>>>,
    requested: Mutex<HashSet<String>>,
    requests: Sender<String>,
}

impl Icons {
    /// `changed` is called after each load
    pub fn start(changed: Arc<dyn Fn() + Send + Sync>) -> Self {
        let loaded = Arc::new(Mutex::new(HashMap::new()));
        let (requests, receiver) = mpsc::channel::<String>();
        {
            let loaded = Arc::clone(&loaded);
            thread::spawn(move || {
                // built on the first request
                let mut index = None;
                for name in receiver {
                    let index = index.get_or_insert_with(Index::build);
                    let image = index.find(&name).and_then(|path| load(&path));
                    loaded.lock().unwrap().insert(name, image);
                    changed();
                }
            });
        }
        Self {
            loaded,
            requested: Mutex::new(HashSet::new()),
            requests,
        }
    }

    /// The icon if it's loaded; else asks for it (and is called back once it is)
    pub fn get(&self, name: &str) -> Option<Image> {
        if name.is_empty() {
            return None;
        }
        if let Some(image) = self.loaded.lock().unwrap().get(name) {
            return image.clone();
        }
        if self.requested.lock().unwrap().insert(name.to_string()) {
            let _ = self.requests.send(name.to_string());
        }
        None
    }
}

/// Every icon file of the themes, by name, best first
struct Index {
    icons: HashMap<String, PathBuf>,
}

impl Index {
    fn build() -> Self {
        let data_dirs = data_dirs();
        let mut themes = Vec::new();
        let mut next = vec![gtk_theme().unwrap_or_else(|| "hicolor".to_string())];
        // the theme, what it inherits, then hicolor
        while let Some(theme) = next.pop() {
            if themes.contains(&theme) {
                continue;
            }
            for dir in &data_dirs {
                let index = dir.join("icons").join(&theme).join("index.theme");
                if let Some(inherits) = fs::read_to_string(index).ok().and_then(|text| {
                    text.lines()
                        .find_map(|line| line.strip_prefix("Inherits=").map(str::to_string))
                }) {
                    next.extend(inherits.split(',').rev().map(|s| s.trim().to_string()));
                    break;
                }
            }
            themes.push(theme);
        }
        if !themes.iter().any(|theme| theme == "hicolor") {
            themes.push("hicolor".to_string());
        }
        let mut icons: HashMap<String, (usize, u32, PathBuf)> = HashMap::new();
        for (rank, theme) in themes.iter().enumerate() {
            for dir in &data_dirs {
                walk(&dir.join("icons").join(theme), &mut |path| {
                    let Some((name, score)) = icon_file(path) else {
                        return;
                    };
                    // earlier themes first, then the size nearest `SIZE`
                    let better = icons
                        .get(&name)
                        .is_none_or(|(r, s, _)| (rank, score) < (*r, *s));
                    if better {
                        icons.insert(name, (rank, score, path.to_path_buf()));
                    }
                });
            }
        }
        for dir in &data_dirs {
            walk(&dir.join("pixmaps"), &mut |path| {
                if let Some((name, _)) = icon_file(path) {
                    icons
                        .entry(name)
                        .or_insert((usize::MAX, 0, path.to_path_buf()));
                }
            });
        }
        Self {
            icons: icons
                .into_iter()
                .map(|(name, (_, _, path))| (name, path))
                .collect(),
        }
    }

    fn find(&self, name: &str) -> Option<PathBuf> {
        let path = Path::new(name);
        if path.is_absolute() {
            return path.exists().then(|| path.to_path_buf());
        }
        self.icons.get(name).cloned()
    }
}

/// An icon file's name and how far its size is from `SIZE` (scalable: 1, nearly the best)
fn icon_file(path: &Path) -> Option<(String, u32)> {
    let extension = path.extension()?.to_str()?;
    if !matches!(extension, "png" | "svg") {
        return None;
    }
    let name = path.file_stem()?.to_str()?.to_string();
    // a size in the path, eg `48x48/apps` or `apps/48`
    let size = path
        .ancestors()
        .filter_map(|dir| dir.file_name()?.to_str())
        .find_map(|dir| {
            dir.split('x')
                .next()?
                .split('@')
                .next()?
                .parse::<u32>()
                .ok()
        });
    let score = match (extension, size) {
        ("svg", _) => 1,
        (_, Some(size)) => size.abs_diff(SIZE) + 2,
        (_, None) => 100,
    };
    Some((name, score))
}

fn walk(dir: &Path, found: &mut impl FnMut(&Path)) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, found);
        } else {
            found(&path);
        }
    }
}

/// `$XDG_DATA_HOME` and `$XDG_DATA_DIRS`
pub fn data_dirs() -> Vec<PathBuf> {
    let home = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")));
    let dirs = env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    home.into_iter().chain(env::split_paths(&dirs)).collect()
}

/// `gtk-icon-theme-name` from GTK's settings
fn gtk_theme() -> Option<String> {
    let config = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    ["gtk-4.0", "gtk-3.0"].iter().find_map(|gtk| {
        fs::read_to_string(config.join(gtk).join("settings.ini"))
            .ok()?
            .lines()
            .find_map(|line| {
                let (key, value) = line.split_once('=')?;
                (key.trim() == "gtk-icon-theme-name").then(|| value.trim().to_string())
            })
    })
}

fn load(path: &Path) -> Option<Image> {
    let data = fs::read(path).ok()?;
    if path.extension().is_some_and(|e| e == "svg") {
        let tree = resvg::usvg::Tree::from_data(&data, &resvg::usvg::Options::default()).ok()?;
        let mut pixmap = resvg::tiny_skia::Pixmap::new(SIZE, SIZE)?;
        let size = tree.size();
        #[expect(clippy::cast_precision_loss, reason = "a small size")]
        let scale = SIZE as f32 / size.width().max(size.height());
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        let rgba = pixmap
            .pixels()
            .iter()
            .flat_map(|pixel| {
                let color = pixel.demultiply();
                [color.red(), color.green(), color.blue(), color.alpha()]
            })
            .collect();
        Image::new(SIZE, SIZE, rgba)
    } else {
        let image = image::load_from_memory(&data).ok()?.into_rgba8();
        Image::new(image.width(), image.height(), image.into_raw())
    }
}
