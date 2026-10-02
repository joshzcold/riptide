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

/// Open the main window with one tab per URL, or `url.start_pages` if none.
/// `commands` (from the command line) run once the tabs are open.
pub fn create(urls: Vec<String>, commands: Vec<String>) {
    let mut delegate = HbWindowDelegate::new(urls, commands);
    window_create_top_level(Some(&mut delegate));
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
    browser_view_create(
        Some(&mut client),
        Some(&CefString::from(url)),
        Some(&settings),
        None,
        None,
        Some(&mut delegate),
    )
}

wrap_window_delegate! {
    struct HbWindowDelegate {
        urls: Vec<String>,
        commands: Vec<String>,
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

            shell::with(|s| {
                s.window = Some(window.clone());
                s.content = Some(content.clone());
                s.tabbar = Some(tabbar.clone());
                s.statusbar = Some(statusbar.clone());
                s.completion = Some(completion.clone());
                s.overlay = overlay;
            });

            window.show();
            let restore = self.urls.is_empty()
                && shell::with(|s| s.engine.settings().bool("auto_save.session")).unwrap_or(false);
            let restored = restore
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
            crate::remote::run_commands(&self.commands);
        }

        fn on_window_destroyed(&self, _window: Option<&mut Window>) {
            // Dropping the last view reference closes its browser synchronously,
            // which re-enters the shell, so drop them outside the borrow.
            let views = shell::with(|s| {
                let tabs = std::mem::take(&mut s.tabs);
                (
                    tabs,
                    s.overlay.take(),
                    s.completion.take(),
                    s.statusbar.take(),
                    s.tabbar.take(),
                    s.content.take(),
                    s.window.take(),
                )
            });
            drop(views);
        }

        fn can_close(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int {
            // Let every page run its unload handlers; CEF closes the window
            // once all of them agree.
            // CEF may ask again while pages unload; save the session only the first time.
            let save = shell::with(|s| {
                let first = !s.window_closing;
                s.window_closing = true;
                first && (s.save_session_on_quit || s.engine.settings().bool("auto_save.session"))
            })
            .unwrap_or(false);
            if save && let Err(e) = storage::save_session(DEFAULT_SESSION) {
                tracing::warn!("could not save session: {e}");
            }
            let hosts: Vec<BrowserHost> = shell::with(|s| s.tabs.iter().filter_map(|t| t.browser()?.host()).collect())
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
            _is_devtools: ::std::os::raw::c_int,
        ) -> Option<BrowserViewDelegate> {
            Some(HbBrowserViewDelegate::new(Role::Tab))
        }

        // Popups become tabs. Letting CEF create them (rather than opening the
        // URL ourselves) keeps `window.opener`, which login flows rely on.
        fn on_popup_browser_view_created(
            &self,
            _browser_view: Option<&mut BrowserView>,
            popup_browser_view: Option<&mut BrowserView>,
            _is_devtools: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            let Some(popup) = popup_browser_view else { return 0 };
            let background = shell::with(|s| std::mem::take(&mut s.popup_in_background)).unwrap_or(false);
            let position = shell::with(|s| s.new_tab_position(true)).unwrap_or(Position::Next);
            tabs::add_view(popup.clone(), position, !background);
            1
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
