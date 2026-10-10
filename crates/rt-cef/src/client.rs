use std::cell::Cell;

use cef::*;
use rt_core::Modifiers;
use rt_core::engine::Level;
use rt_core::vk::{self, RawKey};

use crate::renderer::{EVAL_RESULT_MESSAGE, FOCUS_MESSAGE, UI_MESSAGE};
use crate::{eval, shell, storage, tabs, ui};

#[cfg(target_os = "linux")]
type OsEvent = sys::XEvent;
#[cfg(windows)]
type OsEvent = sys::MSG;

// `cef_event_flags_t` values. The generated enum is `u32` on Linux and macOS
// but `i32` on Windows, so plain constants keep the bit tests portable.
const EVENTFLAG_SHIFT_DOWN: u32 = 1 << 1;
const EVENTFLAG_CONTROL_DOWN: u32 = 1 << 2;
const EVENTFLAG_ALT_DOWN: u32 = 1 << 3;
const EVENTFLAG_COMMAND_DOWN: u32 = 1 << 7;

/// Set when the last browser closed and riptide ended the message loop.
/// Anything else ending it isn't a clean quit.
pub static CLOSED_EVERY_BROWSER: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Which part of the window a browser belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Tab,
    Tabbar,
    Statusbar,
    Completion,
    /// The notice shown in place of a tab whose renderer died.
    Crashed,
    /// An `rt.ui.float`.
    Float,
    /// An `rt.ui.panel`.
    Panel,
    /// A plugin page in a panel: keys and focus work as in a tab.
    PanelPage,
    /// An extension's popup page (`popup.rs`): keys and focus as in a panel page.
    Popup,
    /// The bar above an extension's popup, with its close button.
    PopupBar,
}

