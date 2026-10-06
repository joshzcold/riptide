//! The top-level window: the page area and status bar stacked vertically,
//! a tab bar on any side (`tabs.position`), and a completion overlay that
//! floats above the status bar.

use std::cell::Cell;

use cef::*;
use rt_core::tabs::Position;

use crate::client::{Role, RtClient};
use crate::shell;
use crate::storage;
use crate::tabs;
use crate::ui;

pub const STATUSBAR_HEIGHT: i32 = 20;
pub const TABBAR_HEIGHT: i32 = 20;
const CHROME_BACKGROUND: u32 = 0xFF00_0000;

thread_local! {
    /// Open DevTools windows, by the browser id of the tab they inspect.
    static DEVTOOLS_WINDOWS: std::cell::RefCell<std::collections::HashMap<i32, Window>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
    /// The tab bar's preferred size. CEF asks for it during layout, which
    /// can happen while the shell is borrowed, so it lives outside the shell.
    static TABBAR_SIZE: Cell<(i32, i32)> = const { Cell::new((1, TABBAR_HEIGHT)) };
}

/// The DevTools window inspecting browser `id`, if one is open.
pub fn devtools_window(id: i32) -> Option<Window> {
    DEVTOOLS_WINDOWS.with(|d| d.borrow().get(&id).cloned())
}

/// Where the bars go: `tabs.position`, `tabs.width` and `statusbar.position`.
#[derive(Clone, Default, PartialEq)]
pub struct BarPlacement {
    pub tabs: String,
    pub tabs_width: i64,
    pub statusbar: String,
}

/// Where the tab bar goes relative to the page area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabBarSlot {
    Top,
    Bottom,
    Left,
    Right,
}

impl BarPlacement {
    pub fn from_settings(settings: &rt_core::settings::Settings) -> Self {
        Self {
            tabs: settings.str("tabs.position").to_string(),
            tabs_width: settings.int("tabs.width"),
            statusbar: settings.str("statusbar.position").to_string(),
        }
    }

    /// Unknown values fall back to the top, like the default.
    pub fn tab_bar_slot(&self) -> TabBarSlot {
        match self.tabs.as_str() {
            "left" => TabBarSlot::Left,
            "right" => TabBarSlot::Right,
            "bottom" => TabBarSlot::Bottom,
            _ => TabBarSlot::Top,
        }
    }

    /// The tab bar's preferred size, where 1 means "stretch": a column
    /// `tabs.width` wide beside the page, or a row `TABBAR_HEIGHT` high.
    pub fn tab_bar_size(&self) -> (i32, i32) {
        match self.tab_bar_slot() {
            TabBarSlot::Left | TabBarSlot::Right => {
                (i32::try_from(self.tabs_width).unwrap_or(i32::MAX).max(1), 1)
            }
            TabBarSlot::Top | TabBarSlot::Bottom => (1, TABBAR_HEIGHT),
        }
    }

    pub fn statusbar_on_top(&self) -> bool {
        self.statusbar == "top"
    }
}

/// Put the bars where `placement` says. Top and bottom bars go in the
/// window's column, around the row that holds the page area; a left or
/// right tab bar goes in that row. A top status bar sits above a top tab bar.
pub fn arrange_bars(
    window: &Window,
    row: &Panel,
    tabbar: &BrowserView,
    statusbar: &BrowserView,
    placement: &BarPlacement,
) {
    TABBAR_SIZE.with(|size| size.set(placement.tab_bar_size()));
    let mut tabbar = View::from(tabbar);
    let mut statusbar = View::from(statusbar);
    for view in [&mut tabbar, &mut statusbar] {
        if let Some(parent) = view.parent_view().and_then(|p| p.as_panel()) {
            parent.remove_child_view(Some(view));
        }
    }
    // The window's only child is now the row.
    match placement.tab_bar_slot() {
        TabBarSlot::Left => row.add_child_view_at(Some(&mut tabbar), 0),
        TabBarSlot::Right => row.add_child_view(Some(&mut tabbar)),
        TabBarSlot::Bottom => window.add_child_view(Some(&mut tabbar)),
        TabBarSlot::Top => window.add_child_view_at(Some(&mut tabbar), 0),
    }
    if placement.statusbar_on_top() {
        window.add_child_view_at(Some(&mut statusbar), 0);
    } else {
        window.add_child_view(Some(&mut statusbar));
    }
    window.layout();
}

