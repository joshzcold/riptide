//! Choosing files for upload fields: the picker prompt
//! (`fileselect.handler = prompt`), and what a page asks for, which external
//! pickers get as `{dir}`, `{extensions}` and `RIPTIDE_FILESELECT_*`.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::path_complete::{is_separator, split, strip_home};

/// What an upload field takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileMode {
    /// One file.
    Single,
    /// Any number of files (`<input multiple>`).
    Multiple,
    /// A folder (`<input webkitdirectory>`).
    Folder,
}

impl FileMode {
    /// The name `RIPTIDE_FILESELECT_MODE`, Lua and the docs use.
    pub fn name(self) -> &'static str {
        match self {
            FileMode::Single => "file",
            FileMode::Multiple => "files",
            FileMode::Folder => "folder",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "file" => Some(FileMode::Single),
            "files" => Some(FileMode::Multiple),
            "folder" => Some(FileMode::Folder),
            _ => None,
        }
    }

    /// The `fileselect.*.command` setting for this kind of field.
    pub fn command_setting(self) -> &'static str {
        match self {
            FileMode::Single => "fileselect.single_file.command",
            FileMode::Multiple => "fileselect.multiple_files.command",
            FileMode::Folder => "fileselect.folder.command",
        }
    }
}

/// A request for files: from an upload field, or from `rt.ui.pick_files`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileRequest {
    pub mode: FileMode,
    /// The folder to start in.
    pub dir: PathBuf,
    /// The extensions the page accepts, lowercase and without dots; empty
    /// takes any file.
    pub extensions: Vec<String>,
}

impl FileRequest {
    /// The environment external pickers and `fileselect.prompt.command` get.
    pub fn env(&self) -> [(&'static str, String); 3] {
        [
            ("RIPTIDE_FILESELECT_MODE", self.mode.name().to_string()),
            ("RIPTIDE_FILESELECT_DIR", self.dir.display().to_string()),
            ("RIPTIDE_FILESELECT_EXTENSIONS", self.extensions.join(",")),
        ]
    }

    /// `template` with `{}` replaced by `file`, `{dir}` by the folder to
    /// start in and `{extensions}` by the accepted extensions, comma-separated.
    pub fn expand(&self, template: &[String], file: &str) -> Vec<String> {
        let dir = self.dir.display().to_string();
        let extensions = self.extensions.join(",");
        template
            .iter()
            .map(|arg| {
                arg.replace("{dir}", &dir)
                    .replace("{extensions}", &extensions)
                    .replace("{}", file)
            })
            .collect()
    }
}

/// The extensions an upload field accepts, from Chromium's `accept` filters
/// (`.pdf`, `image/*`) and the extensions it expanded them to (`.png;.jpg`,
/// one entry per filter). Empty if any filter can't be told by extension,
/// since then any file might do.
pub fn accepted_extensions(filters: &[String], expanded: &[String]) -> Vec<String> {
    let mut extensions: Vec<String> = Vec::new();
    for (i, filter) in filters.iter().enumerate() {
        let from_chromium = expanded.get(i).map_or("", |e| e.as_str());
        let source = if from_chromium.trim().is_empty() {
            filter.as_str()
        } else {
            from_chromium
        };
        for part in source.split([';', ',']).map(str::trim) {
            let Some(extension) = part.strip_prefix('.').filter(|e| !e.is_empty()) else {
                return Vec::new();
            };
            let extension = extension.to_lowercase();
            if !extensions.contains(&extension) {
                extensions.push(extension);
            }
        }
    }
    extensions
}

/// Whether file `name` has one of `extensions`; any name if there are none.
pub fn has_extension(name: &str, extensions: &[String]) -> bool {
    if extensions.is_empty() {
        return true;
    }
    let lower = name.to_lowercase();
    extensions.iter().any(|e| lower.ends_with(&format!(".{e}")))
}

/// One file or folder the picker offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Relative to the folder being listed, with `/` between folders; or
    /// absolute, if a listing command printed a path outside it.
    pub path: String,
    pub dir: bool,
}