wrap_client! {
    pub struct RtClient {
        role: Role,
    }

    impl Client {
        fn keyboard_handler(&self) -> Option<KeyboardHandler> {
            Some(RtKeyboardHandler::new(self.role))
        }

        fn display_handler(&self) -> Option<DisplayHandler> {
            (self.role == Role::Tab).then(RtDisplayHandler::new)
        }

        fn load_handler(&self) -> Option<LoadHandler> {
            Some(RtLoadHandler::new(self.role))
        }

        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(RtLifeSpanHandler::new(self.role))
        }

        fn jsdialog_handler(&self) -> Option<JsdialogHandler> {
            (self.role == Role::Tab).then(crate::dialogs::RtJsdialogHandler::new)
        }

        fn request_handler(&self) -> Option<RequestHandler> {
            (self.role == Role::Tab).then(crate::dialogs::RtRequestHandler::new)
        }

        fn dialog_handler(&self) -> Option<DialogHandler> {
            (self.role == Role::Tab).then(crate::fileselect::RtDialogHandler::new)
        }

        fn download_handler(&self) -> Option<DownloadHandler> {
            (self.role == Role::Tab).then(crate::downloads::RtDownloadHandler::new)
        }

        fn find_handler(&self) -> Option<FindHandler> {
            (self.role == Role::Tab).then(crate::search::RtFindHandler::new)
        }

        fn context_menu_handler(&self) -> Option<ContextMenuHandler> {
            (self.role == Role::Tab).then(crate::spell::RtContextMenuHandler::new)
        }

        fn permission_handler(&self) -> Option<PermissionHandler> {
            (self.role == Role::Tab).then(crate::permissions::RtPermissionHandler::new)
        }

        fn on_process_message_received(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            _source_process: ProcessId,
            message: Option<&mut ProcessMessage>,
        ) -> ::std::os::raw::c_int {
            let Some(message) = message else { return 0 };
            let name = CefString::from(&message.name()).to_string();
            if name == EVAL_RESULT_MESSAGE {
                if let Some(args) = message.argument_list() {
                    eval::complete(args.int(0), args.bool(1) != 0, CefString::from(&args.string(2)).to_string());
                }
                return 1;
            }
            if name == crate::renderer::NOTIFICATION_MESSAGE {
                let text = |i| message.argument_list().map(|a| CefString::from(&a.string(i)).to_string()).unwrap_or_default();
                let (title, body) = (text(0), text(1));
                let page = frame.as_ref().map(|f| CefString::from(&f.url()).to_string()).unwrap_or_default();
                let (show_origin, desktop) = shell::with(|s| {
                    let settings = s.engine.settings();
                    (settings.bool("content.notifications.show_origin"), settings.str("content.notifications.presenter") == "libnotify")
                })
                .unwrap_or((true, false));
                if desktop {
                    let origin = rt_core::url::origin(&page).filter(|_| show_origin).map(|o| o.trim_end_matches('/').to_string());
                    crate::notifications::show(browser.as_ref().map(|b| b.identifier()), origin, title, body);
                    return 1;
                }
                let origin = rt_core::url::origin(&page).filter(|_| show_origin).map(|o| format!("{} ", o.trim_end_matches('/'))).unwrap_or_default();
                let body = if body.is_empty() { String::new() } else { format!(": {body}") };
                shell::show_message(Level::Info, format!("{origin}{title}{body}"));
                return 1;
            }
            if name == crate::renderer::SHARE_MESSAGE {
                let surface = message.argument_list().map(|a| CefString::from(&a.string(0)).to_string()).unwrap_or_default();
                let started = shell::with_tab(browser, |s, index, _| {
                    let tab = s.tabs.get_mut(index)?;
                    let started = tab.sharing.is_none() && !surface.is_empty();
                    tab.sharing = (!surface.is_empty()).then(|| surface.clone());
                    started.then(|| rt_core::url::host(&tab.url).to_string())
                })
                .flatten();
                if let Some(site) = started {
                    let what = crate::view::share_name(&surface);
                    shell::show_message(Level::Info, format!("Sharing {what} with {site}. :share-stop stops it"));
                }
                shell::refresh_ui();
                return 1;
            }
            if name == crate::renderer::ROCKER_MESSAGE {
                let direction = message.argument_list().map(|a| CefString::from(&a.string(0)).to_string());
                let enabled = shell::with(|s| s.engine.settings().bool("input.mouse.rocker_gestures")).unwrap_or(false);
                let command = match direction.as_deref() {
                    Some("back") => "back",
                    Some("forward") => "forward",
                    _ => return 1,
                };
                // Only for the tab the gesture happened in, and only while it's on.
                let mut tab = browser.as_ref().map(|b| (**b).clone());
                if enabled
                    && let Some(Some(effects)) = shell::with_tab(tab.as_mut(), |s, _, current| {
                        current.then(|| s.engine.execute_str(command, None))
                    })
                {
                    shell::apply(effects);
                }
                return 1;
            }
            if name == crate::renderer::GM_XHR_MESSAGE {
                if let (Some(frame), Some(args)) = (frame.as_ref(), message.argument_list()) {
                    let text = |i| CefString::from(&args.string(i)).to_string();
                    let id = text(1).parse().unwrap_or(0);
                    crate::gm_api::xhr((*frame).clone(), &text(0), id, &text(2));
                }
                return 1;
            }
            if name == crate::renderer::GM_OPEN_MESSAGE {
                if let (Some(frame), Some(args)) = (frame.as_ref(), message.argument_list()) {
                    let text = |i| CefString::from(&args.string(i)).to_string();
                    crate::gm_api::open_in_tab(frame, &text(0), &text(1), !text(2).is_empty());
                }
                return 1;
            }
            if name == crate::renderer::GM_SET_MESSAGE {
                if let Some(args) = message.argument_list() {
                    let text = |i| CefString::from(&args.string(i)).to_string();
                    let value = text(2);
                    crate::greasemonkey::set_value(&text(0), &text(1), Some(value.as_str()).filter(|v| !v.is_empty()));
                }
                return 1;
            }
            if name == UI_MESSAGE {
                // Trust the browser process's view of the frame, not the page.
                let main = frame.as_ref().is_some_and(|f| f.is_main() != 0);
                let url = frame.map(|f| CefString::from(&f.url()).to_string()).unwrap_or_default();
                if let Some(plugin) = rt_core::ui_message::plugin_page(&url) {
                    if let Some(args) = message.argument_list() {
                        let name = CefString::from(&args.string(0)).to_string();
                        let payload = CefString::from(&args.string(1)).to_string();
                        crate::pages::message(plugin, &url, &name, &payload);
                    }
                    return 1;
                }
                // In a tab, only the settings page's own frame may send.
                let allowed = self.role != Role::Tab || (main && rt_core::ui_message::tab_may_send(&url));
                if allowed
                    && let Some(args) = message.argument_list()
                {
                    let name = CefString::from(&args.string(0)).to_string();
                    let payload = CefString::from(&args.string(1)).to_string();
                    match rt_core::ui_message::parse(&url, &name, &payload) {
                        Ok(message) => {
                            // A click in a window's tab bar is about that window; a
                            // page reporting its size isn't the user picking it.
                            if message.is_input()
                                && let Some(browser) = &browser
                            {
                                shell::activate_browser(browser.identifier());
                            }
                            crate::ui::handle_message(message)
                        }
                        Err(e) => tracing::warn!("rejected UI message: {e}"),
                    }
                }
                return 1;
            }
            if !matches!(self.role, Role::Tab | Role::PanelPage | Role::Popup) || name != FOCUS_MESSAGE {
                return 0;
            }
            let args = message.argument_list();
            let editable = args.as_ref().is_some_and(|a| a.bool(0) != 0);
            let user = args.as_ref().is_none_or(|a| a.bool(1) != 0);
            // A page panel isn't a tab: typing in it drives the mode directly.
            if matches!(self.role, Role::PanelPage | Role::Popup) {
                if user && let Some(effects) = shell::with(|s| s.engine.focus_changed(editable)) {
                    shell::apply(effects);
                }
                return 1;
            }
            // Background tabs can move focus too; only the visible one drives the mode.
            // Focus the page moved by itself (autofocus) counts only with auto_load.
            let effects = shell::with_tab(browser, |s, _, current| {
                let by_page = editable && !user && !s.engine.settings().bool("input.insert_mode.auto_load");
                if current && !by_page { s.engine.focus_changed(editable) } else { Vec::new() }
            });
            if let Some(effects) = effects {
                shell::apply(effects);
            }
            1
        }
    }
}