/// Open a window with one tab per URL, or `url.start_pages` if none.
/// `commands` (from the command line) run once the tabs are open. The first
/// window may restore the saved session instead.
pub fn create(urls: Vec<String>, commands: Vec<String>, private: bool) {
    open(urls, commands, private, None);
}

/// A window for one window of a saved session.
pub fn create_from_session(window: rt_storage::WindowState) {
    open(Vec::new(), Vec::new(), false, Some(window));
}

fn open(
    urls: Vec<String>,
    commands: Vec<String>,
    private: bool,
    session: Option<rt_storage::WindowState>,
) {
    let Some(id) = shell::with(|s| s.new_window(private)) else {
        return;
    };
    let mut delegate = RtWindowDelegate::new(id, urls, commands, session);
    window_create_top_level(Some(&mut delegate));
}

/// The request context for new tabs in the active window: the shared
/// in-memory one for private windows, the profile's otherwise.
pub fn request_context() -> Option<RequestContext> {
    shell::with(|s| {
        if !s.private {
            return None;
        }
        if s.private_context.is_none() {
            // An empty cache path keeps everything in memory.
            s.private_context =
                request_context_create_context(Some(&RequestContextSettings::default()), None);
        }
        s.private_context.clone()
    })
    .flatten()
}

pub fn create_browser_view(role: Role, url: &str) -> Option<BrowserView> {
    let mut client = RtClient::new(role);
    let settings = BrowserSettings {
        background_color: if role == Role::Tab {
            0xFFFF_FFFF
        } else {
            CHROME_BACKGROUND
        },
        ..Default::default()
    };
    let mut delegate = RtBrowserViewDelegate::new(role);
    let mut extra_info = (role == Role::Tab)
        .then(crate::greasemonkey::extra_info)
        .flatten();
    let mut context = (role == Role::Tab).then(request_context).flatten();
    browser_view_create(
        Some(&mut client),
        Some(&CefString::from(url)),
        Some(&settings),
        extra_info.as_mut(),
        context.as_mut(),
        Some(&mut delegate),
    )
}

