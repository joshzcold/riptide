//! Code that runs in the renderer process.

use std::cell::RefCell;

use cef::*;
use rt_config::greasemonkey::{RunAt, Script};

/// Renderer → browser: focus moved. Argument 0 is whether the new node is
/// editable, argument 1 whether a click or key just happened (rather than the
/// page moving focus by itself, as autofocus does).
pub const FOCUS_MESSAGE: &str = "rt.focus";
/// Browser → renderer: evaluate argument 1 (code) in the frame; argument 0 is a request id.
pub const EVAL_MESSAGE: &str = "rt.eval";
/// Renderer → browser: id, success flag, and the string result or error message.
pub const EVAL_RESULT_MESSAGE: &str = "rt.eval-result";
/// Renderer → browser, from `riptide://ui/` pages only: message name and JSON payload.
pub const UI_MESSAGE: &str = "rt.ui";
/// Browser → renderer: the Greasemonkey scripts as JSON (argument 0). New
/// browsers get the same JSON in their `extra_info` under this key.
pub const GREASEMONKEY_MESSAGE: &str = "rt.greasemonkey";
/// Browser → renderer: a script's `GM_setValue` values changed; argument 0
/// is `{script, values}` as JSON.
pub const GM_VALUES_MESSAGE: &str = "rt.gm-values";
/// Renderer → browser: `GM_setValue`; arguments are the script name, the
/// key, and the value as JSON (empty to delete it).
pub const GM_SET_MESSAGE: &str = "rt.gm-set";

/// The scripts, numbered so a renderer can tell which list is newest.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Scripts {
    pub generation: u64,
    pub scripts: Vec<Script>,
}

thread_local! {
    static SCRIPTS: RefCell<(u64, Vec<Script>)> = const { RefCell::new((0, Vec::new())) };
}

fn set_scripts(json: &str) {
    match serde_json::from_str::<Scripts>(json) {
        Ok(Scripts {
            generation,
            scripts,
        }) => SCRIPTS.with(|s| {
            let mut s = s.borrow_mut();
            // CEF hands a browser's original `extra_info` over again on
            // reload, so keep a newer list from `:greasemonkey-reload`.
            if generation >= s.0 {
                *s = (generation, scripts);
            }
        }),
        Err(e) => tracing::warn!("bad greasemonkey scripts from the browser: {e}"),
    }
}

/// A script's stored values changed in another page.
fn set_values(json: &str) {
    #[derive(serde::Deserialize)]
    struct Update {
        generation: u64,
        script: String,
        values: serde_json::Map<String, serde_json::Value>,
    }
    let Ok(update) = serde_json::from_str::<Update>(json) else {
        return;
    };
    SCRIPTS.with(|s| {
        let mut s = s.borrow_mut();
        s.0 = s.0.max(update.generation);
        if let Some(script) = s.1.iter_mut().find(|s| s.name == update.script) {
            script.values = update.values;
        }
    });
}

/// Run the Greasemonkey scripts for this frame. `document-start` scripts run
/// now, before the page's own; the others wait for their event.
fn run_greasemonkey(frame: &Frame, context: &V8Context, url: &str) {
    let main = frame.is_main() != 0;
    SCRIPTS.with(|scripts| {
        for script in scripts.borrow().1.iter() {
            if (script.no_frames && !main) || !script.applies_to(url) {
                continue;
            }
            let info = serde_json::json!({
                "script": { "name": script.name, "version": script.version, "description": script.description },
                "scriptHandler": "riptide",
                "version": env!("CARGO_PKG_VERSION"),
            });
            let values = serde_json::Value::Object(script.values.clone());
            let body = format!(
                "const GM_info = {info};\n\
                 const unsafeWindow = window;\n\
                 const GM_addStyle = (css) => {{ const s = document.createElement('style'); \
                 s.textContent = css; (document.head || document.documentElement).appendChild(s); return s; }};\n\
                 const __rtValues = {values};\n\
                 const GM_getValue = (k, d) => Object.prototype.hasOwnProperty.call(__rtValues, k) ? __rtValues[k] : d;\n\
                 const GM_setValue = (k, v) => {{ __rtValues[k] = v; __rtSet(String(k), JSON.stringify(v) ?? 'null'); }};\n\
                 const GM_deleteValue = (k) => {{ delete __rtValues[k]; __rtSet(String(k), ''); }};\n\
                 const GM_listValues = () => Object.keys(__rtValues);\n\
                 const GM = {{ info: GM_info, addStyle: GM_addStyle, \
                 getValue: async (k, d) => GM_getValue(k, d), setValue: async (k, v) => GM_setValue(k, v), \
                 deleteValue: async (k) => GM_deleteValue(k), listValues: async () => GM_listValues() }};\n\
                 {required}\n{code}",
                required = script.required_code,
                code = script.code
            );
            let run = match script.run_at {
                RunAt::Start => body,
                RunAt::End => format!(
                    "document.addEventListener('DOMContentLoaded', () => {{ {body} }}, {{ once: true }});"
                ),
                RunAt::Idle => format!(
                    "addEventListener('load', () => setTimeout(() => {{ {body} }}, 0), {{ once: true }});"
                ),
            };
            // The setter is passed in as an argument rather than through a
            // global, so the page can't reach it.
            let code = format!("(function (__rtSet) {{\n{run}\n}})");
            let name = CefString::from(format!("greasemonkey:{}", script.name).as_str());
            let mut retval = None;
            let mut exception = None;
            context.enter();
            let mut ok = context.eval(
                Some(&CefString::from(code.as_str())),
                Some(&name),
                0,
                Some(&mut retval),
                Some(&mut exception),
            );
            let mut handler = RtGmSetHandler::new(script.name.clone());
            let setter = v8_value_create_function(Some(&CefString::from("GM_setValue")), Some(&mut handler));
            if ok != 0
                && let (Some(function), Some(setter)) = (retval, setter)
            {
                ok = function.execute_function(None, Some(&[Some(setter)])).is_some().into();
            }
            context.exit();
            if ok == 0 {
                let message = exception.map(|e| CefString::from(&e.message()).to_string());
                tracing::warn!(script = %script.name, ?message, "greasemonkey script failed");
            }
        }
    });
}