wrap_keyboard_handler! {
    struct RtKeyboardHandler {
        role: Role,
    }

    impl KeyboardHandler {
        // The native event's type differs per platform; on macOS it is a raw pointer.
        #[cfg(not(target_os = "macos"))]
        fn on_pre_key_event(
            &self,
            browser: Option<&mut Browser>,
            event: Option<&KeyEvent>,
            _os_event: Option<&mut OsEvent>,
            _is_keyboard_shortcut: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            // Keys go to the window they were typed in.
            if let Some(browser) = browser {
                HAD_KEYS.with(|h| h.borrow_mut().insert(browser.identifier()));
                shell::activate_browser(browser.identifier());
            }
            event.is_some_and(|e| route_key_event(self.role, e)).into()
        }

        #[cfg(target_os = "macos")]
        fn on_pre_key_event(
            &self,
            browser: Option<&mut Browser>,
            event: Option<&KeyEvent>,
            _os_event: *mut u8,
            _is_keyboard_shortcut: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            if let Some(browser) = browser {
                shell::activate_browser(browser.identifier());
            }
            event.is_some_and(|e| route_key_event(self.role, e)).into()
        }
    }
}

thread_local! {
    static SENDING_TO_PAGE: Cell<bool> = const { Cell::new(false) };
}

/// The CEF event for `key`, as the keyboard would deliver it.
fn key_event(key: rt_core::Key, type_: KeyEventType) -> KeyEvent {
    let raw = vk::to_raw(key);
    let flag = |on: bool, bit: u32| if on { bit } else { 0 };
    KeyEvent {
        type_,
        modifiers: flag(raw.mods.shift, EVENTFLAG_SHIFT_DOWN)
            | flag(raw.mods.ctrl, EVENTFLAG_CONTROL_DOWN)
            | flag(raw.mods.alt, EVENTFLAG_ALT_DOWN)
            | flag(raw.mods.meta, EVENTFLAG_COMMAND_DOWN),
        windows_key_code: raw.windows_key_code,
        character: raw.character,
        unmodified_character: raw.unmodified_character,
        ..Default::default()
    }
}

/// Press `key` as if typed: the engine sees it first, as in `OnPreKeyEvent`,
/// and the current tab gets it if the engine doesn't consume it.
#[cfg(any(debug_assertions, feature = "test-control"))]
pub fn press(key: rt_core::Key) {
    if !handle_key_event(&key_event(key, KeyEventType::RAWKEYDOWN)) {
        send_to_page(key);
    }
}

