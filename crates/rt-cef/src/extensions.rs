//! Chrome (Manifest V3) extensions. Chromium loads them at startup with
//! `--load-extension`: the ones `:extension-install` put in
//! `<data>/extensions/<id>/`, and the folders in `extensions.load`.
//! Installed extensions keep their Web Store id (manifest.json gets the
//! store's public key), so their storage and their native messaging hosts'
//! `allowed_origins` still match.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use cef::*;

use rt_core::Command;
use rt_core::engine::Level;
use rt_core::extensions::{self, Manifest};
use rt_core::prompt::{PromptAnswer, PromptKind, Remember, Topic};

use crate::shell;

/// Whether this run loaded any extensions.
static LOADED: AtomicBool = AtomicBool::new(false);

/// Where `:extension-install` keeps extensions.
fn installed_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("extensions")
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// The folders to load: installed extensions, then `extensions.load`. Only
/// folders with a manifest.json; a comma would split Chromium's list.
pub fn folders(data_dir: &Path, configured: &[String]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(installed_dir(data_dir))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            extensions::parse_id(&p.file_name().unwrap_or_default().to_string_lossy()).is_some()
        })
        .collect();
    out.sort();
    for folder in configured {
        let path = match (folder.strip_prefix("~/"), home()) {
            (Some(rest), Some(home)) => home.join(rest),
            _ => PathBuf::from(folder),
        };
        out.push(path);
    }
    out.retain(|p| p.join("manifest.json").is_file() && !p.to_string_lossy().contains(','));
    out
}

/// The value for `--load-extension`, if there's anything to load.
pub fn load_switch(data_dir: &Path, configured: &[String]) -> Option<String> {
    let folders = folders(data_dir, configured);
    if folders.is_empty() {
        return None;
    }
    LOADED.store(true, Ordering::Relaxed);
    link_native_hosts(data_dir, &host_folders(home().as_deref()));
    let list: Vec<String> = folders
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    Some(list.join(","))
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
            match std::os::unix::fs::symlink(&path, &link) {
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
        // Only Chrome-style views, which call windows have, show chrome:// pages.
        Command::Extensions => crate::window::create_call("chrome://extensions".into()),
        Command::ExtensionInstall { source } => install(source),
        Command::ExtensionRemove { name } => remove(name),
        _ => return false,
    }
    true
}

fn data_dir() -> Option<PathBuf> {
    shell::with(|s| s.paths.data_dir.clone())
}

fn install(source: &str) {
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
    let asks: String = if about.asks.is_empty() {
        "\n  • nothing beyond its own pages".into()
    } else {
        about.asks.iter().map(|a| format!("\n  • {a}")).collect()
    };
    let message = format!(
        "{} {} ({}) asks to:{asks}\nInstall it? It loads after :restart.",
        about.name, about.version, package.crx.id
    );
    crate::prompts::ask(
        None,
        crate::prompts::Scope::Other,
        Topic::Confirm,
        "Install extension",
        message,
        PromptKind::YesNo {
            default: false,
            remember: Remember::Never,
        },
        move |answer| {
            if matches!(answer, PromptAnswer::Yes { .. }) {
                match unpack(&package) {
                    Ok(()) => shell::show_message(
                        Level::Info,
                        format!("Installed {}; :restart loads it", package.about.name),
                    ),
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
    let Some(data_dir) = data_dir() else { return };
    if name.is_empty() {
        return shell::show_message(Level::Error, "Name the extension to remove, or its id");
    }
    let found = std::fs::read_dir(installed_dir(&data_dir))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .find(|dir| {
            let id = dir
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            id == name || installed_name(dir).is_some_and(|n| n.eq_ignore_ascii_case(name))
        });
    let Some(dir) = found else {
        return shell::show_message(Level::Error, format!("No installed extension {name:?}"));
    };
    let label = installed_name(&dir).unwrap_or_else(|| name.to_string());
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => shell::show_message(
            Level::Info,
            format!("Removed {label}; it's gone after :restart"),
        ),
        Err(e) => shell::show_message(Level::Error, format!("Can't remove {label}: {e}")),
    }
}

/// An installed extension's name, from its manifest.
fn installed_name(dir: &Path) -> Option<String> {
    let read = |path: PathBuf| -> Option<serde_json::Value> {
        serde_json::from_str(
            std::fs::read_to_string(path)
                .ok()?
                .trim_start_matches('\u{feff}'),
        )
        .ok()
    };
    let manifest = read(dir.join("manifest.json"))?;
    let messages = manifest["default_locale"]
        .as_str()
        .and_then(|l| read(dir.join("_locales").join(l).join("messages.json")));
    Some(extensions::read_manifest(&manifest, messages.as_ref()).name)
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