wrap_window_delegate! {
    struct RtWindowDelegate {
        id: u32,
        urls: Vec<String>,
        commands: Vec<String>,
        session: Option<rt_storage::WindowState>,
    }

    impl ViewDelegate {
        fn preferred_size(&self, _view: Option<&mut View>) -> Size {
            Size { width: 1280, height: 800 }
        }
    }

    impl PanelDelegate {}

    impl WindowDelegate {
        fn on_window_created(&self, window: Option<&mut Window>) {
            let Some(window) = window else { return };
            let (Some(tabbar), Some(row), Some(content), Some(statusbar), Some(completion)) = (
                create_browser_view(Role::Tabbar, ui::TABBAR_URL),
                panel_create(None),
                panel_create(None),
                create_browser_view(Role::Statusbar, ui::STATUSBAR_URL),
                create_browser_view(Role::Completion, ui::COMPLETION_URL),
            ) else {
                tracing::error!("failed to create window views");
                return;
            };

            let layout = window.set_to_box_layout(Some(&BoxLayoutSettings {
                horizontal: 0,
                cross_axis_alignment: AxisAlignment::STRETCH,
                ..Default::default()
            }));
            View::from(&tabbar).set_focusable(0);

            // The row holds the page area, and the tab bar when it's on the left or right.
            let row_layout = row.set_to_box_layout(Some(&BoxLayoutSettings {
                horizontal: 1,
                cross_axis_alignment: AxisAlignment::STRETCH,
                ..Default::default()
            }));
            let mut row_view = View::from(&row);
            window.add_child_view(Some(&mut row_view));
            if let Some(layout) = layout {
                layout.set_flex_for_view(Some(&mut row_view), 1);
            }

            // All tab views share the content panel; only the current one is visible.
            content.set_to_fill_layout();
            let mut content_view = View::from(&content);
            row.add_child_view(Some(&mut content_view));
            if let Some(layout) = row_layout {
                layout.set_flex_for_view(Some(&mut content_view), 1);
            }

            View::from(&statusbar).set_focusable(0);

            let mut completion_view = View::from(&completion);
            completion_view.set_focusable(0);
            let overlay = window.add_overlay_view(Some(&mut completion_view), DockingMode::CUSTOM, 0);
            if let Some(overlay) = &overlay {
                overlay.set_visible(0);
            }

            let id = self.id;
            let first = shell::with(|s| {
                let Some(index) = s.window_index(id) else { return false };
                s.active = index;
                s.window = Some(window.clone());
                s.content = Some(content.clone());
                s.tabbar = Some(tabbar.clone());
                s.row = Some(row.clone());
                s.statusbar = Some(statusbar.clone());
                s.completion = Some(completion.clone());
                s.overlay = overlay;
                s.windows.len() == 1
            })
            .unwrap_or(false);

            let placement = shell::with(|s| {
                let placement = BarPlacement::from_settings(s.engine.settings());
                s.bar_placement = placement.clone();
                placement
            });
            if let Some(placement) = placement {
                arrange_bars(window, &row, &tabbar, &statusbar, &placement);
            }

            window.show();
            if let Some(session) = &self.session {
                tabs::restore_window(session);
            } else {
                let crashed = if first { storage::crashed_session() } else { None };
                let recovered = match crashed {
                    Some(session) if self.urls.is_empty() => {
                        tabs::restore(&session);
                        shell::show_message(rt_core::engine::Level::Info, "Restored the tabs open before the crash");
                        true
                    }
                    Some(_) => {
                        shell::show_message(
                            rt_core::engine::Level::Info,
                            "The tabs open before the crash are in :session-load _autosave",
                        );
                        false
                    }
                    None => false,
                };
                let restore = first
                    && !recovered
                    && self.urls.is_empty()
                    && shell::with(|s| s.engine.settings().bool("auto_save.session")).unwrap_or(false);
                let restored = recovered || restore
                    && match storage::load_session(&storage::default_session()) {
                        Ok(session) => {
                            tabs::restore(&session);
                            true
                        }
                        Err(e) => {
                            tracing::info!("no session to restore: {e}");
                            false
                        }
                    };
                if !restored {
                    open_start_tabs(&self.urls);
                }
                if first {
                    storage::start_autosave();
                }
            }
            crate::remote::run_commands(&self.commands);
        }

        fn on_window_activation_changed(&self, _window: Option<&mut Window>, active: ::std::os::raw::c_int) {
            if active != 0 {
                shell::activate_window(self.id);
            }
        }

        fn on_window_destroyed(&self, _window: Option<&mut Window>) {
            // Dropping the last view reference closes its browser synchronously,
            // which re-enters the shell, so drop them outside the borrow.
            let id = self.id;
            let state = shell::with(|s| {
                let index = s.window_index(id)?;
                let state = if s.windows.len() > 1 {
                    let state = s.windows.remove(index);
                    if s.active >= index && s.active > 0 {
                        s.active -= 1;
                    }
                    state
                } else {
                    // The shell always has a window; keep a closed placeholder.
                    let mut placeholder = shell::WindowState::new(id, false);
                    placeholder.window_closing = true;
                    std::mem::replace(&mut s.windows[index], placeholder)
                };
                Some(state)
            })
            .flatten();
            drop(state);
            shell::refresh_ui();
        }

        fn can_close(&self, window: Option<&mut Window>) -> ::std::os::raw::c_int {
            // Let every page run its unload handlers; CEF closes the window
            // once all of them agree. CEF may ask again while pages unload,
            // so save the session only the first time, and only when the
            // last window closes (`:quit` saves all windows itself).
            let id = self.id;
            // Closing the last window quits, so confirm_quit may ask first.
            let last = shell::with(|s| {
                let index = s.window_index(id)?;
                let open = s.windows.iter().filter(|w| !w.window_closing).count();
                Some(!s.quitting && !s.windows[index].window_closing && open == 1)
            })
            .flatten()
            .unwrap_or(false);
            if last {
                let window = window.map(|w| w.clone());
                let go_ahead = shell::confirm_quit(move || {
                    if let Some(window) = window {
                        window.close();
                    }
                });
                if !go_ahead {
                    return 0;
                }
            }
            let save = shell::with(|s| {
                let index = s.window_index(id)?;
                let first = !s.windows[index].window_closing;
                s.windows[index].window_closing = true;
                let last = s.windows.iter().filter(|w| !w.window_closing).count() == 0;
                Some(first && last && !s.quitting && s.engine.settings().bool("auto_save.session"))
            })
            .flatten()
            .unwrap_or(false);
            if save && let Err(e) = storage::save_session(&storage::default_session()) {
                tracing::warn!("could not save session: {e}");
            }
            let hosts: Vec<BrowserHost> = shell::with(|s| {
                let index = s.window_index(id)?;
                Some(s.windows[index].tabs.iter().filter_map(|t| t.browser()?.host()).collect())
            })
            .flatten()
            .unwrap_or_default();
            let closable = hosts.iter().map(|h| h.try_close_browser()).filter(|&ok| ok == 0).count() == 0;
            closable.into()
        }

        fn window_runtime_style(&self) -> RuntimeStyle {
            RuntimeStyle::ALLOY
        }
    }
}

