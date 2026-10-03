//! JavaScript dialogs (`alert`, `confirm`, `prompt`, leave-page) and HTTP
//! authentication, shown as browser prompts instead of native windows.

use std::cell::RefCell;

use cef::*;
use hb_core::prompt::{PromptAnswer, PromptKind, Remember};

use crate::prompts::{self, Scope};

fn string(s: Option<&CefString>) -> String {
    s.map(CefString::to_string).unwrap_or_default()
}

wrap_jsdialog_handler! {
    pub struct HbJsdialogHandler {}

    impl JsdialogHandler {
        fn on_jsdialog(
            &self,
            browser: Option<&mut Browser>,
            origin_url: Option<&CefString>,
            dialog_type: JsdialogType,
            message_text: Option<&CefString>,
            default_prompt_text: Option<&CefString>,
            callback: Option<&mut JsdialogCallback>,
            _suppress_message: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            let Some(callback) = callback.map(|c| c.clone()) else { return 0 };
            let browser = browser.map(|b| b.identifier());
            let title = format!("{} says", string(origin_url));
            let message = string(message_text);
            let kind = if dialog_type == JsdialogType::ALERT {
                PromptKind::Alert
            } else if dialog_type == JsdialogType::CONFIRM {
                PromptKind::YesNo { default: true, remember: Remember::Never }
            } else {
                PromptKind::Text { default: string(default_prompt_text), masked: false }
            };
            prompts::ask(browser, Scope::JsDialog, title, message, kind, move |answer| {
                let (ok, input) = match answer {
                    PromptAnswer::Ok | PromptAnswer::Yes { .. } => (1, String::new()),
                    PromptAnswer::Text(text) => (1, text),
                    PromptAnswer::No { .. } | PromptAnswer::Cancelled => (0, String::new()),
                };
                callback.cont(ok, Some(&CefString::from(input.as_str())));
            });
            1
        }

        fn on_before_unload_dialog(
            &self,
            browser: Option<&mut Browser>,
            _message_text: Option<&CefString>,
            is_reload: ::std::os::raw::c_int,
            callback: Option<&mut JsdialogCallback>,
        ) -> ::std::os::raw::c_int {
            let Some(callback) = callback.map(|c| c.clone()) else { return 0 };
            let action = if is_reload != 0 { "Reload" } else { "Leave" };
            prompts::ask(
                browser.map(|b| b.identifier()),
                Scope::JsDialog,
                "Unsaved changes",
                format!("This page may have unsaved changes. {action} anyway?"),
                PromptKind::YesNo { default: false, remember: Remember::Never },
                move |answer| {
                    let leave = matches!(answer, PromptAnswer::Yes { .. });
                    callback.cont(leave.into(), None);
                },
            );
            1
        }

        fn on_reset_dialog_state(&self, browser: Option<&mut Browser>) {
            // The page is gone; CEF no longer wants an answer to its dialogs.
            if let Some(browser) = browser {
                prompts::withdraw_for_browser(browser.identifier(), Some(Scope::JsDialog));
            }
        }
    }
}