/// Type `key` into the current tab, as a macro replays it.
pub fn send_to_page(key: rt_core::Key) {
    let Some(host) = shell::with(|s| s.current_browser())
        .flatten()
        .and_then(|b| b.host())
    else {
        return;
    };
    send_key_to(&host, key);
}

thread_local! {
    /// Browsers that have had a key event, typed or sent.
    static HAD_KEYS: std::cell::RefCell<std::collections::HashSet<i32>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
}

/// Whether `browser` has had a key event yet (see [`prime_page`]).
pub fn had_keys(browser: i32) -> bool {
    HAD_KEYS.with(|h| h.borrow().contains(&browser))
}

/// A closure for [`later`], taken out when it runs.
type Pending = std::rc::Rc<std::cell::RefCell<Option<Box<dyn FnOnce()>>>>;

/// Run `f` on the UI thread after `ms` milliseconds.
pub fn later(ms: i64, f: impl FnOnce() + 'static) {
    let mut task = Later::new(std::rc::Rc::new(std::cell::RefCell::new(Some(Box::new(f)))));
    post_delayed_task(ThreadId::UI, Some(&mut task), ms);
}

wrap_task! {
    struct Later {
        f: Pending,
    }

    impl Task {
        fn execute(&self) {
            if let Some(f) = self.f.take() {
                f();
            }
        }
    }
}

/// How long after a page's first key the next one gets through, with room
/// to spare on a busy machine (about 400 ms was needed under Xvfb).
const FIRST_KEY_DELAY_MS: i64 = 700;

/// Send `keys` to the current tab, then run `then`. The first key a page
/// gets starts something in Chromium that loses keys for a few hundred
/// milliseconds, so a page that hasn't had one gets a bare Shift first and
/// the keys a moment later.
pub fn send_when_ready(keys: Vec<rt_core::Key>, then: impl FnOnce() + 'static) {
    let send = move || {
        for key in keys {
            send_to_page(key);
        }
        then();
    };
    let ready = shell::with(|s| s.current_browser())
        .flatten()
        .is_some_and(|b| had_keys(b.identifier()));
    if ready {
        return send();
    }
    prime_page();
    later(FIRST_KEY_DELAY_MS, send);
}

/// A lone Shift press and release in the current tab. Chromium drops the
/// first key event a page gets after it loads, so this goes before keys a
/// page must see, such as a call's mute key; pages ignore a bare Shift.
pub fn prime_page() {
    let Some(host) = shell::with(|s| s.current_browser())
        .flatten()
        .and_then(|b| b.host())
    else {
        return;
    };
    let shift = |type_, modifiers| KeyEvent {
        type_,
        modifiers,
        windows_key_code: 0x10,
        ..Default::default()
    };
    SENDING_TO_PAGE.with(|s| s.set(true));
    host.send_key_event(Some(&shift(KeyEventType::RAWKEYDOWN, EVENTFLAG_SHIFT_DOWN)));
    host.send_key_event(Some(&shift(KeyEventType::KEYUP, 0)));
    SENDING_TO_PAGE.with(|s| s.set(false));
}

/// Press and release `key` in a browser that has keyboard focus.
pub fn send_key_to(host: &BrowserHost, key: rt_core::Key) {
    let raw = vk::to_raw(key);
    let event = |type_: KeyEventType| key_event(key, type_);
    SENDING_TO_PAGE.with(|s| s.set(true));
    host.send_key_event(Some(&event(KeyEventType::RAWKEYDOWN)));
    if raw.character != 0 {
        host.send_key_event(Some(&event(KeyEventType::CHAR)));
    }
    host.send_key_event(Some(&event(KeyEventType::KEYUP)));
    SENDING_TO_PAGE.with(|s| s.set(false));
}

/// Keys normally arrive from the focused tab. If a UI view (tab bar, status
/// bar, overlay) has keyboard focus instead, e.g. after a click on the tab
/// bar, keys the engine doesn't consume would land in that view, so they're
/// sent on to the current tab and focus goes back to it.
fn route_key_event(role: Role, event: &KeyEvent) -> bool {
    // Keys replayed by a macro already went through the engine.
    if SENDING_TO_PAGE.with(Cell::get) {
        return false;
    }
    let consumed = handle_key_event(event);
    if consumed || matches!(role, Role::Tab | Role::PanelPage | Role::Popup) {
        return consumed;
    }
    // During prompts the status bar has focus on purpose (see `focus_for_prompt`).
    let prompting = shell::with(|s| {
        matches!(
            s.engine.mode(),
            rt_core::Mode::Prompt | rt_core::Mode::YesNo
        )
    });
    if prompting.unwrap_or(true) {
        return consumed;
    }
    let Some(tab) = shell::with(|s| s.tabs.current().map(|t| t.view.clone())).flatten() else {
        return false;
    };
    if let Some(host) = tab.browser().and_then(|b| b.host()) {
        host.send_key_event(Some(event));
    }
    View::from(&tab).request_focus();
    true
}

