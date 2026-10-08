//! An extension's toolbar popup, shown as Chrome shows it: a box over the
//! top right of the page, sized to its content, with a bar above it naming
//! the extension and a close button. Escape, switching tabs or opening
//! another popup closes it. One popup at a time.

use std::cell::RefCell;

use cef::*;

use crate::client::Role;
use crate::shell;

pub const BAR_URL: &str = "riptide://ui/popup.html";

/// The bar's height, and the largest a popup may be, as in Chrome.
const BAR_HEIGHT: i32 = 26;
const MAX_SIZE: (i32, i32) = (800, 600);
/// Until the popup has measured itself.
const DEFAULT_SIZE: (i32, i32) = (360, 420);
const MARGIN: i32 = 12;
/// Room for the page's scroll bar on the right.
const RIGHT: i32 = 24;
/// Narrower, and the extension's name doesn't fit in the bar.
const MIN_WIDTH: i32 = 220;

struct Popup {
    window: u32,
    title: String,
    page: BrowserView,
    bar: BrowserView,
    page_overlay: Option<OverlayController>,
    bar_overlay: Option<OverlayController>,
    size: (i32, i32),
}

thread_local! {
    static POPUP: RefCell<Option<Popup>> = const { RefCell::new(None) };
}

/// Show `url`, an extension's popup page, for the current window.
pub fn open(title: &str, url: &str) {
    close();
    let Some((window_id, window)) = shell::with(|s| Some((s.id, s.window.clone()?))).flatten()
    else {
        return;
    };
    let (Some(page), Some(bar)) = (
        crate::window::create_browser_view(Role::Popup, url),
        crate::window::create_browser_view(Role::PopupBar, BAR_URL),
    ) else {
        return;
    };
    View::from(&bar).set_focusable(0);
    let mut bar_view = View::from(&bar);
    let bar_overlay = window.add_overlay_view(Some(&mut bar_view), DockingMode::CUSTOM, 0);
    let mut page_view = View::from(&page);
    let page_overlay = window.add_overlay_view(Some(&mut page_view), DockingMode::CUSTOM, 1);
    POPUP.with(|p| {
        *p.borrow_mut() = Some(Popup {
            window: window_id,
            title: title.to_string(),
            page: page.clone(),
            bar,
            page_overlay,
            bar_overlay,
            size: DEFAULT_SIZE,
        })
    });
    reposition();
    View::from(&page).request_focus();
}

/// Close the popup, if one is open.
pub fn close() {
    let Some(popup) = POPUP.with(|p| p.borrow_mut().take()) else {
        return;
    };
    for overlay in [popup.page_overlay, popup.bar_overlay]
        .into_iter()
        .flatten()
    {
        if overlay.is_valid() != 0 {
            overlay.destroy();
        }
    }
    // Keys go back to the page.
    if let Some(tab) = shell::with(|s| s.tabs.current().map(|t| t.view.clone())).flatten() {
        View::from(&tab).request_focus();
    }
}

fn is_ours(browser: &Browser, bar: bool) -> bool {
    POPUP.with(|p| {
        p.borrow().as_ref().is_some_and(|popup| {
            let view = if bar { &popup.bar } else { &popup.page };
            view.browser()
                .is_some_and(|b| b.identifier() == browser.identifier())
        })
    })
}

/// The bar has loaded: give it the extension's name.
pub fn bar_loaded(browser: &Browser) {
    let title = POPUP.with(|p| p.borrow().as_ref().map(|p| p.title.clone()));
    if let (Some(title), Some(frame)) = (
        title.filter(|_| is_ours(browser, true)),
        browser.main_frame(),
    ) {
        let json = serde_json::json!({ "title": title });
        shell::exec_js(&frame, &format!("rtPopup({json})"));
    }
}

/// The popup page has loaded: size the box to what it draws.
pub fn page_loaded(browser: &Browser) {
    if !is_ours(browser, false) {
        return;
    }
    // Its natural size, not the box's: measured at max-content width.
    let code = "(() => { const d = document.documentElement; const width = d.style.width; \
                d.style.width = 'max-content'; const r = d.getBoundingClientRect(); \
                d.style.width = width; return JSON.stringify([Math.ceil(r.width), Math.ceil(r.height)]); })()";
    crate::eval::eval(browser, code, |result| {
        let Ok(text) = result else { return };
        let Ok([width, height]) = serde_json::from_str::<[i32; 2]>(&text) else {
            return;
        };
        let size = (
            width.clamp(MIN_WIDTH, MAX_SIZE.0),
            height.clamp(40, MAX_SIZE.1),
        );
        POPUP.with(|p| {
            if let Some(popup) = p.borrow_mut().as_mut() {
                popup.size = size;
            }
        });
        reposition();
    });
}

/// Place the popup at the top right of its window's page area.
pub fn reposition() {
    let placed = POPUP.with(|p| {
        let p = p.borrow();
        let popup = p.as_ref()?;
        let area = shell::with(|s| {
            let window = s.windows.iter().find(|w| w.id == popup.window)?;
            Some(View::from(window.row.as_ref()?).bounds())
        })
        .flatten()?;
        let width = popup.size.0.min(area.width - 2 * MARGIN).max(1);
        let height = popup
            .size
            .1
            .min(area.height - 2 * MARGIN - BAR_HEIGHT)
            .max(1);
        let x = area.x + area.width - width - RIGHT;
        let y = area.y + MARGIN;
        Some((
            popup.bar_overlay.clone(),
            Rect {
                x,
                y,
                width,
                height: BAR_HEIGHT,
            },
            popup.page_overlay.clone(),
            Rect {
                x,
                y: y + BAR_HEIGHT,
                width,
                height,
            },
        ))
    });
    let Some((bar, bar_rect, page, page_rect)) = placed else {
        return;
    };
    for (overlay, rect) in [(bar, bar_rect), (page, page_rect)] {
        if let Some(overlay) = overlay.filter(|o| o.is_valid() != 0) {
            overlay.set_bounds(Some(&rect));
            overlay.set_visible(1);
        }
    }
}

/// Escape in normal mode closes an open popup in the focused window.
pub fn forward_key(key: &rt_core::key::Key) -> bool {
    let escape = key.code == rt_core::key::KeyCode::Escape && key.mods.is_empty();
    if !escape {
        return false;
    }
    let here = shell::with(|s| {
        (s.engine.mode() == rt_core::Mode::Normal)
            .then_some(s.id)
            .is_some_and(|id| POPUP.with(|p| p.borrow().as_ref().is_some_and(|p| p.window == id)))
    })
    .unwrap_or(false);
    if here {
        close();
    }
    here
}

/// What the e2e tests see: the popup's title and URL, or null.
pub fn test_state() -> serde_json::Value {
    POPUP.with(|p| {
        p.borrow().as_ref().map_or(serde_json::Value::Null, |popup| {
            let url = popup
                .page
                .browser()
                .and_then(|b| b.main_frame())
                .map(|f| CefString::from(&f.url()).to_string())
                .unwrap_or_default();
            serde_json::json!({ "title": popup.title, "url": url, "size": [popup.size.0, popup.size.1] })
        })
    })
}