wrap_request_handler! {
    pub struct HbRequestHandler {}

    impl RequestHandler {
        /// Tabs may show hb:// pages the user opened (e.g. help), but never the
        /// UI pages, and never because a site redirected there.
        fn on_before_browse(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _user_gesture: ::std::os::raw::c_int,
            is_redirect: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            let url = request.map(|r| CefString::from(&r.url()).to_string()).unwrap_or_default();
            let blocked = url.starts_with(hb_core::ui_message::UI_PREFIX)
                || (is_redirect != 0 && url.starts_with("hb://"));
            if blocked {
                tracing::warn!("blocked navigation to {url}");
            }
            blocked.into()
        }

        fn resource_request_handler(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _request: Option<&mut Request>,
            _is_navigation: ::std::os::raw::c_int,
            _is_download: ::std::os::raw::c_int,
            _request_initiator: Option<&CefString>,
            _disable_default_handling: Option<&mut ::std::os::raw::c_int>,
        ) -> Option<ResourceRequestHandler> {
            Some(HbResourceRequestHandler::new())
        }

        fn on_certificate_error(
            &self,
            browser: Option<&mut Browser>,
            cert_error: Errorcode,
            request_url: Option<&CefString>,
            _ssl_info: Option<&mut Sslinfo>,
            callback: Option<&mut Callback>,
        ) -> ::std::os::raw::c_int {
            let Some(callback) = callback.map(|c| c.clone()) else { return 0 };
            let url = string(request_url);
            let code = cert_error.get_raw();
            crate::tls::certificate_error(browser.map(|b| b.identifier()), code, &url, callback).into()
        }

        fn auth_credentials(
            &self,
            browser: Option<&mut Browser>,
            _origin_url: Option<&CefString>,
            is_proxy: ::std::os::raw::c_int,
            host: Option<&CefString>,
            port: ::std::os::raw::c_int,
            realm: Option<&CefString>,
            _scheme: Option<&CefString>,
            callback: Option<&mut AuthCallback>,
        ) -> ::std::os::raw::c_int {
            let Some(callback) = callback.map(|c| c.clone()) else { return 0 };
            let browser = browser.map(|b| b.identifier());
            let what = if is_proxy != 0 { "Proxy" } else { "Site" };
            let realm = string(realm);
            let realm = if realm.is_empty() { String::new() } else { format!(" ({realm})") };
            let message = format!("{what} {}:{port}{realm} needs a username and password", string(host));
            // CEF calls this on the IO thread; prompts live on the UI thread.
            let mut task = AskCredentials::new(browser, message, RefCell::new(Some(callback)));
            post_task(ThreadId::UI, Some(&mut task));
            1
        }
    }
}

wrap_resource_request_handler! {
    pub struct HbResourceRequestHandler {}

    impl ResourceRequestHandler {
        /// Runs on the IO thread for every request a tab makes.
        fn on_before_resource_load(
            &self,
            browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _callback: Option<&mut Callback>,
        ) -> ReturnValue {
            let (Some(browser), Some(request)) = (browser, request) else {
                return ReturnValue::CONTINUE;
            };
            let url = CefString::from(&request.url()).to_string();
            let page = browser
                .main_frame()
                .map(|f| CefString::from(&f.url()).to_string())
                .unwrap_or_default();
            if crate::adblock::should_block(&url, &page, request.resource_type()) {
                ReturnValue::CANCEL
            } else {
                ReturnValue::CONTINUE
            }
        }
    }
}

wrap_task! {
    struct AskCredentials {
        browser: Option<i32>,
        message: String,
        // A task runs once; the cell lets `execute(&self)` take the callback.
        callback: RefCell<Option<AuthCallback>>,
    }

    impl Task {
        fn execute(&self) {
            let Some(callback) = self.callback.borrow_mut().take() else { return };
            ask_credentials(self.browser, self.message.clone(), callback);
        }
    }
}

/// Ask for a username, then a hidden password.
fn ask_credentials(browser: Option<i32>, message: String, callback: AuthCallback) {
    let password_message = message.clone();
    prompts::ask(
        browser,
        Scope::Other,
        "Authentication required",
        message,
        PromptKind::Text {
            default: String::new(),
            masked: false,
        },
        move |answer| {
            let PromptAnswer::Text(username) = answer else {
                return callback.cancel();
            };
            prompts::ask(
                browser,
                Scope::Other,
                format!("Password for {username}"),
                password_message,
                PromptKind::Text {
                    default: String::new(),
                    masked: true,
                },
                move |answer| match answer {
                    PromptAnswer::Text(password) => callback.cont(
                        Some(&CefString::from(username.as_str())),
                        Some(&CefString::from(password.as_str())),
                    ),
                    _ => callback.cancel(),
                },
            );
        },
    );
}