/// Returns true when the key is consumed and must not reach the page.
fn handle_key_event(event: &KeyEvent) -> bool {
    if event.type_ == KeyEventType::CHAR {
        return shell::with(|s| s.suppress_char).unwrap_or(false);
    }
    if event.type_ != KeyEventType::RAWKEYDOWN && event.type_ != KeyEventType::KEYDOWN {
        return false;
    }
    let has = |flag: u32| event.modifiers & flag != 0;
    let raw = RawKey {
        windows_key_code: event.windows_key_code,
        character: event.character,
        unmodified_character: event.unmodified_character,
        mods: Modifiers {
            ctrl: has(EVENTFLAG_CONTROL_DOWN),
            alt: has(EVENTFLAG_ALT_DOWN),
            shift: has(EVENTFLAG_SHIFT_DOWN),
            meta: has(EVENTFLAG_COMMAND_DOWN),
        },
    };
    let Some(key) = vk::translate(raw) else {
        return false;
    };
    if crate::popup::forward_key(&key)
        || crate::float::forward_key(&key)
        || crate::panel::forward_key(&key)
    {
        shell::with(|s| s.suppress_char = true);
        return true;
    }
    if crate::settings_page::forward_key(&key) {
        shell::with(|s| s.suppress_char = true);
        return true;
    }
    let Some(outcome) = shell::with(|s| {
        // hints.auto_follow_timeout: keys typed just after a hint was followed are dropped.
        let timeout = s.engine.settings().int("hints.auto_follow_timeout");
        if timeout > 0
            && s.engine.mode() == rt_core::Mode::Normal
            && s.hint_followed_at
                .is_some_and(|at| at.elapsed().as_millis() < timeout as u128)
        {
            s.suppress_char = true;
            return (
                rt_core::engine::KeyOutcome {
                    consumed: true,
                    effects: Vec::new(),
                },
                None,
            );
        }
        let outcome = s.engine.handle_key(key);
        s.suppress_char = outcome.consumed;
        let timeout = s.engine.settings().int("input.partial_timeout");
        let partial = s.engine.partial_keys().filter(|_| timeout > 0);
        (outcome, partial.map(|generation| (generation, timeout)))
    }) else {
        return false;
    };
    let (outcome, partial) = outcome;
    if let Some((generation, timeout)) = partial {
        shell::expire_partial_after(generation, timeout);
    }
    tracing::trace!(
        %key,
        vk = event.windows_key_code,
        editable = event.focus_on_editable_field,
        consumed = outcome.consumed,
        "key"
    );
    shell::apply(outcome.effects);
    outcome.consumed
}