/// How the built-in listing walks a folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalkOptions {
    /// List names starting with a dot, and look inside such folders.
    pub hidden: bool,
    /// Folder names not to look inside, e.g. `node_modules`.
    pub ignore: Vec<String>,
    /// Stop after this many entries.
    pub max: usize,
    pub folders_only: bool,
}

/// Everything under `root`, nearest first: each folder's entries in name
/// order, then the folders inside it. Links to folders are listed but not
/// followed, so a loop can't go on forever.
pub fn walk(root: &Path, options: &WalkOptions) -> Vec<Entry> {
    let mut found = Vec::new();
    let mut queue: VecDeque<(PathBuf, String)> =
        VecDeque::from([(root.to_path_buf(), String::new())]);
    while let Some((dir, prefix)) = queue.pop_front() {
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut children: Vec<(String, bool, bool)> = read
            .filter_map(Result::ok)
            .filter_map(|e| {
                let name = e.file_name().into_string().ok()?;
                let kind = e.file_type().ok()?;
                let link = kind.is_symlink();
                let is_dir = kind.is_dir() || (link && e.path().is_dir());
                Some((name, is_dir, link))
            })
            .filter(|(name, _, _)| options.hidden || !name.starts_with('.'))
            .collect();
        children.sort();
        for (name, is_dir, link) in children {
            if found.len() >= options.max {
                return found;
            }
            let path = format!("{prefix}{name}");
            if is_dir && !link && !options.ignore.contains(&name) {
                queue.push_back((dir.join(&name), format!("{path}/")));
            }
            if is_dir || !options.folders_only {
                found.push(Entry { path, dir: is_dir });
            }
        }
    }
    found
}

/// Entries from a listing command's output, one path per line: relative to
/// `root` (a leading `./` is dropped) or absolute. A trailing separator, or
/// a look at the path, tells folders.
pub fn parse_listing(output: &str, root: &Path, max: usize) -> Vec<Entry> {
    output
        .lines()
        .map(|line| line.trim_end_matches('\r'))
        .filter(|line| !line.trim().is_empty())
        .take(max)
        .map(|line| {
            let line = line.strip_prefix("./").unwrap_or(line);
            let relative = Path::new(line)
                .strip_prefix(root)
                .ok()
                .and_then(|p| p.to_str())
                .unwrap_or(line);
            let marked_dir = relative.ends_with(is_separator);
            let path = relative.trim_end_matches(is_separator).to_string();
            let dir = marked_dir || root.join(&path).is_dir();
            Entry { path, dir }
        })
        .collect()
}

/// What the picker's text means: the folder to list (from a typed path such
/// as `~/src/` or `/tmp/`) and the words to narrow it with.
pub fn parse_input<'a>(text: &'a str, start: &Path, home: Option<&Path>) -> (PathBuf, &'a str) {
    if !is_path(text) {
        return (start.to_path_buf(), text);
    }
    let (dir, rest) = if text == "~" { ("~/", "") } else { split(text) };
    let root = match (strip_home(dir).or((dir == "~").then_some("")), home) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => PathBuf::from(dir),
    };
    (root, rest)
}

/// Whether `text` starts with a folder rather than words: `/`, `~/` or,
/// on Windows, a drive such as `C:\`.
fn is_path(text: &str) -> bool {
    let drive = cfg!(windows)
        && {
            let mut chars = text.chars();
            matches!(
                (chars.next(), chars.next(), chars.next()),
                (Some(letter), Some(':'), Some(sep)) if letter.is_ascii_alphabetic() && is_separator(sep)
            )
        };
    text.starts_with(is_separator) || text == "~" || strip_home(text).is_some() || drive
}

/// `path` with the home folder as `~`, as the picker's text shows it.
pub fn display(path: &Path, home: Option<&Path>) -> String {
    if let Some(rest) = home.and_then(|h| path.strip_prefix(h).ok()) {
        let rest = rest.display().to_string();
        return if rest.is_empty() {
            "~".to_string()
        } else {
            format!("~/{rest}")
        };
    }
    path.display().to_string()
}

