use cef::*;
use hb_core::Modifiers;
use hb_core::engine::Level;
use hb_core::vk::{self, RawKey};

use crate::renderer::FOCUS_MESSAGE;
use crate::shell;
use crate::ui;

#[cfg(target_os = "linux")]
type OsEvent = sys::XEvent;

/// Which part of the window a browser belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Tab,
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

        fn on_process_message_received(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _source_process: ProcessId,
            message: Option<&mut ProcessMessage>,
        ) -> ::std::os::raw::c_int {
            let Some(message) = message else { return 0 };
            if self.role != Role::Tab || CefString::from(&message.name()).to_string() != FOCUS_MESSAGE {
                return 0;
            }
            let editable = message.argument_list().is_some_and(|args| args.bool(0) != 0);
            if let Some(effects) = shell::with(|s| s.engine.focus_changed(editable)) {
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
            _browser: Option<&mut Browser>,
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
            shell::with(|s| {
                s.engine.set_url(&url);
                s.url = url;
            });
            shell::refresh_ui();
        }

        fn on_title_change(&self, _browser: Option<&mut Browser>, title: Option<&CefString>) {
            let title = title.map(CefString::to_string).unwrap_or_default();
            if let Some(window) = shell::with(|s| s.window.clone()).flatten() {
                window.set_title(Some(&CefString::from(format!("{title} - hackers-browser").as_str())));
            }
        }

        fn on_loading_progress_change(&self, _browser: Option<&mut Browser>, progress: f64) {
            shell::with(|s| {
                if s.progress.is_some() {
                    s.progress = Some(progress);
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
            _browser: Option<&mut Browser>,
            is_loading: ::std::os::raw::c_int,
            _can_go_back: ::std::os::raw::c_int,
            _can_go_forward: ::std::os::raw::c_int,
        ) {
            if self.role != Role::Tab {
                return;
            }
            let effects = shell::with(|s| {
                if is_loading != 0 {
                    s.progress = Some(0.0);
                    s.load_error = false;
                    s.engine.load_started()
                } else {
                    s.progress = None;
                    Vec::new()
                }
            });
            shell::apply(effects.unwrap_or_default());
        }

        fn on_load_end(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            _http_status_code: ::std::os::raw::c_int,
        ) {
            let Some(frame) = frame.filter(|f| f.is_main() != 0) else {
                return;
            };
            match self.role {
                Role::Tab => {
                    if let Some((url, error)) = shell::with(|s| s.pending_error.take()).flatten() {
                        shell::exec_js(frame, &ui::error_page_js(&url, &error));
                    }
                    return;
                }
                Role::Statusbar => shell::with(|s| s.statusbar_ready = true),
                Role::Completion => shell::with(|s| s.completion_ready = true),
            };
            shell::refresh_ui();
        }

        fn on_load_error(
            &self,
            _browser: Option<&mut Browser>,
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
            shell::with(|s| {
                s.load_error = true;
                s.engine.set_url(&failed_url);
                s.engine
                    .show_message(Level::Error, format!("Error loading {failed_url}: {error_text}"));
                s.url = failed_url.clone();
                s.pending_error = Some((failed_url, error_text));
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
            browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _popup_id: ::std::os::raw::c_int,
            target_url: Option<&CefString>,
            _target_frame_name: Option<&CefString>,
            _target_disposition: WindowOpenDisposition,
            _user_gesture: ::std::os::raw::c_int,
            _popup_features: Option<&PopupFeatures>,
            _window_info: Option<&mut WindowInfo>,
            _client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            _extra_info: Option<&mut Option<DictionaryValue>>,
            _no_javascript_access: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            // Until tabs exist (M3), popups open in the current tab.
            if self.role == Role::Tab
                && let (Some(browser), Some(url)) = (browser, target_url)
                && let Some(frame) = browser.main_frame()
            {
                frame.load_url(Some(url));
            }
            1
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
