//! Chrome (Manifest V3) extensions. Chromium loads them at startup with
//! `--load-extension`: the ones `:extension-install` put in
//! `<data>/extensions/<id>/`, and the folders in `extensions.load`.
//! Installed extensions keep their Web Store id (manifest.json gets the
//! store's public key), so their storage and their native messaging hosts'
//! `allowed_origins` still match.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use cef::*;

use rt_core::Command;
use rt_core::engine::Level;
use rt_core::extensions::{self, Manifest};
use rt_core::prompt::{PromptAnswer, PromptKind, Remember, Topic};

use crate::shell;

/// Whether this run loaded any extensions.
static LOADED: AtomicBool = AtomicBool::new(false);

/// What this run loaded: id, version and name, to tell what a restart changes.
static STARTED: Mutex<Vec<(String, String, String)>> = Mutex::new(Vec::new());

thread_local! {
    /// Newer versions update checks found, by id.
    static UPDATES: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
    /// Name, version and id of each extension, for completion, which runs
    /// while the shell is busy and can't list them itself.
    static NAMES: RefCell<Vec<(String, String, String)>> = const { RefCell::new(Vec::new()) };
}

/// A download that's an extension (`<ID>_<version>.crx`, as the Web Store
/// names them, or any `.crx`): fetch it and offer to install it. Returns
/// whether it was one.
pub fn crx_download(url: &str, name: &str) -> bool {
    if !name.to_ascii_lowercase().ends_with(".crx") {
        return false;
    }
    let expected = name
        .split(['_', '.'])
        .next()
        .and_then(|stem| extensions::parse_id(&stem.to_ascii_lowercase()));
    if let Some(path) = url.strip_prefix("file://") {
        let path = crate::pages::percent_decode(path).unwrap_or_else(|| path.to_string());
        match std::fs::read(&path) {
            Ok(bytes) => offer(&bytes, expected.as_deref()),
            Err(e) => shell::show_message(Level::Error, format!("Can't read {path}: {e}")),
        }
        return true;
    }
    shell::show_message(Level::Info, "Downloading the extension to install it…");
    crate::fetch::get(url, move |result| match result {
        Ok(bytes) => offer(&bytes, expected.as_deref()),
        Err(e) => shell::show_message(Level::Error, format!("Can't download the extension: {e}")),
    });
    true
}

/// The Web Store asks you to switch to Chrome; say how riptide installs it.
pub fn store_page_loaded(url: &str) {
    let store = rt_core::url::host(url) == "chromewebstore.google.com";
    let current = shell::with(|s| s.tabs.current().is_some_and(|t| t.url == url)).unwrap_or(false);
    if store && current && extensions::parse_id(url).is_some() {
        shell::show_message(
            Level::Info,
            "A Chrome extension: :extension-install installs it in riptide",
        );
    }
}

fn remember_names(entries: &[Entry]) {
    let names = entries
        .iter()
        .map(|e| (e.about.name.clone(), e.about.version.clone(), e.id.clone()))
        .collect();
    NAMES.with(|n| *n.borrow_mut() = names);
}

/// Where `:extension-install` keeps extensions.
fn installed_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("extensions")
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// An extension riptide loads: installed by `:extension-install`, or a
/// folder in `extensions.load`.
pub struct Entry {
    pub id: String,
    pub dir: PathBuf,
    pub installed: bool,
    pub about: Manifest,
}

/// A folder's manifest.json and what it says, its name resolved from
/// `_locales`.
fn read_folder(dir: &Path) -> Option<(serde_json::Value, Manifest)> {
    let read = |path: PathBuf| -> Option<serde_json::Value> {
        let text = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()
    };
    let manifest = read(dir.join("manifest.json"))?;
    let messages = manifest["default_locale"]
        .as_str()
        .and_then(|l| read(dir.join("_locales").join(l).join("messages.json")));
    let about = extensions::read_manifest(&manifest, messages.as_ref());
    Some((manifest, about))
}