/// A folder `path` as typed text that lists it: with a separator at the end.
pub fn folder_text(path: &Path, home: Option<&Path>) -> String {
    let text = display(path, home);
    if text.ends_with(is_separator) {
        text
    } else {
        format!("{text}{}", std::path::MAIN_SEPARATOR)
    }
}

/// The folders the listing for `root` was asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListRequest {
    /// The prompt it's for, to drop a listing that comes back too late.
    pub prompt: u64,
    pub root: PathBuf,
    pub request: FileRequest,
}

/// The picker prompt's state: what's listed, what's typed, what's marked.
#[derive(Clone, Debug)]
pub struct Picker {
    pub prompt: u64,
    pub request: FileRequest,
    root: Option<PathBuf>,
    entries: Vec<Entry>,
    loading: bool,
    error: Option<String>,
    query: String,
    /// Indexes into `entries`, best match first.
    ranked: Vec<usize>,
    /// Index into `ranked`.
    selected: Option<usize>,
    /// Absolute paths, so marks last while moving between folders.
    marked: Vec<PathBuf>,
}

/// What the overlay draws for the picker.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct FilesView {
    /// The folder listed, as `~/…`.
    pub root: String,
    /// The rows shown, scrolled to keep the selection in view.
    pub items: Vec<FileRow>,
    /// How many entries match, and how many the folder has.
    pub matched: usize,
    pub total: usize,
    pub marked: usize,
    pub loading: bool,
    pub error: Option<String>,
    /// The typed words, for highlighting.
    pub words: Vec<String>,
    pub mode: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FileRow {
    /// Folders end in `/`.
    pub name: String,
    pub dir: bool,
    pub selected: bool,
    pub marked: bool,
}

/// The entry a folder picker shows for the folder being listed.
const HERE: &str = ".";

impl Picker {
    pub fn new(prompt: u64, request: FileRequest) -> Self {
        Self {
            prompt,
            request,
            root: None,
            entries: Vec::new(),
            loading: false,
            error: None,
            query: String::new(),
            ranked: Vec::new(),
            selected: None,
            marked: Vec::new(),
        }
    }

    /// Follow the typed `text`: the listing to ask for if it names another
    /// folder, after which [`Picker::set_listing`] fills it in.
    pub fn sync(&mut self, text: &str, home: Option<&Path>) -> Option<ListRequest> {
        let (root, query) = parse_input(text, &self.request.dir, home);
        let wanted = (self.root.as_ref() != Some(&root)).then(|| {
            self.root = Some(root.clone());
            self.entries.clear();
            self.loading = true;
            self.error = None;
            self.query.clear();
            self.rerank();
            ListRequest {
                prompt: self.prompt,
                root,
                request: self.request.clone(),
            }
        });
        if query != self.query {
            query.clone_into(&mut self.query);
            self.rerank();
        }
        wanted
    }

    /// The listing of `root` came back; ignored if the text moved on.
    pub fn set_listing(&mut self, root: &Path, mut entries: Vec<Entry>, error: Option<String>) {
        if self.root.as_deref() != Some(root) {
            return;
        }
        if self.request.mode == FileMode::Folder {
            entries.retain(|e| e.dir);
            entries.insert(
                0,
                Entry {
                    path: HERE.into(),
                    dir: true,
                },
            );
        }
        self.entries = entries;
        self.loading = false;
        self.error = error;
        self.rerank();
    }