wrap_display_handler! {
    struct RtDisplayHandler {}

    impl DisplayHandler {
        fn on_address_change(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            url: Option<&CefString>,
        ) {
            if frame.is_none_or(|f| f.is_main() == 0) {
                return;
            }
            let url = url.map(CefString::to_string).unwrap_or_default();
            if url.starts_with("chrome-error:") {
                return;
            }
            let changed = shell::with_tab(browser, |s, index, current| {
                if current {
                    s.engine.set_url(&url);
                }
                // A lazily restored tab keeps its real URL until it loads it.
                let tab = s.tabs.get_mut(index).filter(|tab| tab.pending.is_none())?;
                tab.url = url.clone();
                if let Some(history) = &mut tab.history {
                    history.committed(&url);
                }
                Some(())
            });
            shell::refresh_ui();
            if changed.flatten().is_some() {
                crate::lua::emit("url_changed", &[("url", &url)]);
            }
        }

        fn on_fullscreen_mode_change(&self, _browser: Option<&mut Browser>, fullscreen: ::std::os::raw::c_int) {
            crate::view::page_fullscreen(fullscreen != 0);
        }

        /// `content.javascript.log_message.levels`: page console messages in
        /// the status bar. Greasemonkey script errors always show; the UI
        /// bars aren't tabs, so they never do.
        fn on_console_message(
            &self,
            browser: Option<&mut Browser>,
            level: LogSeverity,
            message: Option<&CefString>,
            source: Option<&CefString>,
            line: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            let name = match level {
                LogSeverity::ERROR | LogSeverity::FATAL => "error",
                LogSeverity::WARNING => "warning",
                LogSeverity::INFO => "info",
                _ => "debug",
            };
            let source = source.map(|s| s.to_string()).unwrap_or_default();
            let script = source.starts_with("greasemonkey:");
            let shown = shell::with_tab(browser, |s, index, _| {
                let url = s.tabs.get(index).map(|t| t.url.clone()).unwrap_or_default();
                s.engine.settings().list_for("content.javascript.log_message.levels", &url).iter().any(|l| l == name)
            })
            .unwrap_or(false);
            if shown || (script && name == "error") {
                let text = message.map(|m| m.to_string()).unwrap_or_default();
                let where_ = if source.is_empty() { String::new() } else { format!(" ({source}:{line})") };
                let level = match name {
                    "error" => Level::Error,
                    "warning" => Level::Warning,
                    _ => Level::Info,
                };
                shell::show_message(level, format!("JS: {text}{where_}"));
            }
            0
        }

        /// The page started or stopped using a camera, the screen or a microphone.
        fn on_media_access_change(
            &self,
            browser: Option<&mut Browser>,
            has_video_access: ::std::os::raw::c_int,
            has_audio_access: ::std::os::raw::c_int,
        ) {
            let media = (has_video_access != 0, has_audio_access != 0);
            shell::with_tab(browser, |s, index, _| {
                if let Some(tab) = s.tabs.get_mut(index) {
                    tab.media = media;
                    // Covers shares the page never got to report the end of,
                    // such as when it navigates away.
                    if !media.0 {
                        tab.sharing = None;
                    }
                }
            });
            shell::refresh_ui();
        }

        fn on_title_change(&self, browser: Option<&mut Browser>, title: Option<&CefString>) {
            let title = title.map(CefString::to_string).unwrap_or_default();
            let url = shell::with_tab(browser, |s, index, _| {
                let tab = s.tabs.get_mut(index).filter(|tab| tab.pending.is_none())?;
                tab.title = title.clone();
                if let Some(history) = &mut tab.history {
                    history.titled(&title);
                }
                Some(tab.url.clone())
            });
            if let Some(Some(url)) = url {
                storage::set_title(&url, &title);
                crate::lua::emit("title_changed", &[("url", &url), ("title", &title)]);
            }
            shell::refresh_ui();
        }

        fn on_favicon_urlchange(&self, browser: Option<&mut Browser>, icon_urls: Option<&mut CefStringList>) {
            let urls = icon_urls.map(crate::favicons::read_list).unwrap_or_default();
            if let Some(browser) = browser {
                crate::favicons::changed(browser, urls);
            }
        }

        fn on_loading_progress_change(&self, browser: Option<&mut Browser>, progress: f64) {
            shell::with_tab(browser, |s, index, _| {
                if let Some(tab) = s.tabs.get_mut(index)
                    && tab.progress.is_some()
                {
                    tab.progress = Some(progress);
                }
            });
            shell::refresh_ui();
        }
    }
}