wrap_browser_view_delegate! {
    struct RtBrowserViewDelegate {
        role: Role,
    }

    impl ViewDelegate {
        fn preferred_size(&self, _view: Option<&mut View>) -> Size {
            bar_size(self.role)
        }

        fn minimum_size(&self, _view: Option<&mut View>) -> Size {
            bar_size(self.role)
        }

        fn on_layout_changed(&self, _view: Option<&mut View>, _new_bounds: Option<&Rect>) {
            if self.role == Role::Statusbar {
                shell::position_overlay();
            }
        }
    }

    impl BrowserViewDelegate {
        fn browser_runtime_style(&self) -> RuntimeStyle {
            RuntimeStyle::ALLOY
        }

        fn delegate_for_popup_browser_view(
            &self,
            _browser_view: Option<&mut BrowserView>,
            _settings: Option<&BrowserSettings>,
            _client: Option<&mut Client>,
            is_devtools: ::std::os::raw::c_int,
        ) -> Option<BrowserViewDelegate> {
            if is_devtools != 0 {
                return Some(DevToolsViewDelegate::new());
            }
            Some(RtBrowserViewDelegate::new(Role::Tab))
        }

        // Popups become tabs. Letting CEF create them (rather than opening the
        // URL ourselves) keeps `window.opener`, which login flows rely on.
        fn on_popup_browser_view_created(
            &self,
            browser_view: Option<&mut BrowserView>,
            popup_browser_view: Option<&mut BrowserView>,
            is_devtools: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            let Some(popup) = popup_browser_view else { return 0 };
            if is_devtools != 0 {
                let inspected = browser_view.and_then(|v| v.browser()).map_or(0, |b| b.identifier());
                let mut delegate = DevToolsWindowDelegate::new(popup.clone(), inspected);
                if let Some(window) = window_create_top_level(Some(&mut delegate)) {
                    DEVTOOLS_WINDOWS.with(|d| d.borrow_mut().insert(inspected, window));
                }
                return 1;
            }
            // The popup becomes a tab in its opener's window.
            if let Some(opener) = browser_view.and_then(|v| v.browser()) {
                shell::activate_browser(opener.identifier());
            }
            let background = shell::with(|s| std::mem::take(&mut s.popup_in_background)).unwrap_or(false);
            let position = shell::with(|s| s.new_tab_position(true)).unwrap_or(Position::Next);
            tabs::add_view(popup.clone(), position, !background);
            1
        }
    }
}