    fn rerank(&mut self) {
        let extensions = &self.request.extensions;
        let mut ranked: Vec<(u8, usize)> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.dir || has_extension(&e.path, extensions))
            .filter(|(_, e)| e.path != HERE || self.query.trim().is_empty())
            .filter_map(|(i, e)| Some((crate::completion::rank_words(&self.query, &[&e.path])?, i)))
            .collect();
        // Letters with gaps only when nothing contains the words, as in completion.
        if ranked
            .iter()
            .any(|(r, _)| *r <= crate::completion::CONTAINS)
        {
            ranked.retain(|(r, _)| *r <= crate::completion::CONTAINS);
        }
        ranked.sort_by_key(|(r, _)| *r);
        self.ranked = ranked.into_iter().map(|(_, i)| i).collect();
        self.selected = None;
    }

    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    pub fn marked_count(&self) -> usize {
        self.marked.len()
    }

    /// Whether the listing hasn't come back yet.
    pub fn loading(&self) -> bool {
        self.loading
    }

    fn absolute(&self, entry: &Entry) -> PathBuf {
        let root = self
            .root
            .clone()
            .unwrap_or_else(|| self.request.dir.clone());
        if entry.path == HERE {
            root
        } else {
            root.join(&entry.path)
        }
    }

    /// The selected entry, or the best match if none is.
    pub fn current(&self) -> Option<&Entry> {
        let i = *self.ranked.get(self.selected.unwrap_or(0))?;
        self.entries.get(i)
    }

    /// The current entry as an absolute path.
    pub fn current_path(&self) -> Option<PathBuf> {
        self.current().map(|e| self.absolute(e))
    }

    pub fn focus(&mut self, forward: bool) {
        let len = self.ranked.len();
        if len == 0 {
            return;
        }
        self.selected = Some(match (self.selected, forward) {
            (None, true) => 0,
            (None, false) => len - 1,
            (Some(i), true) => (i + 1) % len,
            (Some(i), false) => (i + len - 1) % len,
        });
    }

    /// Mark or unmark the selected entry (the first or last if none is) and
    /// move on; only moves when the field takes one file.
    pub fn mark(&mut self, forward: bool) {
        if self.request.mode != FileMode::Multiple {
            return self.focus(forward);
        }
        let len = self.ranked.len();
        if len == 0 {
            return;
        }
        let at = self.selected.unwrap_or(if forward { 0 } else { len - 1 });
        let entry = &self.entries[self.ranked[at]];
        if !entry.dir {
            let path = self.absolute(entry);
            match self.marked.iter().position(|m| *m == path) {
                Some(i) => {
                    self.marked.remove(i);
                }
                None => self.marked.push(path),
            }
        }
        self.selected = Some(if forward {
            (at + 1) % len
        } else {
            (at + len - 1) % len
        });
    }

    /// What Return answers with: the marked files, or the current entry.
    pub fn chosen(&self) -> Vec<PathBuf> {
        if !self.marked.is_empty() {
            return self.marked.clone();
        }
        self.current_path().into_iter().collect()
    }

    pub fn view(&self, rows: usize, home: Option<&Path>) -> FilesView {
        let rows = rows.max(1);
        let matched = self.ranked.len();
        let start = self
            .selected
            .map_or(0, |i| i.saturating_sub(rows / 2))
            .min(matched.saturating_sub(rows));
        let items = self
            .ranked
            .iter()
            .enumerate()
            .skip(start)
            .take(rows)
            .map(|(at, &i)| {
                let entry = &self.entries[i];
                let name = if entry.dir {
                    format!("{}/", entry.path)
                } else {
                    entry.path.clone()
                };
                FileRow {
                    name,
                    dir: entry.dir,
                    selected: self.selected == Some(at),
                    marked: self.marked.contains(&self.absolute(entry)),
                }
            })
            .collect();
        let here = usize::from(self.request.mode == FileMode::Folder && !self.entries.is_empty());
        FilesView {
            root: self
                .root
                .as_deref()
                .map(|r| display(r, home))
                .unwrap_or_default(),
            items,
            matched,
            total: self.entries.len() - here,
            marked: self.marked.len(),
            loading: self.loading,
            error: self.error.clone(),
            words: self.query.split_whitespace().map(str::to_string).collect(),
            mode: self.request.mode.name(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rt-fileselect-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        for folder in ["docs/guide", "src", "node_modules/pkg", ".git"] {
            std::fs::create_dir_all(dir.join(folder)).unwrap();
        }
        for file in [
            "README.md",
            "docs/guide/prompts.md",
            "docs/logo.png",
            "src/main.rs",
            "node_modules/pkg/index.js",
            ".git/config",
            ".env",
        ] {
            std::fs::write(dir.join(file), "").unwrap();
        }
        dir
    }

    fn options() -> WalkOptions {
        WalkOptions {
            hidden: false,
            ignore: vec!["node_modules".into()],
            max: 1000,
            folders_only: false,
        }
    }

    fn paths(entries: &[Entry]) -> Vec<&str> {
        entries.iter().map(|e| e.path.as_str()).collect()
    }

    #[test]
    fn walks_nearest_first_without_hidden_or_ignored_folders() {
        let dir = tree();
        let found = walk(&dir, &options());
        assert_eq!(
            paths(&found),
            [
                "README.md",
                "docs",
                "node_modules",
                "src",
                "docs/guide",
                "docs/logo.png",
                "src/main.rs",
                "docs/guide/prompts.md",
            ]
        );
        let hidden = walk(
            &dir,
            &WalkOptions {
                hidden: true,
                ..options()
            },
        );
        assert!(paths(&hidden).contains(&".git/config"));
        assert!(paths(&hidden).contains(&".env"));
        let folders = walk(
            &dir,
            &WalkOptions {
                folders_only: true,
                ..options()
            },
        );
        assert!(folders.iter().all(|e| e.dir), "{folders:?}");
        assert_eq!(
            walk(
                &dir,
                &WalkOptions {
                    max: 2,
                    ..options()
                }
            )
            .len(),
            2
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn listing_output_is_relative_to_the_folder() {
        let dir = tree();
        let output = format!(
            "./src/main.rs\ndocs/\n{}/README.md\n\n/elsewhere/x\n",
            dir.display()
        );
        let entries = parse_listing(&output, &dir, 100);
        assert_eq!(
            entries,
            [
                Entry {
                    path: "src/main.rs".into(),
                    dir: false
                },
                Entry {
                    path: "docs".into(),
                    dir: true
                },
                Entry {
                    path: "README.md".into(),
                    dir: false
                },
                Entry {
                    path: "/elsewhere/x".into(),
                    dir: false
                },
            ]
        );
        assert_eq!(parse_listing(&output, &dir, 1).len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn accepted_extensions_come_from_the_page() {
        let list = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            accepted_extensions(&list(&[".PDF", "image/*"]), &list(&["", ".png;.jpg"])),
            ["pdf", "png", "jpg"]
        );
        assert!(accepted_extensions(&list(&["image/*"]), &[]).is_empty());
        assert!(accepted_extensions(&[], &[]).is_empty());
        assert!(has_extension("a/B.PNG", &list(&["png"])));
        assert!(!has_extension("notes.txt", &list(&["png"])));
        assert!(has_extension("notes.txt", &[]));
    }

    #[test]
    fn external_commands_get_the_folder_and_extensions() {
        let request = FileRequest {
            mode: FileMode::Multiple,
            dir: PathBuf::from("/home/me/Pictures"),
            extensions: vec!["png".into(), "jpg".into()],
        };
        let template = [
            "fd".to_string(),
            "-e{extensions}".into(),
            "{dir}".into(),
            "--out={}".into(),
        ];
        assert_eq!(
            request.expand(&template, "/tmp/chosen"),
            ["fd", "-epng,jpg", "/home/me/Pictures", "--out=/tmp/chosen"]
        );
        assert_eq!(
            request.env()[0],
            ("RIPTIDE_FILESELECT_MODE", "files".to_string())
        );
        assert_eq!(request.env()[2].1, "png,jpg");
    }

    #[test]
    fn typed_paths_pick_the_folder() {
        let home = Path::new("/home/me");
        let start = Path::new("/start");
        assert_eq!(
            parse_input("main rs", start, Some(home)),
            (start.into(), "main rs")
        );
        assert_eq!(
            parse_input("~/src/ma", start, Some(home)),
            (PathBuf::from("/home/me/src/"), "ma")
        );
        assert_eq!(
            parse_input("~", start, Some(home)),
            (PathBuf::from("/home/me/"), "")
        );
        assert_eq!(
            parse_input("/tmp/", start, Some(home)),
            (PathBuf::from("/tmp/"), "")
        );
        assert_eq!(display(Path::new("/home/me/src"), Some(home)), "~/src");
        assert_eq!(display(home, Some(home)), "~");
        assert_eq!(
            folder_text(Path::new("/tmp"), Some(home)),
            format!("/tmp{}", std::path::MAIN_SEPARATOR)
        );
    }

    fn picker(mode: FileMode, extensions: &[&str]) -> (Picker, PathBuf) {
        let dir = tree();
        let mut picker = Picker::new(
            7,
            FileRequest {
                mode,
                dir: dir.clone(),
                extensions: extensions.iter().map(|e| e.to_string()).collect(),
            },
        );
        let wanted = picker.sync("", None).expect("lists the start folder");
        assert_eq!((wanted.prompt, &wanted.root), (7, &dir));
        assert!(picker.sync("", None).is_none(), "only once");
        let folders_only = mode == FileMode::Folder;
        let entries = walk(
            &dir,
            &WalkOptions {
                folders_only,
                ..options()
            },
        );
        picker.set_listing(&dir, entries, None);
        (picker, dir)
    }

    fn names(picker: &Picker) -> Vec<String> {
        picker
            .view(100, None)
            .items
            .into_iter()
            .map(|r| r.name)
            .collect()
    }

    #[test]
    fn typing_narrows_and_ranks_like_completion() {
        let (mut picker, dir) = picker(FileMode::Single, &[]);
        assert!(names(&picker).len() > 5);
        picker.sync("prompts", None);
        assert_eq!(names(&picker), ["docs/guide/prompts.md"]);
        picker.sync("main rs", None);
        assert_eq!(names(&picker), ["src/main.rs"]);
        // Letters with gaps, when nothing contains them.
        picker.sync("dgprm", None);
        assert_eq!(names(&picker), ["docs/guide/prompts.md"]);
        assert_eq!(picker.chosen(), [dir.join("docs/guide/prompts.md")]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn extensions_hide_other_files_but_keep_folders() {
        let (picker, dir) = picker(FileMode::Single, &["png"]);
        assert_eq!(
            names(&picker),
            [
                "docs/",
                "node_modules/",
                "src/",
                "docs/guide/",
                "docs/logo.png"
            ]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn several_files_are_marked_and_chosen_together() {
        let (mut picker, dir) = picker(FileMode::Multiple, &[]);
        picker.sync(".md", None);
        assert_eq!(names(&picker), ["README.md", "docs/guide/prompts.md"]);
        picker.mark(true);
        picker.mark(true);
        let view = picker.view(100, None);
        assert_eq!(view.marked, 2);
        assert!(view.items.iter().all(|r| r.marked));
        // Marks last while the text changes.
        picker.sync("main", None);
        picker.mark(true);
        assert_eq!(
            picker.chosen(),
            [
                dir.join("README.md"),
                dir.join("docs/guide/prompts.md"),
                dir.join("src/main.rs")
            ]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_folder_picker_offers_the_folder_itself_first() {
        let (mut picker, dir) = picker(FileMode::Folder, &[]);
        assert_eq!(names(&picker)[0], "./");
        assert_eq!(picker.chosen(), std::slice::from_ref(&dir));
        assert_eq!(picker.view(100, None).total, 4);
        picker.sync("guide", None);
        assert_eq!(names(&picker), ["docs/guide/"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_view_scrolls_to_the_selection_and_late_listings_are_dropped() {
        let (mut picker, dir) = picker(FileMode::Single, &[]);
        for _ in 0..6 {
            picker.focus(true);
        }
        let view = picker.view(3, None);
        assert_eq!(view.items.len(), 3);
        assert!(view.items[1].selected, "{view:?}");
        // Another folder typed: the old one's listing no longer applies.
        let wanted = picker.sync("/nonexistent-rt/", None).unwrap();
        picker.set_listing(
            &dir,
            vec![Entry {
                path: "late".into(),
                dir: false,
            }],
            None,
        );
        assert!(picker.view(10, None).loading);
        picker.set_listing(&wanted.root, Vec::new(), Some("not found".into()));
        let view = picker.view(10, None);
        assert_eq!(
            (view.loading, view.error.as_deref()),
            (false, Some("not found"))
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
