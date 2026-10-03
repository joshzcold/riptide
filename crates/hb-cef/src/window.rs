//! The top-level window: tab bar, page area and status bar stacked
//! vertically, plus a completion overlay that floats above the status bar.

use cef::*;
use hb_core::tabs::Position;

use crate::client::{HbClient, Role};
use crate::shell;
use crate::storage::{self, DEFAULT_SESSION};
use crate::tabs;
use crate::ui;

pub const STATUSBAR_HEIGHT: i32 = 20;
pub const TABBAR_HEIGHT: i32 = 20;
const CHROME_BACKGROUND: u32 = 0xFF00_0000;

/// Open a window with one tab per URL, or `url.start_pages` if none.
/// `commands` (from the command line) run once the tabs are open. The first
/// window may restore the saved session instead.
pub fn create(urls: Vec<String>, commands: Vec<String>, private: bool) {
    open(urls, commands, private, None);
}

/// A window for one window of a saved session.
pub fn create_from_session(window: hb_storage::WindowState) {
    open(Vec::new(), Vec::new(), false, Some(window));
}

fn open(
    urls: Vec<String>,
    commands: Vec<String>,
    private: bool,
    session: Option<hb_storage::WindowState>,
) {
    let Some(id) = shell::with(|s| s.new_window(private)) else {
        return;
    };
    let mut delegate = HbWindowDelegate::new(id, urls, commands, session);
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
    let mut client = HbClient::new(role);
    let settings = BrowserSettings {
        background_color: if role == Role::Tab {
            0xFFFF_FFFF
        } else {
            CHROME_BACKGROUND
        },
        ..Default::default()
    };
    let mut delegate = HbBrowserViewDelegate::new(role);
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
    struct HbWindowDelegate {
        id: u32,
        urls: Vec<String>,
        commands: Vec<String>,
        session: Option<hb_storage::WindowState>,
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
            let (Some(tabbar), Some(content), Some(statusbar), Some(completion)) = (
                create_browser_view(Role::Tabbar, ui::TABBAR_URL),
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
            let mut tabbar_view = View::from(&tabbar);
            tabbar_view.set_focusable(0);
            window.add_child_view(Some(&mut tabbar_view));

            // All tab views share the content panel; only the current one is visible.
            content.set_to_fill_layout();
            let mut content_view = View::from(&content);
            window.add_child_view(Some(&mut content_view));
            if let Some(layout) = layout {
                layout.set_flex_for_view(Some(&mut content_view), 1);
            }

            let mut statusbar_view = View::from(&statusbar);
            statusbar_view.set_focusable(0);
            window.add_child_view(Some(&mut statusbar_view));

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
                s.statusbar = Some(statusbar.clone());
                s.completion = Some(completion.clone());
                s.overlay = overlay;
                s.windows.len() == 1
            })
            .unwrap_or(false);

            window.show();
            if let Some(session) = &self.session {
                tabs::restore_window(session);
            } else {
                let crashed = if first { storage::crashed_session() } else { None };
                let recovered = match crashed {
                    Some(session) if self.urls.is_empty() => {
                        tabs::restore(&session);
                        shell::show_message(hb_core::engine::Level::Info, "Restored the tabs open before the crash");
                        true
                    }
                    Some(_) => {
                        shell::show_message(
                            hb_core::engine::Level::Info,
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
                    && match storage::load_session(DEFAULT_SESSION) {
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

        fn can_close(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int {
            // Let every page run its unload handlers; CEF closes the window
            // once all of them agree. CEF may ask again while pages unload,
            // so save the session only the first time, and only when the
            // last window closes (`:quit` saves all windows itself).
            let id = self.id;
            let save = shell::with(|s| {
                let index = s.window_index(id)?;
                let first = !s.windows[index].window_closing;
                s.windows[index].window_closing = true;
                let last = s.windows.iter().filter(|w| !w.window_closing).count() == 0;
                Some(first && last && !s.quitting && s.engine.settings().bool("auto_save.session"))
            })
            .flatten()
            .unwrap_or(false);
            if save && let Err(e) = storage::save_session(DEFAULT_SESSION) {
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
    struct HbBrowserViewDelegate {
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
            Some(HbBrowserViewDelegate::new(Role::Tab))
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
                let mut delegate = DevToolsWindowDelegate::new(popup.clone());
                window_create_top_level(Some(&mut delegate));
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
            window.set_title(Some(&CefString::from("DevTools - hackers-browser")));
            window.show();
        }

        fn can_close(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int {
            self.view
                .browser()
                .and_then(|b| b.host())
                .is_none_or(|h| h.try_close_browser() != 0)
                .into()
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
        Role::Tabbar => Size {
            width: 1,
            height: TABBAR_HEIGHT,
        },
        _ => Size {
            width: 1,
            height: 1,
        },
    }
}