wrap_load_handler! {
    struct RtLoadHandler {
        role: Role,
    }

    impl LoadHandler {
        fn on_loading_state_change(
            &self,
            browser: Option<&mut Browser>,
            is_loading: ::std::os::raw::c_int,
            can_go_back: ::std::os::raw::c_int,
            can_go_forward: ::std::os::raw::c_int,
        ) {
            if self.role != Role::Tab {
                return;
            }
            let window = browser.as_deref().and_then(crate::tabs::window_of_crashed);
            let effects = shell::with_tab(browser, |s, index, current| {
                let tab = s.tabs.get_mut(index)?;
                tab.can_go_back = can_go_back != 0;
                tab.can_go_forward = can_go_forward != 0;
                tab.search_match = None;
                tab.scroll = None;
                if is_loading == 0 {
                    tab.progress = None;
                    return None;
                }
                tab.progress = Some(0.0);
                tab.load_error = false;
                tab.crashed = None;
                current.then(|| s.engine.load_started())
            })
            .flatten();
            if is_loading != 0
                && let Some(window) = window
            {
                crate::tabs::show_current_in(window, true);
            }
            shell::apply(effects.unwrap_or_default());
        }

        fn on_load_start(&self, browser: Option<&mut Browser>, frame: Option<&mut Frame>, _transition_type: TransitionType) {
            if self.role == Role::Tab
                && let Some(frame) = frame.filter(|f| f.is_main() != 0)
            {
                crate::userstyle::inject(frame);
                let url = CefString::from(&frame.url()).to_string();
                crate::lua::emit("load_started", &[("url", &url)]);
            }
        }

        fn on_load_end(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            _http_status_code: ::std::os::raw::c_int,
        ) {
            let Some(frame) = frame else { return };
            // A tab's frames get element hiding of their own.
            if frame.is_main() == 0 {
                if self.role == Role::Tab
                    && let Some(page) = browser.as_deref().and_then(|b| b.main_frame())
                {
                    let url = CefString::from(&frame.url()).to_string();
                    let page = CefString::from(&page.url()).to_string();
                    crate::adblock::apply_cosmetic_frame(frame, &url, &page);
                }
                return;
            }
            // riptide's own bars are sized for 100%: undo a zoom saved for
            // them (Chromium keeps one per site, and they share riptide://ui).
            if self.role != Role::Tab
                && let Some(host) = browser.as_deref().and_then(|b| b.host())
                && host.zoom_level() != 0.0
            {
                host.set_zoom_level(0.0);
            }
            match self.role {
                Role::Tab => {
                    let browser_ref = browser.as_deref().cloned();
                    let done = shell::with_tab(browser, |s, index, current| {
                        let private = s.private;
                        let effects = if current { s.engine.apply_mode_override() } else { Vec::new() };
                        let tab = s.tabs.get_mut(index)?;
                        // Private windows leave no history.
                        let visit = (!tab.load_error && !private && tab.pending.is_none())
                            .then(|| (tab.url.clone(), tab.title.clone()));
                        // A lazily restored tab's placeholder isn't the saved page.
                        let scroll = if tab.pending.is_none() && !tab.load_error {
                            tab.restore_scroll.take()
                        } else {
                            None
                        };
                        Some((tab.pending_error.take(), visit, effects, scroll))
                    });
                    let Some(Some((error, visit, effects, scroll))) = done else { return };
                    shell::apply(effects);
                    if let Some(y) = scroll {
                        crate::history::restore_scroll(frame, y);
                    }
                    if let Some((url, error)) = error {
                        shell::exec_js(frame, &ui::error_page_js(&url, &error));
                    }
                    if let Some((url, title)) = visit {
                        storage::record_visit(&url, &title);
                    }
                    let url = CefString::from(&frame.url()).to_string();
                    if let Some(browser) = browser_ref {
                        crate::marks::loaded(&browser);
                        crate::view::loaded(&browser);
                        crate::adblock::apply_cosmetic(&browser, &url);
                        crate::userstyle::inject(frame);
                        // scrolling.bar = never.
                        if shell::with(|s| s.engine.settings().str("scrolling.bar") == "never").unwrap_or(false) {
                            crate::adblock::inject_css(&browser, "::-webkit-scrollbar { display: none !important; }");
                        }
                    }
                    crate::lua::emit("load_finished", &[("url", &url)]);
                    crate::extensions::store_page_loaded(&url);
                    shell::show_messages_waiting_for_load();
                    return;
                }
                Role::Tabbar => shell::with(|s| s.tabbar_ready = true),
                Role::Statusbar => shell::with(|s| s.statusbar_ready = true),
                Role::Completion => shell::with(|s| s.completion_ready = true),
                Role::Crashed => return,
                Role::Float => {
                    if let Some(browser) = &browser {
                        crate::float::ready(browser.identifier());
                    }
                    return;
                }
                Role::Panel => {
                    if let Some(browser) = &browser {
                        crate::panel::ready(browser.identifier());
                    }
                    return;
                }
                Role::PanelPage => return,
                Role::Popup => {
                    if let Some(browser) = &browser {
                        crate::popup::page_loaded(browser);
                    }
                    return;
                }
                Role::PopupBar => {
                    if let Some(browser) = &browser {
                        crate::popup::bar_loaded(browser);
                    }
                    return;
                }
            };
            shell::refresh_ui();
        }

        fn on_load_error(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            error_code: Errorcode,
            error_text: Option<&CefString>,
            failed_url: Option<&CefString>,
        ) {
            let Some(frame) = frame else { return };
            if self.role != Role::Tab || frame.is_main() == 0 {
                return;
            }
            // Aborted loads include downloads and navigations we replaced;
            // unknown schemes (mailto:) go to content.unknown_url_scheme_policy.
            if matches!(
                sys::cef_errorcode_t::from(error_code),
                sys::cef_errorcode_t::ERR_ABORTED | sys::cef_errorcode_t::ERR_UNKNOWN_URL_SCHEME
            ) {
                return;
            }
            let error_text = error_text.map(CefString::to_string).unwrap_or_default();
            let failed_url = failed_url.map(CefString::to_string).unwrap_or_default();
            // Navigating to our own error page would add a history entry that
            // `back` returns to, so draw into Chromium's error document instead.
            shell::with_tab(browser, |s, index, current| {
                if current {
                    s.engine.set_url(&failed_url);
                    s.engine
                        .show_message(Level::Error, format!("Error loading {failed_url}: {error_text}"));
                }
                if let Some(tab) = s.tabs.get_mut(index) {
                    tab.load_error = true;
                    tab.url = failed_url.clone();
                    if let Some(history) = &mut tab.history {
                        history.committed(&failed_url);
                    }
                    tab.pending_error = Some((failed_url, error_text));
                }
            });
            shell::refresh_ui();
        }
    }
}