// CEF only supports Chrome-style DevTools; an Alloy-style one aborts.
wrap_window_delegate! {
    struct DevToolsWindowDelegate {
        view: BrowserView,
        // The tab being inspected, for `:devtools-focus`.
        inspected: i32,
    }

    impl ViewDelegate {
        fn preferred_size(&self, _view: Option<&mut View>) -> Size {
            Size { width: 1100, height: 750 }
        }
    }

    impl PanelDelegate {}

    impl WindowDelegate {
        fn on_window_created(&self, window: Option<&mut Window>) {
            let Some(window) = window else { return };
            window.set_to_fill_layout();
            window.add_child_view(Some(&mut View::from(&self.view)));
            // Chrome-style windows ignore the preferred size.
            window.center_window(Some(&Size { width: 1100, height: 750 }));
            window.set_title(Some(&CefString::from("DevTools - Riptide")));
            window.show();
        }

        fn can_close(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int {
            let closing = self
                .view
                .browser()
                .and_then(|b| b.host())
                .is_none_or(|h| h.try_close_browser() != 0);
            if closing {
                DEVTOOLS_WINDOWS.with(|d| d.borrow_mut().remove(&self.inspected));
            }
            closing.into()
        }

        fn window_runtime_style(&self) -> RuntimeStyle {
            RuntimeStyle::CHROME
        }
    }
}

wrap_browser_view_delegate! {
    struct DevToolsViewDelegate {}

    impl ViewDelegate {}

    impl BrowserViewDelegate {
        fn browser_runtime_style(&self) -> RuntimeStyle {
            RuntimeStyle::CHROME
        }
    }
}

/// One tab per command-line URL, or `url.start_pages` if there were none.
fn open_start_tabs(urls: &[String]) {
    let urls = shell::with(|s| {
        if urls.is_empty() {
            s.engine.settings().list("url.start_pages").to_vec()
        } else {
            urls.iter().map(|u| s.fuzzy_url(u)).collect()
        }
    })
    .unwrap_or_default();
    let urls = if urls.is_empty() {
        vec!["about:blank".to_string()]
    } else {
        urls
    };
    for (i, url) in urls.iter().enumerate() {
        tabs::open(url, Position::Last, i == 0);
    }
}

/// Non-empty sizes: a zero dimension counts as "unset" and falls back to the
/// browser's large default, which squeezes the page out of the layout.
fn bar_size(role: Role) -> Size {
    match role {
        Role::Statusbar => Size {
            width: 1,
            height: STATUSBAR_HEIGHT,
        },
        Role::Tabbar => {
            let (width, height) = TABBAR_SIZE.with(Cell::get);
            Size { width, height }
        }
        _ => Size {
            width: 1,
            height: 1,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placement(tabs: &str, width: i64, statusbar: &str) -> BarPlacement {
        BarPlacement {
            tabs: tabs.into(),
            tabs_width: width,
            statusbar: statusbar.into(),
        }
    }

    #[test]
    fn tab_bar_slots() {
        assert_eq!(
            placement("top", 200, "bottom").tab_bar_slot(),
            TabBarSlot::Top
        );
        assert_eq!(
            placement("bottom", 200, "bottom").tab_bar_slot(),
            TabBarSlot::Bottom
        );
        assert_eq!(
            placement("left", 200, "bottom").tab_bar_slot(),
            TabBarSlot::Left
        );
        assert_eq!(
            placement("right", 200, "bottom").tab_bar_slot(),
            TabBarSlot::Right
        );
        assert_eq!(
            placement("sideways", 200, "bottom").tab_bar_slot(),
            TabBarSlot::Top
        );
    }

    #[test]
    fn a_side_tab_bar_is_tabs_width_wide_and_a_top_one_is_a_row() {
        assert_eq!(placement("left", 250, "bottom").tab_bar_size(), (250, 1));
        assert_eq!(placement("right", 0, "bottom").tab_bar_size(), (1, 1));
        assert_eq!(
            placement("right", i64::MAX, "bottom").tab_bar_size(),
            (i32::MAX, 1)
        );
        assert_eq!(
            placement("top", 250, "bottom").tab_bar_size(),
            (1, TABBAR_HEIGHT)
        );
        assert_eq!(
            placement("bottom", 250, "bottom").tab_bar_size(),
            (1, TABBAR_HEIGHT)
        );
    }

    #[test]
    fn defaults_put_tabs_on_top_and_the_status_bar_below() {
        let defaults = BarPlacement::from_settings(&rt_core::settings::Settings::default());
        assert_eq!(defaults.tab_bar_slot(), TabBarSlot::Top);
        assert!(!defaults.statusbar_on_top());
        assert!(placement("top", 200, "top").statusbar_on_top());
    }
}
