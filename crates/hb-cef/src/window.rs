//! The top-level window: a page area above a status bar, plus a completion
//! overlay that floats above the status bar while typing a command.

use cef::*;

use crate::client::{HbClient, Role};
use crate::shell;
use crate::ui;

pub const STATUSBAR_HEIGHT: i32 = 20;
const CHROME_BACKGROUND: u32 = 0xFF00_0000;

pub fn create(start_url: String) {
    let mut delegate = HbWindowDelegate::new(start_url);
    window_create_top_level(Some(&mut delegate));
}

fn create_browser_view(role: Role, url: &str) -> Option<BrowserView> {
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
        start_url: String,
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
            let (Some(tab), Some(statusbar), Some(completion)) = (
                create_browser_view(Role::Tab, &self.start_url),
                create_browser_view(Role::Statusbar, &ui::data_uri(ui::STATUSBAR_HTML)),
                create_browser_view(Role::Completion, &ui::data_uri(ui::COMPLETION_HTML)),
            ) else {
                tracing::error!("failed to create browser views");
                return;
            };

            let layout = window.set_to_box_layout(Some(&BoxLayoutSettings {
                horizontal: 0,
                cross_axis_alignment: AxisAlignment::STRETCH,
                ..Default::default()
            }));
            let mut tab_view = View::from(&tab);
            window.add_child_view(Some(&mut tab_view));
            if let Some(layout) = layout {
                layout.set_flex_for_view(Some(&mut tab_view), 1);
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
                s.tab = Some(tab.clone());
                s.statusbar = Some(statusbar.clone());
                s.completion = Some(completion.clone());
                s.overlay = overlay;
            });

            window.show();
            tab_view.request_focus();
        }

        fn on_window_destroyed(&self, _window: Option<&mut Window>) {
            // Dropping the last view reference closes its browser synchronously,
            // which re-enters the shell, so drop them outside the borrow.
            let views = shell::with(|s| {
                (s.overlay.take(), s.completion.take(), s.statusbar.take(), s.tab.take(), s.window.take())
            });
            drop(views);
        }

        fn can_close(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int {
            // Let the page run its unload handlers first; CEF closes the window afterwards.
            match shell::with(|s| s.tab_browser()).flatten().and_then(|b| b.host()) {
                Some(host) => host.try_close_browser(),
                None => 1,
            }
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
        // A zero width or height counts as "unset", which falls back to the
        // browser's large default and squeezes the page out of the layout.
        fn preferred_size(&self, _view: Option<&mut View>) -> Size {
            match self.role {
                Role::Statusbar => Size { width: 1, height: STATUSBAR_HEIGHT },
                _ => Size { width: 1, height: 1 },
            }
        }

        fn minimum_size(&self, _view: Option<&mut View>) -> Size {
            match self.role {
                Role::Statusbar => Size { width: 1, height: STATUSBAR_HEIGHT },
                _ => Size { width: 1, height: 1 },
            }
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
    }
}