wrap_render_process_handler! {
    pub struct RtRenderProcessHandler {}

    impl RenderProcessHandler {
        fn on_browser_created(&self, _browser: Option<&mut Browser>, extra_info: Option<&mut DictionaryValue>) {
            let key = CefString::from(GREASEMONKEY_MESSAGE);
            if let Some(info) = extra_info.filter(|i| i.has_key(Some(&key)) != 0) {
                set_scripts(&CefString::from(&info.string(Some(&key))).to_string());
            }
        }

        /// Give the browser's own UI pages (and only those) `rt.send(name, json)`;
        /// run Greasemonkey scripts in web pages.
        fn on_context_created(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            context: Option<&mut V8Context>,
        ) {
            let (Some(frame), Some(context)) = (frame, context) else { return };
            let url = CefString::from(&frame.url()).to_string();
            if !url.starts_with(rt_core::ui_message::UI_PREFIX) {
                if !url.starts_with("riptide://") {
                    run_greasemonkey(frame, context, &url);
                }
                return;
            }
            let Some(global) = context.global() else { return };
            let Some(mut api) = v8_value_create_object(None, None) else { return };
            let mut handler = RtSendHandler::new();
            let Some(mut send) = v8_value_create_function(Some(&CefString::from("send")), Some(&mut handler)) else {
                return;
            };
            let fixed = V8Propertyattribute::from(
                sys::cef_v8_propertyattribute_t::V8_PROPERTY_ATTRIBUTE_READONLY
                    | sys::cef_v8_propertyattribute_t::V8_PROPERTY_ATTRIBUTE_DONTDELETE,
            );
            api.set_value_bykey(Some(&CefString::from("send")), Some(&mut send), fixed);
            global.set_value_bykey(Some(&CefString::from("rt")), Some(&mut api), fixed);
        }

        fn on_focused_node_changed(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            node: Option<&mut Domnode>,
        ) {
            let editable = node.is_some_and(|n| n.is_editable() != 0);
            let frame = frame.map(|f| f.clone()).or_else(|| browser?.main_frame());
            let Some(frame) = frame else { return };
            let user = !editable
                || eval(&frame, &CefString::from(USER_ACTIVATION_JS)).as_deref() == Ok("true");
            let Some(mut message) = process_message_create(Some(&CefString::from(FOCUS_MESSAGE))) else {
                return;
            };
            if let Some(args) = message.argument_list() {
                args.set_bool(0, editable.into());
                args.set_bool(1, user.into());
            }
            frame.send_process_message(ProcessId::BROWSER, Some(&mut message));
        }

        fn on_process_message_received(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            _source_process: ProcessId,
            message: Option<&mut ProcessMessage>,
        ) -> ::std::os::raw::c_int {
            let (Some(frame), Some(message)) = (frame, message) else { return 0 };
            let name = CefString::from(&message.name()).to_string();
            if name == GREASEMONKEY_MESSAGE {
                if let Some(args) = message.argument_list() {
                    set_scripts(&CefString::from(&args.string(0)).to_string());
                }
                return 1;
            }
            if name == GM_VALUES_MESSAGE {
                if let Some(args) = message.argument_list() {
                    set_values(&CefString::from(&args.string(0)).to_string());
                }
                return 1;
            }
            if name != EVAL_MESSAGE {
                return 0;
            }
            let Some(args) = message.argument_list() else { return 0 };
            let id = args.int(0);
            let code = CefString::from(&args.string(1));
            let result = eval(frame, &code);
            send_result(frame, id, result);
            1
        }
    }
}