/// Every extension to load, by name: installed ones, then `extensions.load`.
/// A folder with a comma is skipped, since it would split Chromium's list.
pub fn inventory(data_dir: &Path, configured: &[String]) -> Vec<Entry> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(installed_dir(data_dir))
        .into_iter()
        .flatten()
        .flatten()
    {
        let dir = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if extensions::parse_id(&name).as_deref() != Some(name.as_str()) {
            continue;
        }
        if let Some((_, about)) = read_folder(&dir) {
            out.push(Entry {
                id: name,
                dir,
                installed: true,
                about,
            });
        }
    }
    for folder in configured {
        let path = match (folder.strip_prefix("~/"), home()) {
            (Some(rest), Some(home)) => home.join(rest),
            _ => PathBuf::from(folder),
        };
        // Chromium names a folder's extension after its absolute path.
        let Ok(dir) = path.canonicalize() else {
            continue;
        };
        let Some((manifest, about)) = read_folder(&dir) else {
            continue;
        };
        let id = extensions::folder_id(&dir.to_string_lossy(), manifest["key"].as_str());
        out.push(Entry {
            id,
            dir,
            installed: false,
            about,
        });
    }
    out.retain(|e| !e.dir.to_string_lossy().contains(','));
    out.sort_by_key(|e| e.about.name.to_lowercase());
    out
}

/// The value for `--load-extension`, if there's anything to load.
pub fn load_switch(data_dir: &Path, configured: &[String]) -> Option<String> {
    let entries = inventory(data_dir, configured);
    if entries.is_empty() {
        return None;
    }
    LOADED.store(true, Ordering::Relaxed);
    remember_names(&entries);
    if let Ok(mut started) = STARTED.lock() {
        *started = entries
            .iter()
            .map(|e| (e.id.clone(), e.about.version.clone(), e.about.name.clone()))
            .collect();
    }
    link_native_hosts(data_dir, &host_folders(home().as_deref()));
    let list: Vec<String> = entries
        .iter()
        .map(|e| e.dir.to_string_lossy().into_owned())
        .collect();
    Some(list.join(","))
}

/// The extensions as the settings page shows them.
fn current() -> Vec<Entry> {
    shell::with(|s| {
        inventory(
            &s.paths.data_dir,
            s.engine.settings().list("extensions.load"),
        )
    })
    .unwrap_or_default()
}

/// An extension by id or name (any case).
fn find(name: &str) -> Option<Entry> {
    current()
        .into_iter()
        .find(|e| e.id == name || e.about.name.eq_ignore_ascii_case(name))
}

/// For the settings page's Extensions tab.
pub fn page_data() -> serde_json::Value {
    let entries = current();
    remember_names(&entries);
    let started = STARTED.lock().map(|s| s.clone()).unwrap_or_default();
    let updates = UPDATES.with(|u| u.borrow().clone());
    let items: Vec<serde_json::Value> = entries
        .iter()
        .map(|e| {
            let state = match started.iter().find(|(id, ..)| *id == e.id) {
                Some((_, version, _)) if *version == e.about.version => "loaded",
                Some(_) => "changed",
                None => "new",
            };
            serde_json::json!({
                "id": e.id,
                "name": e.about.name,
                "version": e.about.version,
                "description": e.about.description,
                "installed": e.installed,
                "folder": (!e.installed).then(|| e.dir.display().to_string()),
                "asks": e.about.asks,
                "popup": e.about.popup.is_some(),
                "options": e.about.options.is_some(),
                "state": state,
                "update": updates.get(&e.id).filter(|v| extensions::newer(v, &e.about.version)),
            })
        })
        .collect();
    let removed: Vec<&String> = started
        .iter()
        .filter(|(id, ..)| !entries.iter().any(|e| e.id == *id))
        .map(|(.., name)| name)
        .collect();
    let restart = !removed.is_empty() || items.iter().any(|i| i["state"] != "loaded");
    serde_json::json!({ "items": items, "removed": removed, "restart": restart })
}

/// Names for `:extension-open`, `-remove` and `-update`.
pub fn completions(pattern: &str) -> Vec<rt_core::completion::Completion> {
    let words: Vec<String> = pattern.split_whitespace().map(str::to_lowercase).collect();
    NAMES.with(|n| {
        n.borrow()
            .iter()
            .filter(|(name, _, id)| {
                let text = format!("{name} {id}").to_lowercase();
                words.iter().all(|w| text.contains(w.as_str()))
            })
            .map(|(name, version, id)| rt_core::completion::Completion {
                icon: None,
                category: "Extensions",
                name: name.clone(),
                description: format!("{version} {id}"),
                time: None,
                detail: None,
            })
            .collect()
    })
}