wrap_life_span_handler! {
    struct RtLifeSpanHandler {
        role: Role,
    }

    impl LifeSpanHandler {
        fn on_before_popup(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _popup_id: ::std::os::raw::c_int,
            _target_url: Option<&CefString>,
            _target_frame_name: Option<&CefString>,
            target_disposition: WindowOpenDisposition,
            _user_gesture: ::std::os::raw::c_int,
            _popup_features: Option<&PopupFeatures>,
            _window_info: Option<&mut WindowInfo>,
            _client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            _extra_info: Option<&mut Option<DictionaryValue>>,
            _no_javascript_access: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            if self.role != Role::Tab {
                return 1;
            }
            // CEF now creates the popup view; `on_popup_browser_view_created` adopts it as a tab.
            let background = target_disposition == WindowOpenDisposition::NEW_BACKGROUND_TAB;
            shell::with(|s| s.popup_in_background = background);
            0
        }

        fn do_close(&self, browser: Option<&mut Browser>) -> ::std::os::raw::c_int {
            // A page calling `window.close()` should close its tab, not the window.
            // Closing from inside this callback is unsafe, so post it.
            let tab = shell::with_tab(browser, |s, index, _| {
                (!s.window_closing && s.tabs.len() > 1).then(|| (index, s.engine.settings().bool("content.javascript.can_close_tabs")))
            });
            match tab.flatten() {
                Some((_, false)) => {
                    shell::show_message(Level::Info, "The page tried to close its tab (content.javascript.can_close_tabs)");
                    1
                }
                Some((index, true)) => {
                    let mut task = tabs::CloseTab::new(index);
                    post_task(ThreadId::UI, Some(&mut task));
                    1
                }
                None => 0,
            }
        }

        fn on_after_created(&self, browser: Option<&mut Browser>) {
            shell::with(|s| s.open_browsers += 1);
            // Focus asked for before the browser existed doesn't always reach
            // it, which drops the first keys typed into a new tab.
            let view = shell::with_tab(browser, |s, _, current| current.then(|| s.tabs.current().map(|t| t.view.clone())));
            if let Some(Some(Some(view))) = view
                && !shell::with(|s| matches!(s.engine.mode(), rt_core::Mode::Prompt | rt_core::Mode::YesNo)).unwrap_or(false)
            {
                View::from(&view).request_focus();
            }
        }

        fn on_before_close(&self, _browser: Option<&mut Browser>) {
            let remaining = shell::with(|s| {
                s.open_browsers = s.open_browsers.saturating_sub(1);
                s.open_browsers
            });
            if remaining == Some(0) {
                CLOSED_EVERY_BROWSER.store(true, std::sync::atomic::Ordering::SeqCst);
                quit_message_loop();
            }
        }
    }
}
