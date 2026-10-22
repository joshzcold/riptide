//! An extension's toolbar popup, shown as Chrome shows it: a box over the
//! top right of the page, sized to its content, with a bar above it naming
//! the extension and a close button. Escape, switching tabs or opening
//! another popup closes it. One popup at a time. A plugin's page in a float
//! (`rt.ui.page { where = "float" }`) is shown the same way, and may cover
//! part of the page (`over`) or take every key (`keys = "all"`).

use std::cell::RefCell;

use cef::*;
use rt_config::lua::PageRect;

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
    /// The size was given, so the page isn't measured.
    fixed: bool,
    /// Where it was dragged to, from its usual place at the top right.
    offset: (i32, i32),
    /// The part of `tab`'s page it covers, instead of the top right.
    over: Option<(PageRect, BrowserView)>,
    /// Every key goes to it, riptide's bindings and Escape included.
    all_keys: bool,
    /// Which opening this is, so timers of an older popup stop.
    opened: u32,
    /// `tab`'s scroll position when `over` was measured, and how far it has
    /// scrolled since, in CSS pixels: the popup follows the part it covers.
    scroll_from: Option<(f64, f64)>,
    scrolled: (f64, f64),
    /// Reads in a row that found the page where it was.
    still: u32,
    /// The plugin page it shows (plugin, page id), told where it's dragged.
    plugin_page: Option<(String, u32)>,
    /// Drags so far, so only the last one of a run is told.
    moves: u32,
    /// Resized by its page, so `size` holds even over part of a page.
    resized: bool,
}

/// Where a popup goes and what it takes; by default the top right, sized to
/// its content, with Escape closing it.
#[derive(Default)]
pub struct Placement {
    pub size: Option<(i32, i32)>,
    pub over: Option<PageRect>,
    /// From where `over` puts it, in CSS pixels, as `on_move` reported.
    pub offset: Option<(i32, i32)>,
    pub all_keys: bool,
    /// The plugin page it shows, told where it's dragged.
    pub plugin_page: Option<(String, u32)>,
}

/// How often a popup over part of a page reads where the page has scrolled:
/// often while it scrolls, less once it has been still for `STILL_READS`.
const FOLLOW_MS: i64 = 40;
const FOLLOW_STILL_MS: i64 = 250;
const STILL_READS: u32 = 25;
/// A drag is told once the bar has been still this long.
const MOVED_MS: i64 = 400;
/// Where a popup over part of the page waits while that part is scrolled away.
const OFF_SCREEN: i32 = 100_000;

thread_local! {
    static POPUP: RefCell<Option<Popup>> = const { RefCell::new(None) };
    static OPENED: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    /// Where the last popup was left, for the next one.
    static LAST_OFFSET: std::cell::Cell<(i32, i32)> = const { std::cell::Cell::new((0, 0)) };
}

/// Show `url`, an extension's popup page, for the current window.
pub fn open(title: &str, url: &str) {
    open_with(title, url, Placement::default());
}

/// [`open`], placed as `placement` says.
pub fn open_with(title: &str, url: &str, placement: Placement) {
    let Placement {
        size,
        over,
        offset,
        all_keys,
        plugin_page,
    } = placement;
    let over = over.and_then(|rect| {
        let tab = shell::with(|s| s.tabs.current().map(|t| t.view.clone())).flatten()?;
        Some((rect, tab))
    });
    close();
    let opened = OPENED.with(|o| {
        o.set(o.get().wrapping_add(1));
        o.get()
    });
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
            size: size
                .map(|(w, h)| (w.clamp(MIN_WIDTH, MAX_SIZE.0), h.clamp(40, MAX_SIZE.1)))
                .unwrap_or(DEFAULT_SIZE),
            fixed: size.is_some() || over.is_some(),
            offset: match &over {
                Some((_, tab)) => {
                    let zoom = zoom_of(tab);
                    let (x, y) = offset.unwrap_or_default();
                    (scale(x, zoom), scale(y, zoom))
                }
                None => LAST_OFFSET.with(std::cell::Cell::get),
            },
            all_keys,
            opened,
            scroll_from: None,
            scrolled: (0.0, 0.0),
            still: 0,
            plugin_page,
            moves: 0,
            resized: false,
            over,
        })
    });
    reposition();
    follow_scroll(opened);
    // Overlays only get the keyboard from a click, so the tab keeps it and
    // passes keys on to a popup that takes them all.
    if all_keys {
        focus_tab();
    } else {
        View::from(&page).request_focus();
    }
}