/// Where other Chromium browsers keep native messaging hosts.
fn host_folders(home: Option<&Path>) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = [
        "/etc/chromium/native-messaging-hosts",
        "/etc/opt/chrome/native-messaging-hosts",
    ]
    .iter()
    .map(PathBuf::from)
    .collect();
    if let Some(home) = home {
        for browser in [
            "chromium",
            "google-chrome",
            "google-chrome-beta",
            "BraveSoftware/Brave-Browser",
            "microsoft-edge",
            "vivaldi",
        ] {
            out.push(
                home.join(".config")
                    .join(browser)
                    .join("NativeMessagingHosts"),
            );
        }
    }
    out
}

/// Native messaging hosts that desktop apps (KeePassXC, 1Password) install
/// for other browsers, linked into the data folder, where Chromium looks for
/// riptide's. Only hosts that allow one of the installed extensions are linked.
fn link_native_hosts(data_dir: &Path, sources: &[PathBuf]) {
    let ids: Vec<String> = std::fs::read_dir(installed_dir(data_dir))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| extensions::parse_id(&e.file_name().to_string_lossy()))
        .collect();
    if ids.is_empty() {
        return;
    }
    let target = data_dir.join("NativeMessagingHosts");
    for source in sources {
        for entry in std::fs::read_dir(source).into_iter().flatten().flatten() {
            let path = entry.path();
            let link = target.join(entry.file_name());
            if path.extension().is_none_or(|e| e != "json") || link.symlink_metadata().is_ok() {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(host) = serde_json::from_str::<serde_json::Value>(&text) else {
                continue;
            };
            let allows = host["allowed_origins"].as_array().is_some_and(|origins| {
                origins.iter().filter_map(|o| o.as_str()).any(|o| {
                    ids.iter()
                        .any(|id| o.trim_end_matches('/') == format!("chrome-extension://{id}"))
                })
            });
            if !allows {
                continue;
            }
            let _ = std::fs::create_dir_all(&target);
            #[cfg(unix)]
            let linked = std::os::unix::fs::symlink(&path, &link);
            // Chromium finds hosts through the registry on Windows; a copy keeps the folder complete.
            #[cfg(not(unix))]
            let linked = std::fs::copy(&path, &link).map(|_| ());
            match linked {
                Ok(()) => tracing::info!("linked native messaging host {}", path.display()),
                Err(e) => tracing::warn!("can't link {}: {e}", path.display()),
            }
        }
    }
}

/// Pages opened while Chromium is still loading extensions' rules can wait
/// forever without a document (CEF says they're loading); reload them.
pub fn after_startup() {
    if !LOADED.load(Ordering::Relaxed) {
        return;
    }
    for ms in [1500, 4000] {
        crate::client::later(ms, reload_stuck);
    }
}

fn reload_stuck() {
    let stuck: Vec<Browser> = shell::with(|s| {
        s.windows
            .iter()
            .flat_map(|w| w.tabs.iter())
            .filter(|t| t.pending.is_none() && t.crashed.is_none() && t.progress.is_none())
            .filter(|t| t.url.starts_with("http") || t.url.starts_with("file:"))
            .filter_map(|t| t.browser())
            .filter(|b| b.has_document() == 0)
            .collect()
    })
    .unwrap_or_default();
    for browser in stuck {
        tracing::info!("reloading a tab that didn't start loading");
        browser.reload();
    }
}

pub fn run_command(command: &Command) -> bool {
    match command {
        Command::ExtensionInstall { source } => install(source),
        Command::ExtensionRemove { name } => remove(name),
        Command::ExtensionOpen { name, page } => open(name, page.as_deref()),
        Command::ExtensionUpdate { name } if name.is_empty() => check_updates(None),
        Command::ExtensionUpdate { name } => match find(name) {
            Some(e) if e.installed => install(&e.id),
            Some(e) => shell::show_message(
                Level::Error,
                format!(
                    "{} is loaded from {}, not installed",
                    e.about.name,
                    e.dir.display()
                ),
            ),
            None => shell::show_message(Level::Error, format!("No extension {name:?}")),
        },
        _ => return false,
    }
    true
}

fn data_dir() -> Option<PathBuf> {
    shell::with(|s| s.paths.data_dir.clone())
}

/// The Extensions tab's Install box (a store page or id, checked already).
pub fn install_from_page(source: &str) {
    install(source);
}

