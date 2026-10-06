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

/// Which part of the window a browser belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Tab,
    Tabbar,
    Statusbar,
    Completion,
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
                let url = frame.map(|f| CefString::from(&f.url()).to_string()).unwrap_or_default();
                if self.role != Role::Tab
                    && let Some(args) = message.argument_list()
                {
                    // A click in a window's tab bar is about that window.
                    if let Some(browser) = &browser {
                        shell::activate_browser(browser.identifier());
                    }
                    let name = CefString::from(&args.string(0)).to_string();
                    let payload = CefString::from(&args.string(1)).to_string();
                    match rt_core::ui_message::parse(&url, &name, &payload) {
                        Ok(message) => crate::ui::handle_message(message),
                        Err(e) => tracing::warn!("rejected UI message: {e}"),
                    }
                }
                return 1;
            }
            if self.role != Role::Tab || name != FOCUS_MESSAGE {
                return 0;
            }
            let args = message.argument_list();
            let editable = args.as_ref().is_some_and(|a| a.bool(0) != 0);
            let user = args.as_ref().is_none_or(|a| a.bool(1) != 0);
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
    if consumed || role == Role::Tab {
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
    let Some(outcome) = shell::with(|s| {
        // hints.auto_follow_timeout: keys typed just after a hint was followed are dropped.
        let timeout = s.engine.settings().int("hints.auto_follow_timeout");
        if timeout > 0
            && s.engine.mode() == rt_core::Mode::Normal
            && s.hint_followed_at
                .is_some_and(|at| at.elapsed().as_millis() < timeout as u128)
        {
            s.suppress_char = true;
            return rt_core::engine::KeyOutcome {
                consumed: true,
                effects: Vec::new(),
            };
        }
        let outcome = s.engine.handle_key(key);
        s.suppress_char = outcome.consumed;
        outcome
    }) else {
        return false;
    };
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
                s.tabs.get_mut(index).filter(|tab| tab.pending.is_none()).map(|tab| tab.url = url.clone())
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

        fn on_title_change(&self, browser: Option<&mut Browser>, title: Option<&CefString>) {
            let title = title.map(CefString::to_string).unwrap_or_default();
            let url = shell::with_tab(browser, |s, index, _| {
                let tab = s.tabs.get_mut(index).filter(|tab| tab.pending.is_none())?;
                tab.title = title.clone();
                Some(tab.url.clone())
            });
            if let Some(Some(url)) = url {
                storage::set_title(&url, &title);
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
                current.then(|| s.engine.load_started())
            })
            .flatten();
            shell::apply(effects.unwrap_or_default());
        }

        fn on_load_end(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            _http_status_code: ::std::os::raw::c_int,
        ) {
            let Some(frame) = frame.filter(|f| f.is_main() != 0) else {
                return;
            };
            match self.role {
                Role::Tab => {
                    let browser_ref = browser.as_deref().cloned();
                    let done = shell::with_tab(browser, |s, index, _| {
                        let private = s.private;
                        let tab = s.tabs.get_mut(index)?;
                        // Private windows leave no history.
                        let visit = (!tab.load_error && !private && tab.pending.is_none())
                            .then(|| (tab.url.clone(), tab.title.clone()));
                        Some((tab.pending_error.take(), visit))
                    });
                    let Some(Some((error, visit))) = done else { return };
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
                    }
                    crate::lua::emit("load_finished", &[("url", &url)]);
                    return;
                }
                Role::Tabbar => shell::with(|s| s.tabbar_ready = true),
                Role::Statusbar => shell::with(|s| s.statusbar_ready = true),
                Role::Completion => shell::with(|s| s.completion_ready = true),
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
            // Aborted loads include downloads and navigations we replaced.
            if sys::cef_errorcode_t::from(error_code) == sys::cef_errorcode_t::ERR_ABORTED {
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
                quit_message_loop();
            }
        }
    }
}