fn focus_tab() {
    if let Some(tab) = shell::with(|s| s.tabs.current().map(|t| t.view.clone())).flatten() {
        View::from(&tab).request_focus();
    }
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
    focus_tab();
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

/// The bar has loaded: give it the extension's name, and the key that
/// closes it.
pub fn bar_loaded(browser: &Browser) {
    let title = POPUP.with(|p| {
        let p = p.borrow();
        let popup = p.as_ref()?;
        Some((popup.title.clone(), popup.all_keys))
    });
    if let (Some((title, all_keys)), Some(frame)) = (
        title.filter(|_| is_ours(browser, true)),
        browser.main_frame(),
    ) {
        let key = if all_keys { "Shift-Esc" } else { "Esc" };
        let json = serde_json::json!({ "title": title, "key": key });
        shell::exec_js(&frame, &format!("rtPopup({json})"));
    }
}

/// The popup page has loaded: size the box to what it draws. One that
/// takes every key gets them from the tab, which loading took the keyboard
/// from.
pub fn page_loaded(browser: &Browser) {
    if !is_ours(browser, false) {
        return;
    }
    if takes_all_keys() {
        focus_tab();
    }
    let fixed = POPUP.with(|p| p.borrow().as_ref().is_some_and(|p| p.fixed));
    if fixed {
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

/// Place the popup at the top right of its tab's page: the focused pane.
pub fn reposition() {
    let placed = POPUP.with(|p| {
        let p = p.borrow();
        let popup = p.as_ref()?;
        // Over part of the page, it's hidden while scrolled out of view.
        let mut shown = true;
        let (x, y, width, height) = match &popup.over {
            Some((rect, tab)) => {
                let (area, content) = shell::with(|s| {
                    let window = s.windows.iter().find(|w| w.id == popup.window)?;
                    Some((
                        View::from(window.row.as_ref()?).bounds(),
                        View::from(window.content.as_ref()?).bounds(),
                    ))
                })
                .flatten()?;
                let (x, y, width, height) = cover(&area, &content, rect, popup.scrolled, tab)?;
                let (width, height) = if popup.resized {
                    (
                        popup
                            .size
                            .0
                            .clamp(MIN_WIDTH.min(area.width), area.width.max(1)),
                        popup.size.1.clamp(40, (area.height - BAR_HEIGHT).max(40)),
                    )
                } else {
                    (width, height)
                };
                let (x, y) = (x + popup.offset.0, y + popup.offset.1);
                shown = y + BAR_HEIGHT + height > area.y
                    && y < area.y + area.height
                    && x + width > area.x
                    && x < area.x + area.width;
                let (x, y) = clamp_to(&area, (width, height), (x, y));
                (x, y, width, height)
            }
            None => {
                let area = crate::panes::focused_area(popup.window)?;
                let width = popup.size.0.min(area.width - 2 * MARGIN).max(1);
                let height = popup
                    .size
                    .1
                    .min(area.height - 2 * MARGIN - BAR_HEIGHT)
                    .max(1);
                let (x, y) = place(&area, (width, height), popup.offset);
                (x, y, width, height)
            }
        };
        Some((
            shown,
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
    let Some((shown, bar, bar_rect, page, page_rect)) = placed else {
        return;
    };
    for (overlay, rect) in [(bar, bar_rect), (page, page_rect)] {
        if let Some(overlay) = overlay.filter(|o| o.is_valid() != 0) {
            // Out of view, it's moved away rather than hidden: showing an
            // overlay again takes the keyboard from the tab.
            let rect = if shown {
                rect
            } else {
                Rect {
                    x: -OFF_SCREEN,
                    y: -OFF_SCREEN,
                    ..rect
                }
            };
            overlay.set_bounds(Some(&rect));
            if overlay.is_visible() == 0 {
                overlay.set_visible(1);
            }
        }
    }
}

/// The bar's top left: the top right of `area`, moved by `offset`, but
/// never outside it.
fn place(area: &Rect, (width, height): (i32, i32), offset: (i32, i32)) -> (i32, i32) {
    let x = area.x + area.width - width - RIGHT + offset.0;
    let y = area.y + MARGIN + offset.1;
    clamp_to(area, (width, height), (x, y))
}

/// The bar's top left at `(x, y)`, moved into `area`.
fn clamp_to(area: &Rect, (width, height): (i32, i32), (x, y): (i32, i32)) -> (i32, i32) {
    let x = x.clamp(area.x, (area.x + area.width - width).max(area.x));
    let y = y.clamp(
        area.y,
        (area.y + area.height - height - BAR_HEIGHT).max(area.y),
    );
    (x, y)
}

/// The bar's top left and the page's size to cover `rect` of `tab`'s page,
/// in the window's pixels: the bar sits just above it. Tabs are laid out in
/// `content`, which is laid out in the page area.
fn cover(
    area: &Rect,
    content: &Rect,
    rect: &PageRect,
    scrolled: (f64, f64),
    tab: &BrowserView,
) -> Option<(i32, i32, i32, i32)> {
    let zoom = 1.2_f64.powf(tab.browser()?.host()?.zoom_level());
    let pane = View::from(tab).bounds();
    let origin = (area.x + content.x + pane.x, area.y + content.y + pane.y);
    let width = scale(rect.width, zoom).clamp(MIN_WIDTH.min(area.width), area.width.max(1));
    let height = scale(rect.height, zoom).clamp(40, (area.height - BAR_HEIGHT).max(40));
    let x = ((f64::from(rect.x) - scrolled.0) * zoom).round() as i32;
    let y = ((f64::from(rect.y) - scrolled.1) * zoom).round() as i32;
    Some((origin.0 + x, origin.1 + y - BAR_HEIGHT, width, height))
}

/// CSS pixels at `zoom`, in the window's pixels.
fn scale(v: i32, zoom: f64) -> i32 {
    (f64::from(v) * zoom).round() as i32
}

fn zoom_of(tab: &BrowserView) -> f64 {
    tab.browser()
        .and_then(|b| b.host())
        .map_or(1.0, |host| 1.2_f64.powf(host.zoom_level()))
}

/// While popup `opened` covers part of its tab's page, follow the page's
/// scrolling. Pages can't tell us they scrolled, so it's read every few
/// milliseconds while it scrolls, as the status bar's `scroll` widget does
/// more slowly, and less often once the page is still.
fn follow_scroll(opened: u32) {
    let tab = POPUP.with(|p| {
        let p = p.borrow();
        let popup = p.as_ref().filter(|p| p.opened == opened)?;
        popup.over.as_ref().map(|(_, tab)| tab.clone())
    });
    let Some(browser) = tab.and_then(|t| t.browser()) else {
        return;
    };
    crate::eval::eval(
        &browser,
        "JSON.stringify([scrollX, scrollY])",
        move |result| {
            let at = result
                .ok()
                .and_then(|json| serde_json::from_str::<[f64; 2]>(&json).ok());
            let read = at.and_then(|[x, y]| {
                POPUP.with(|p| {
                    let mut p = p.borrow_mut();
                    let popup = p.as_mut().filter(|p| p.opened == opened)?;
                    let from = *popup.scroll_from.get_or_insert((x, y));
                    let scrolled = (x - from.0, y - from.1);
                    let moved = scrolled != popup.scrolled;
                    popup.scrolled = scrolled;
                    popup.still = if moved {
                        0
                    } else {
                        popup.still.saturating_add(1)
                    };
                    Some((moved, popup.still))
                })
            });
            if read.is_some_and(|(moved, _)| moved) {
                reposition();
            }
            let still = read.is_some_and(|(_, still)| still >= STILL_READS);
            let wait = if still { FOLLOW_STILL_MS } else { FOLLOW_MS };
            crate::client::later(wait, move || follow_scroll(opened));
        },
    );
}

/// Tell the plugin page where its popup was dragged to, once the drag is over.
fn report_move(opened: u32, moves: u32) {
    let report = POPUP.with(|p| {
        let p = p.borrow();
        let popup = p
            .as_ref()
            .filter(|p| p.opened == opened && p.moves == moves)?;
        let (source, id) = popup.plugin_page.clone()?;
        let zoom = popup.over.as_ref().map_or(1.0, |(_, tab)| zoom_of(tab));
        let (x, y) = popup.offset;
        let css = |v: i32| (f64::from(v) / zoom).round() as i32;
        Some((source, id, css(x), css(y)))
    });
    if let Some((source, id, x, y)) = report {
        crate::pages::moved(&source, id, x, y);
    }
}

/// The popup takes every key: riptide's bindings and Escape don't apply.
pub fn takes_all_keys() -> bool {
    POPUP.with(|p| p.borrow().as_ref().is_some_and(|p| p.all_keys))
}

/// Hand `key` to a popup that takes every key, as the `keydown` event the
/// page would see. `mods` are the keys held, shift included.
pub fn type_key(key: &rt_core::key::Key, mods: rt_core::key::Modifiers) {
    use rt_core::key::KeyCode;
    let name = match key.code {
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Escape => "Escape".into(),
        KeyCode::Enter => "Enter".into(),
        KeyCode::Backspace => "Backspace".into(),
        KeyCode::Delete => "Delete".into(),
        KeyCode::Tab => "Tab".into(),
        KeyCode::Insert => "Insert".into(),
        KeyCode::Home => "Home".into(),
        KeyCode::End => "End".into(),
        KeyCode::PageUp => "PageUp".into(),
        KeyCode::PageDown => "PageDown".into(),
        KeyCode::Up => "ArrowUp".into(),
        KeyCode::Down => "ArrowDown".into(),
        KeyCode::Left => "ArrowLeft".into(),
        KeyCode::Right => "ArrowRight".into(),
        KeyCode::F(n) => format!("F{n}"),
    };
    let init = serde_json::json!({
        "key": name,
        "ctrlKey": mods.ctrl,
        "altKey": mods.alt,
        "shiftKey": mods.shift,
        "metaKey": mods.meta,
        "bubbles": true,
        "cancelable": true,
    });
    let frame = all_keys_host()
        .and_then(|host| host.browser())
        .and_then(|b| b.main_frame());
    if let Some(frame) = frame {
        shell::exec_js(
            &frame,
            &format!("document.dispatchEvent(new KeyboardEvent('keydown', {init}))"),
        );
    }
}

/// The browser of a popup that takes every key, in the focused window.
pub fn all_keys_host() -> Option<BrowserHost> {
    let window = shell::with(|s| s.id)?;
    POPUP.with(|p| {
        let p = p.borrow();
        let popup = p.as_ref().filter(|p| p.all_keys && p.window == window)?;
        popup.page.browser()?.host()
    })
}

/// `page:resize`: the popup's page at this size, in its place.
pub fn resize(width: i32, height: i32) {
    let resized = POPUP.with(|p| {
        let mut p = p.borrow_mut();
        let popup = p.as_mut()?;
        popup.size = (width.max(MIN_WIDTH), height.max(40));
        popup.fixed = true;
        popup.resized = true;
        Some(())
    });
    if resized.is_some() {
        reposition();
    }
}

/// The bar was dragged by `dx`, `dy` pixels.
pub fn move_by(dx: i32, dy: i32) {
    let moved = POPUP.with(|p| {
        let mut p = p.borrow_mut();
        let popup = p.as_mut()?;
        popup.offset = (popup.offset.0 + dx, popup.offset.1 + dy);
        if popup.over.is_none() {
            LAST_OFFSET.with(|l| l.set(popup.offset));
        }
        popup.moves = popup.moves.wrapping_add(1);
        Some((popup.opened, popup.moves))
    });
    if let Some((opened, moves)) = moved {
        reposition();
        crate::client::later(MOVED_MS, move || report_move(opened, moves));
    }
}

/// The popup page's URL and main frame, if one is open.
pub fn page_frame() -> Option<(String, Frame)> {
    POPUP.with(|p| {
        let frame = p.borrow().as_ref()?.page.browser()?.main_frame()?;
        Some((CefString::from(&frame.url()).to_string(), frame))
    })
}

/// Escape in normal mode closes an open popup in the focused window, unless
/// the popup takes every key.
pub fn forward_key(key: &rt_core::key::Key) -> bool {
    let escape = key.code == rt_core::key::KeyCode::Escape && key.mods.is_empty();
    if !escape {
        return false;
    }
    let here = shell::with(|s| {
        (s.engine.mode() == rt_core::Mode::Normal)
            .then_some(s.id)
            .is_some_and(|id| {
                POPUP.with(|p| {
                    p.borrow()
                        .as_ref()
                        .is_some_and(|p| p.window == id && !p.all_keys)
                })
            })
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
        p.borrow()
            .as_ref()
            .map_or(serde_json::Value::Null, |popup| {
                let url = popup
                    .page
                    .browser()
                    .and_then(|b| b.main_frame())
                    .map(|f| CefString::from(&f.url()).to_string())
                    .unwrap_or_default();
                let bounds = popup
                    .page_overlay
                    .as_ref()
                    .filter(|o| o.is_valid() != 0)
                    .map(|o| o.bounds())
                    .map(|b| [b.x, b.y, b.width, b.height]);
                serde_json::json!({
                    "title": popup.title,
                    "url": url,
                    "size": [popup.size.0, popup.size.1],
                    "bounds": bounds,
                    "all_keys": popup.all_keys,
                })
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dragged_popup_stays_in_the_page_area() {
        let area = Rect {
            x: 0,
            y: 30,
            width: 1000,
            height: 700,
        };
        let size = (300, 200);
        // Its usual place: the top right.
        assert_eq!(
            place(&area, size, (0, 0)),
            (1000 - 300 - RIGHT, 30 + MARGIN)
        );
        // Dragged left and down.
        assert_eq!(
            place(&area, size, (-400, 100)),
            (1000 - 300 - RIGHT - 400, 30 + MARGIN + 100)
        );
        // Dragged too far: held at the edges.
        assert_eq!(place(&area, size, (-5000, -5000)), (0, 30));
        assert_eq!(
            place(&area, size, (5000, 5000)),
            (700, 30 + 700 - 200 - BAR_HEIGHT)
        );
    }
}