fn install(source: &str) {
    // Without a source: the Web Store page in the current tab.
    let current;
    let source = if source.is_empty() {
        current = shell::with(|s| s.tabs.current().map(|t| t.url.clone()))
            .flatten()
            .unwrap_or_default();
        if extensions::parse_id(&current).is_none() {
            return shell::show_message(
                Level::Error,
                "Open an extension's page on the Chrome Web Store first, or give its page, id or a .crx file",
            );
        }
        current.as_str()
    } else {
        source
    };
    let is_file = source.ends_with(".crx")
        && (source.starts_with('/') || source.starts_with("~/") || source.starts_with("./"));
    if is_file {
        let path = match (source.strip_prefix("~/"), home()) {
            (Some(rest), Some(home)) => home.join(rest),
            _ => PathBuf::from(source),
        };
        return match std::fs::read(&path) {
            Ok(bytes) => offer(&bytes, None),
            Err(e) => {
                shell::show_message(Level::Error, format!("Can't read {}: {e}", path.display()))
            }
        };
    }
    let Some(id) = extensions::parse_id(source) else {
        return shell::show_message(
            Level::Error,
            format!("{source:?} isn't an extension id, a Web Store page or a .crx file"),
        );
    };
    shell::show_message(Level::Info, format!("Downloading extension {id}…"));
    let url = extensions::store_url(&id, cef::sys::CHROME_VERSION_MAJOR);
    crate::fetch::get(&url, move |result| match result {
        Ok(bytes) if bytes.is_empty() => shell::show_message(
            Level::Error,
            format!("The Web Store has no extension {id} for this version of Chromium"),
        ),
        Ok(bytes) => offer(&bytes, Some(&id)),
        Err(e) => shell::show_message(Level::Error, format!("Can't download extension {id}: {e}")),
    });
}

/// An extension's files, read from its zip.
struct Package {
    crx: extensions::Crx,
    manifest: serde_json::Value,
    about: Manifest,
}

fn read_package(bytes: &[u8], expected: Option<&str>) -> Result<Package, String> {
    let crx = extensions::parse_crx(bytes, expected)?;
    let mut zip =
        zip::ZipArchive::new(std::io::Cursor::new(&crx.zip)).map_err(|e| e.to_string())?;
    let mut read = |name: &str| -> Option<serde_json::Value> {
        let mut file = zip.by_name(name).ok()?;
        let mut text = String::new();
        file.read_to_string(&mut text).ok()?;
        serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()
    };
    let manifest = read("manifest.json").ok_or("the extension has no readable manifest.json")?;
    let messages = manifest["default_locale"]
        .as_str()
        .and_then(|locale| read(&format!("_locales/{locale}/messages.json")));
    let about = extensions::read_manifest(&manifest, messages.as_ref());
    Ok(Package {
        crx,
        manifest,
        about,
    })
}

/// Show what the extension asks for, and install it if the user agrees.
fn offer(bytes: &[u8], expected: Option<&str>) {
    let package = match read_package(bytes, expected) {
        Ok(package) => package,
        Err(e) => {
            return shell::show_message(Level::Error, format!("Can't install the extension: {e}"));
        }
    };
    let about = &package.about;
    if about.manifest_version != 3 {
        return shell::show_message(
            Level::Error,
            format!(
                "{} is a Manifest V{} extension, which Chromium no longer runs",
                about.name, about.manifest_version
            ),
        );
    }
    let list =
        |asks: &[String]| -> String { asks.iter().map(|a| format!("\n  • {a}")).collect() };
    let installed = current()
        .into_iter()
        .find(|e| e.installed && e.id == package.crx.id);
    let (title, message) = match installed {
        Some(old) if old.about.version == about.version => {
            return shell::show_message(
                Level::Info,
                format!("{} {} is already installed", about.name, about.version),
            );
        }
        Some(old) => {
            let new: Vec<String> = about
                .asks
                .iter()
                .filter(|a| !old.about.asks.contains(a))
                .cloned()
                .collect();
            let asks = if new.is_empty() {
                "\nIt asks for nothing new.".to_string()
            } else {
                format!("\nIt now also asks to:{}", list(&new))
            };
            (
                "Update extension",
                format!(
                    "Update {} {} to {}?{asks}",
                    about.name, old.about.version, about.version
                ),
            )
        }
        None => {
            let asks = if about.asks.is_empty() {
                "\n  • nothing beyond its own pages".to_string()
            } else {
                list(&about.asks)
            };
            (
                "Install extension",
                format!(
                    "{} {} ({}) asks to:{asks}{popup}\nInstall it? It loads after a restart.",
                    about.name,
                    about.version,
                    package.crx.id,
                    // What it shows in its popup can't reach the page you're on.
                    popup = if about.popup.is_some() {
                        "\nBlocking works. Anything that needs to know which site you're on doesn't, such as per-site switches or a password manager's login suggestions."
                    } else {
                        ""
                    },
                ),
            )
        }
    };
    crate::prompts::ask(
        None,
        crate::prompts::Scope::Other,
        Topic::Confirm,
        title,
        message,
        PromptKind::YesNo {
            default: false,
            remember: Remember::Never,
        },
        move |answer| {
            if matches!(answer, PromptAnswer::Yes { .. }) {
                match unpack(&package) {
                    Ok(()) => {
                        UPDATES.with(|u| u.borrow_mut().remove(&package.crx.id));
                        crate::settings_page::refresh();
                        ask_restart(format!("Installed {}", package.about.name), "loads it");
                    }
                    Err(e) => shell::show_message(
                        Level::Error,
                        format!("Can't install {}: {e}", package.about.name),
                    ),
                }
            }
        },
    );
}