wrap_v8_handler! {
    struct RtSendHandler {}

    impl V8Handler {
        fn execute(
            &self,
            _name: Option<&CefString>,
            _object: Option<&mut V8Value>,
            arguments: Option<&[Option<V8Value>]>,
            _retval: Option<&mut Option<V8Value>>,
            exception: Option<&mut CefString>,
        ) -> ::std::os::raw::c_int {
            let text = |i: usize| {
                arguments?
                    .get(i)?
                    .as_ref()
                    .filter(|v| v.is_string() != 0)
                    .map(|v| CefString::from(&v.string_value()))
            };
            let (Some(name), Some(payload)) = (text(0), text(1)) else {
                if let Some(exception) = exception {
                    *exception = CefString::from("rt.send(name, json) takes two strings");
                }
                return 1;
            };
            let frame = v8_context_get_current_context().and_then(|c| c.frame());
            let message = process_message_create(Some(&CefString::from(UI_MESSAGE)));
            if let (Some(frame), Some(mut message)) = (frame, message) {
                if let Some(args) = message.argument_list() {
                    args.set_string(0, Some(&name));
                    args.set_string(1, Some(&payload));
                }
                frame.send_process_message(ProcessId::BROWSER, Some(&mut message));
            }
            1
        }
    }
}

// `GM_setValue` / `GM_deleteValue` for one script: tell the browser.
wrap_v8_handler! {
    struct RtGmSetHandler {
        script: String,
    }

    impl V8Handler {
        fn execute(
            &self,
            _name: Option<&CefString>,
            _object: Option<&mut V8Value>,
            arguments: Option<&[Option<V8Value>]>,
            _retval: Option<&mut Option<V8Value>>,
            _exception: Option<&mut CefString>,
        ) -> ::std::os::raw::c_int {
            let text = |i: usize| {
                arguments?
                    .get(i)?
                    .as_ref()
                    .filter(|v| v.is_string() != 0)
                    .map(|v| CefString::from(&v.string_value()))
            };
            let (Some(key), Some(value)) = (text(0), text(1)) else { return 1 };
            let frame = v8_context_get_current_context().and_then(|c| c.frame());
            let message = process_message_create(Some(&CefString::from(GM_SET_MESSAGE)));
            if let (Some(frame), Some(mut message)) = (frame, message) {
                if let Some(args) = message.argument_list() {
                    args.set_string(0, Some(&CefString::from(self.script.as_str())));
                    args.set_string(1, Some(&key));
                    args.set_string(2, Some(&value));
                }
                frame.send_process_message(ProcessId::BROWSER, Some(&mut message));
            }
            1
        }
    }
}

const USER_ACTIVATION_JS: &str = "String(!!navigator.userActivation?.isActive)";

/// Evaluate `code` in the frame's main world. Replies come from this Rust code,
/// not from page JavaScript, so a page cannot forge them.
fn eval(frame: &Frame, code: &CefString) -> Result<String, String> {
    let context = frame.v8_context().ok_or("no JavaScript context")?;
    let url = CefString::from(&frame.url());
    let mut retval = None;
    let mut exception = None;
    context.enter();
    let ok = context.eval(
        Some(code),
        Some(&url),
        0,
        Some(&mut retval),
        Some(&mut exception),
    );
    context.exit();
    if ok == 0 {
        let message = exception
            .map(|e| CefString::from(&e.message()).to_string())
            .unwrap_or_else(|| "evaluation failed".to_string());
        return Err(message);
    }
    Ok(retval
        .filter(|v| v.is_string() != 0)
        .map(|v| CefString::from(&v.string_value()).to_string())
        .unwrap_or_default())
}

fn send_result(frame: &Frame, id: i32, result: Result<String, String>) {
    let Some(mut reply) = process_message_create(Some(&CefString::from(EVAL_RESULT_MESSAGE)))
    else {
        return;
    };
    if let Some(args) = reply.argument_list() {
        args.set_int(0, id);
        args.set_bool(1, result.is_ok().into());
        let text = match &result {
            Ok(s) | Err(s) => s.as_str(),
        };
        args.set_string(2, Some(&CefString::from(text)));
    }
    frame.send_process_message(ProcessId::BROWSER, Some(&mut reply));
}
