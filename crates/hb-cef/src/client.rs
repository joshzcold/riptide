use cef::*;
use hb_core::Modifiers;
use hb_core::engine::Level;
use hb_core::vk::{self, RawKey};

use crate::renderer::{EVAL_RESULT_MESSAGE, FOCUS_MESSAGE};
use crate::{eval, shell, storage, tabs, ui};

#[cfg(target_os = "linux")]
type OsEvent = sys::XEvent;

/// Which part of the window a browser belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Tab,
    Tabbar,
    Statusbar,
    Completion,
}

wrap_client! {
    pub struct HbClient {
        role: Role,
    }

    impl Client {
        fn keyboard_handler(&self) -> Option<KeyboardHandler> {
            Some(HbKeyboardHandler::new())
        }

        fn display_handler(&self) -> Option<DisplayHandler> {
            (self.role == Role::Tab).then(HbDisplayHandler::new)
        }

        fn load_handler(&self) -> Option<LoadHandler> {
            Some(HbLoadHandler::new(self.role))
        }

        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(HbLifeSpanHandler::new(self.role))
        }

        fn jsdialog_handler(&self) -> Option<JsdialogHandler> {
            (self.role == Role::Tab).then(crate::dialogs::HbJsdialogHandler::new)
        }

        fn request_handler(&self) -> Option<RequestHandler> {
            (self.role == Role::Tab).then(crate::dialogs::HbRequestHandler::new)
        }

        fn download_handler(&self) -> Option<DownloadHandler> {
            (self.role == Role::Tab).then(crate::downloads::HbDownloadHandler::new)
        }

        fn permission_handler(&self) -> Option<PermissionHandler> {
            (self.role == Role::Tab).then(crate::permissions::HbPermissionHandler::new)
        }

        fn on_process_message_received(
            &self,
            browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
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
            if self.role != Role::Tab || name != FOCUS_MESSAGE {
                return 0;
            }
            let editable = message.argument_list().is_some_and(|args| args.bool(0) != 0);
            // Background tabs can move focus too; only the visible one drives the mode.
            let effects = shell::with_tab(browser, |s, _, current| {
                if current { s.engine.focus_changed(editable) } else { Vec::new() }
            });
            if let Some(effects) = effects {
                shell::apply(effects);
            }
            1
        }
    }
}

wrap_keyboard_handler! {
    struct HbKeyboardHandler {}

    impl KeyboardHandler {
        fn on_pre_key_event(
            &self,
            _browser: Option<&mut Browser>,
            event: Option<&KeyEvent>,
            _os_event: Option<&mut OsEvent>,
            _is_keyboard_shortcut: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            event.is_some_and(handle_key_event).into()
        }
    }
}

/// Returns true when the key is consumed and must not reach the page.
fn handle_key_event(event: &KeyEvent) -> bool {
    if event.type_ == KeyEventType::CHAR {
        return shell::with(|s| s.suppress_char).unwrap_or(false);
    }
    if event.type_ != KeyEventType::RAWKEYDOWN && event.type_ != KeyEventType::KEYDOWN {
        return false;
    }
    let flags = event.modifiers;
    let has = |flag: sys::cef_event_flags_t| flags & flag.0 != 0;
    let raw = RawKey {
        windows_key_code: event.windows_key_code,
        character: event.character,
        unmodified_character: event.unmodified_character,
        mods: Modifiers {
            ctrl: has(sys::cef_event_flags_t::EVENTFLAG_CONTROL_DOWN),
            alt: has(sys::cef_event_flags_t::EVENTFLAG_ALT_DOWN),
            shift: has(sys::cef_event_flags_t::EVENTFLAG_SHIFT_DOWN),
            meta: has(sys::cef_event_flags_t::EVENTFLAG_COMMAND_DOWN),
        },
    };
    let Some(key) = vk::translate(raw) else {
        return false;
    };
    let Some(outcome) = shell::with(|s| {
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
    struct HbDisplayHandler {}

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
            shell::with_tab(browser, |s, index, current| {
                if current {
                    s.engine.set_url(&url);
                }
                if let Some(tab) = s.tabs.get_mut(index) {
                    tab.url = url;
                }
            });
            shell::refresh_ui();
        }

        fn on_title_change(&self, browser: Option<&mut Browser>, title: Option<&CefString>) {
            let title = title.map(CefString::to_string).unwrap_or_default();
            let url = shell::with_tab(browser, |s, index, _| {
                let tab = s.tabs.get_mut(index)?;
                tab.title = title.clone();
                Some(tab.url.clone())
            });
            if let Some(Some(url)) = url {
                storage::set_title(&url, &title);
            }
            shell::refresh_ui();
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
    struct HbLoadHandler {
        role: Role,
    }

    impl LoadHandler {
        fn on_loading_state_change(
            &self,
            browser: Option<&mut Browser>,
            is_loading: ::std::os::raw::c_int,
            _can_go_back: ::std::os::raw::c_int,
            _can_go_forward: ::std::os::raw::c_int,
        ) {
            if self.role != Role::Tab {
                return;
            }
            let effects = shell::with_tab(browser, |s, index, current| {
                let tab = s.tabs.get_mut(index)?;
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
                    let done = shell::with_tab(browser, |s, index, _| {
                        let tab = s.tabs.get_mut(index)?;
                        let visit = (!tab.load_error).then(|| (tab.url.clone(), tab.title.clone()));
                        Some((tab.pending_error.take(), visit))
                    });
                    let Some(Some((error, visit))) = done else { return };
                    if let Some((url, error)) = error {
                        shell::exec_js(frame, &ui::error_page_js(&url, &error));
                    }
                    if let Some((url, title)) = visit {
                        storage::record_visit(&url, &title);
                    }
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
    struct HbLifeSpanHandler {
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
            let tab = shell::with_tab(browser, |s, index, _| (!s.window_closing && s.tabs.len() > 1).then_some(index));
            match tab.flatten() {
                Some(index) => {
                    let mut task = tabs::CloseTab::new(index);
                    post_task(ThreadId::UI, Some(&mut task));
                    1
                }
                None => 0,
            }
        }

        fn on_after_created(&self, _browser: Option<&mut Browser>) {
            shell::with(|s| s.open_browsers += 1);
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