/// Unzip into `<data>/extensions/<id>`, replacing an older version.
fn unpack(package: &Package) -> Result<(), String> {
    let data_dir = data_dir().ok_or("no data folder")?;
    let root = installed_dir(&data_dir);
    let id = &package.crx.id;
    let tmp = root.join(format!("{id}.partial"));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let mut zip =
        zip::ZipArchive::new(std::io::Cursor::new(&package.crx.zip)).map_err(|e| e.to_string())?;
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).map_err(|e| e.to_string())?;
        // enclosed_name refuses paths that would leave the folder.
        let Some(relative) = file.enclosed_name() else {
            return Err(format!(
                "{:?} would be outside the extension's folder",
                file.name()
            ));
        };
        let path = tmp.join(relative);
        if file.is_dir() {
            std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut out = std::fs::File::create(&path).map_err(|e| e.to_string())?;
        std::io::copy(&mut file, &mut out).map_err(|e| e.to_string())?;
    }
    // The store's key keeps the store's id when loaded from a folder.
    let mut manifest = package.manifest.clone();
    manifest["key"] = serde_json::Value::String(extensions::base64(&package.crx.public_key));
    let text = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    std::fs::write(tmp.join("manifest.json"), text).map_err(|e| e.to_string())?;
    let dir = root.join(id);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::rename(&tmp, &dir).map_err(|e| e.to_string())
}

fn remove(name: &str) {
    if name.is_empty() {
        return shell::show_message(Level::Error, "Name the extension to remove, or its id");
    }
    let Some(entry) = find(name) else {
        return shell::show_message(Level::Error, format!("No installed extension {name:?}"));
    };
    if !entry.installed {
        return shell::show_message(
            Level::Error,
            format!(
                "{} is loaded from {}; take it out of extensions.load",
                entry.about.name,
                entry.dir.display()
            ),
        );
    }
    let label = entry.about.name;
    match std::fs::remove_dir_all(&entry.dir) {
        Ok(()) => {
            crate::settings_page::refresh();
            ask_restart(format!("Removed {label}"), "removes it");
        }
        Err(e) => shell::show_message(Level::Error, format!("Can't remove {label}: {e}")),
    }
}

/// Extensions load when riptide starts: offer to restart now.
fn ask_restart(done: String, what: &'static str) {
    crate::prompts::ask(
        None,
        crate::prompts::Scope::Other,
        Topic::Confirm,
        "Restart",
        format!("{done}. A restart {what}. Restart now?"),
        PromptKind::YesNo {
            default: true,
            remember: Remember::Never,
        },
        move |answer| {
            if matches!(answer, PromptAnswer::Yes { .. }) {
                if let Some(effects) = shell::with(|s| s.engine.execute_str("restart", None)) {
                    shell::apply(effects);
                }
            } else {
                let after = if what == "loads it" {
                    ":restart loads it"
                } else {
                    "it's gone after :restart"
                };
                shell::show_message(Level::Info, format!("{done}; {after}"));
            }
        },
    );
}

/// An extension's popup or options page, in a new tab.
fn open(name: &str, page: Option<&str>) {
    let Some(entry) = find(name) else {
        return shell::show_message(Level::Error, format!("No extension {name:?}"));
    };
    let path = match page {
        Some("options") => entry.about.options.clone(),
        Some(_) => entry.about.popup.clone(),
        None => entry
            .about
            .popup
            .clone()
            .or_else(|| entry.about.options.clone()),
    };
    let Some(path) = path else {
        let what = page.unwrap_or("popup or options page");
        return shell::show_message(Level::Error, format!("{} has no {what}", entry.about.name));
    };
    let url = format!("chrome-extension://{}/{path}", entry.id);
    // A popup floats over the page, as in Chrome; options get a tab.
    if Some(&path) == entry.about.popup.as_ref() && page != Some("options") {
        return crate::popup::open(&entry.about.name, &url);
    }
    shell::open(
        rt_core::command::OpenTarget::Tab,
        true,
        Some(format!("chrome-extension://{}/{path}", entry.id)),
    );
}

/// A Settings page button for one extension.
pub fn ui_action(id: &str, action: rt_core::ui_message::ExtensionAction) {
    use rt_core::ui_message::ExtensionAction;
    match action {
        ExtensionAction::Popup => open(id, Some("popup")),
        ExtensionAction::Options => open(id, Some("options")),
        ExtensionAction::Remove => remove(id),
        ExtensionAction::Check => check_updates((!id.is_empty()).then_some(id)),
        ExtensionAction::Update => install(id),
        ExtensionAction::Chrome => chrome_page(),
    }
}

/// Chrome's extensions page, in a call window: only Chrome-style views,
/// which call windows have, show chrome:// pages.
fn chrome_page() {
    crate::window::create_call("chrome://extensions".into());
}

/// Ask the Web Store whether installed extensions (or one) have newer versions.
fn check_updates(only: Option<&str>) {
    let entries: Vec<Entry> = current()
        .into_iter()
        .filter(|e| e.installed && only.is_none_or(|id| e.id == id))
        .collect();
    if entries.is_empty() {
        return shell::show_message(Level::Info, "No installed extensions to check");
    }
    shell::show_message(Level::Info, "Checking for extension updates…");
    let left = std::rc::Rc::new(std::cell::Cell::new(entries.len()));
    let found = std::rc::Rc::new(RefCell::new(Vec::new()));
    for entry in entries {
        let url = extensions::update_check_url(
            &entry.id,
            &entry.about.version,
            cef::sys::CHROME_VERSION_MAJOR,
        );
        let (left, found) = (left.clone(), found.clone());
        crate::fetch::get(&url, move |result| {
            let newer = result
                .ok()
                .and_then(|body| extensions::update_check_version(&String::from_utf8_lossy(&body)))
                .filter(|v| extensions::newer(v, &entry.about.version));
            if let Some(version) = newer {
                UPDATES.with(|u| u.borrow_mut().insert(entry.id.clone(), version.clone()));
                found
                    .borrow_mut()
                    .push(format!("{} {version}", entry.about.name));
            }
            left.set(left.get() - 1);
            if left.get() == 0 {
                let found = found.borrow();
                let text = if found.is_empty() {
                    "Extensions are up to date".to_string()
                } else {
                    format!(
                        "Updates: {}; :extension-update <name> installs one",
                        found.join(", ")
                    )
                };
                shell::show_message(Level::Info, text);
                crate::settings_page::refresh();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_hosts_that_allow_an_installed_extension_are_linked() {
        let dir = std::env::temp_dir().join(format!("rt-ext-hosts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let id = "oboonakemofpalcgghocfoadofidjkkk";
        let data = dir.join("data");
        std::fs::create_dir_all(installed_dir(&data).join(id)).unwrap();
        let hosts = dir.join("home/.config/chromium/NativeMessagingHosts");
        std::fs::create_dir_all(&hosts).unwrap();
        let host = |name: &str, origin: &str| {
            let text = serde_json::json!({ "name": name, "path": "/bin/true", "type": "stdio",
                "allowed_origins": [origin] });
            std::fs::write(hosts.join(format!("{name}.json")), text.to_string()).unwrap();
        };
        host(
            "org.keepassxc.keepassxc_browser",
            &format!("chrome-extension://{id}/"),
        );
        host(
            "com.other.app",
            "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/",
        );
        link_native_hosts(&data, &host_folders(Some(&dir.join("home"))));
        let linked = data.join("NativeMessagingHosts");
        assert!(
            linked
                .join("org.keepassxc.keepassxc_browser.json")
                .is_symlink()
        );
        assert!(
            !linked.join("com.other.app.json").exists(),
            "not for an installed extension"
        );
        // Linking again leaves the link alone.
        link_native_hosts(&data, &host_folders(Some(&dir.join("home"))));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
