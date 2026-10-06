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

/// Renderer → browser: `GM_xmlhttpRequest`; arguments are the script name,
/// the request id, and the request as JSON.
pub const GM_XHR_MESSAGE: &str = "rt.gm-xhr";
/// Browser → renderer: a `GM_xmlhttpRequest` finished; arguments are the
/// request id and the response as JSON.
pub const GM_XHR_DONE_MESSAGE: &str = "rt.gm-xhr-done";
/// Renderer → browser: `GM_openInTab`; arguments are the script name, the
/// URL, and whether to open it in the background.
pub const GM_OPEN_MESSAGE: &str = "rt.gm-open";

/// The scripts, numbered so a renderer can tell which list is newest.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Scripts {
    pub generation: u64,
    pub scripts: Vec<Script>,
    /// `input.mouse.rocker_gestures`, which pages need to know about too.
    #[serde(default)]
    pub rocker_gestures: bool,
}

/// Renderer → browser: a rocker gesture; argument 0 is `back` or `forward`.
pub const ROCKER_MESSAGE: &str = "rt.rocker";

thread_local! {
    static SCRIPTS: RefCell<(u64, Vec<Script>)> = const { RefCell::new((0, Vec::new())) };
    static ROCKER: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static NEXT_XHR: std::cell::Cell<i32> = const { std::cell::Cell::new(1) };
    /// `GM_xmlhttpRequest` callbacks waiting for the browser, by request id.
    static XHR_CALLBACKS: RefCell<std::collections::HashMap<i32, (V8Context, V8Value)>> =
        RefCell::new(std::collections::HashMap::new());
}

/// JavaScript for the APIs a script must `@grant`, built on the private
/// `__rtXhr` and `__rtOpen` functions.
fn granted_api(script: &Script) -> String {
    let mut js = String::new();
    if script.grants("GM_xmlhttpRequest") {
        js.push_str(
            "const GM_xmlhttpRequest = (d) => {\n               const request = { method: d.method || 'GET', url: String(new URL(d.url, location.href)), \n                 headers: d.headers || {}, data: d.data == null ? null : String(d.data), timeout: d.timeout || 0 };\n               __rtXhr(JSON.stringify(request), (json) => {\n                 const r = JSON.parse(json);\n                 const response = { readyState: 4, status: r.status, statusText: r.statusText, \n                   responseText: r.responseText, response: r.responseText, responseHeaders: r.responseHeaders, \n                   finalUrl: r.finalUrl, error: r.error, context: d.context };\n                 if (d.responseType === 'json') { try { response.response = JSON.parse(r.responseText); } catch (e) { response.response = null; } }\n                 if (r.error === 'timeout') (d.ontimeout || d.onerror)?.(response);\n                 else if (r.error) d.onerror?.(response);\n                 else d.onload?.(response);\n                 d.onloadend?.(response);\n               });\n               return { abort() {} };\n             };\n             GM.xmlHttpRequest = (d) => new Promise((resolve, reject) => GM_xmlhttpRequest({ ...d, \n               onload: (r) => { d.onload?.(r); resolve(r); }, onerror: (r) => { d.onerror?.(r); reject(r); }, \n               ontimeout: (r) => { d.ontimeout?.(r); reject(r); } }));\n",
        );
    }
    if script.grants("GM_openInTab") {
        js.push_str(
            "const GM_openInTab = (url, options) => {\n               const background = typeof options === 'object' && options !== null \n                 ? options.active === false || options.loadInBackground === true : options === true;\n               __rtOpen(String(new URL(url, location.href)), background);\n               return { close() {}, closed: false };\n             };\n             GM.openInTab = async (url, options) => GM_openInTab(url, options);\n",
        );
    }
    js
}

fn set_scripts(json: &str) {
    match serde_json::from_str::<Scripts>(json) {
        Ok(scripts) => {
            let (generation, rocker) = (scripts.generation, scripts.rocker_gestures);
            let current = SCRIPTS.with(|s| {
                let mut state = s.borrow_mut();
                apply_scripts(&mut state, scripts);
                state.0
            });
            if current == generation {
                ROCKER.with(|r| r.set(rocker));
            }
        }
        Err(e) => tracing::warn!("bad greasemonkey scripts from the browser: {e}"),
    }
}

/// CEF hands a browser's original `extra_info` over again on reload, so an
/// older list never replaces a newer one from `:greasemonkey-reload`.
fn apply_scripts(state: &mut (u64, Vec<Script>), update: Scripts) {
    if update.generation >= state.0 {
        *state = (update.generation, update.scripts);
    }
}

/// A script's stored values changed in another page.
fn set_values(json: &str) {
    let Ok(update) = serde_json::from_str::<ValuesUpdate>(json) else {
        return;
    };
    SCRIPTS.with(|s| apply_values(&mut s.borrow_mut(), update));
}

#[derive(serde::Deserialize)]
struct ValuesUpdate {
    generation: u64,
    script: String,
    values: serde_json::Map<String, serde_json::Value>,
}

/// New values for one script. The generation moves forward, so the stale list
/// a reload brings back can't overwrite them.
fn apply_values(state: &mut (u64, Vec<Script>), update: ValuesUpdate) {
    state.0 = state.0.max(update.generation);
    if let Some(script) = state.1.iter_mut().find(|s| s.name == update.script) {
        script.values = update.values;
    }
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
                 {granted}{required}\n{code}",
                granted = granted_api(script),
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
            // The native functions are passed in as arguments rather than
            // through globals, so the page can't reach them.
            let code = format!("(function (__rtSet, __rtXhr, __rtOpen) {{\n{run}\n}})");
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
            let mut handler = RtGmXhrHandler::new(script.name.clone());
            let xhr = v8_value_create_function(Some(&CefString::from("GM_xmlhttpRequest")), Some(&mut handler));
            let mut handler = RtGmOpenHandler::new(script.name.clone());
            let open = v8_value_create_function(Some(&CefString::from("GM_openInTab")), Some(&mut handler));
            if ok != 0
                && let (Some(function), Some(setter), Some(xhr), Some(open)) = (retval, setter, xhr, open)
            {
                ok = function
                    .execute_function(None, Some(&[Some(setter), Some(xhr), Some(open)]))
                    .is_some()
                    .into();
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
                    if ROCKER.with(std::cell::Cell::get) {
                        install_rocker(context);
                    }
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
            if name == GM_XHR_DONE_MESSAGE {
                if let Some(args) = message.argument_list() {
                    xhr_done(args.int(0), &CefString::from(&args.string(1)));
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

/// `input.mouse.rocker_gestures`: with the right button held, a left click
/// goes back; with the left held, a right click goes forward. As in
/// qutebrowser, the page's context menu goes away.
const ROCKER_JS: &str = r#"(function (go) {
  let left = false, right = false, used = false;
  const stop = (e) => { e.preventDefault(); e.stopImmediatePropagation(); };
  addEventListener("mousedown", (e) => {
    if (e.button === 0) left = true;
    if (e.button === 2) right = true;
    if (left && right) { go(e.button === 0 ? "back" : "forward"); used = true; stop(e); }
  }, true);
  addEventListener("mouseup", (e) => {
    if (e.button === 0) left = false;
    if (e.button === 2) right = false;
    if (used) stop(e);
  }, true);
  addEventListener("click", (e) => { if (used) { stop(e); if (!left && !right) used = false; } }, true);
  addEventListener("contextmenu", stop, true);
})"#;

fn install_rocker(context: &V8Context) {
    let mut retval = None;
    let mut exception = None;
    context.enter();
    let ok = context.eval(
        Some(&CefString::from(ROCKER_JS)),
        Some(&CefString::from("riptide:rocker")),
        0,
        Some(&mut retval),
        Some(&mut exception),
    );
    let mut handler = RtRockerHandler::new();
    let go = v8_value_create_function(Some(&CefString::from("go")), Some(&mut handler));
    if ok != 0
        && let (Some(function), Some(go)) = (retval, go)
    {
        function.execute_function(None, Some(&[Some(go)]));
    }
    context.exit();
}

// The rocker listener's way to the browser.
wrap_v8_handler! {
    struct RtRockerHandler {}

    impl V8Handler {
        fn execute(
            &self,
            _name: Option<&CefString>,
            _object: Option<&mut V8Value>,
            arguments: Option<&[Option<V8Value>]>,
            _retval: Option<&mut Option<V8Value>>,
            _exception: Option<&mut CefString>,
        ) -> ::std::os::raw::c_int {
            let Some(direction) = arguments.and_then(|a| a.first().cloned().flatten()).filter(|v| v.is_string() != 0) else {
                return 1;
            };
            send_to_browser(ROCKER_MESSAGE, &[&CefString::from(&direction.string_value())]);
            1
        }
    }
}

/// Hand a finished `GM_xmlhttpRequest` to the script's callback.
fn xhr_done(id: i32, json: &CefString) {
    let Some((context, callback)) = XHR_CALLBACKS.with(|c| c.borrow_mut().remove(&id)) else {
        return;
    };
    if context.is_valid() == 0 || context.enter() == 0 {
        return;
    }
    let argument = v8_value_create_string(Some(json));
    callback.execute_function(None, Some(&[argument]));
    context.exit();
}

/// Send `message` with string arguments to the browser from the current frame.
fn send_to_browser(name: &str, arguments: &[&CefString]) -> bool {
    let frame = v8_context_get_current_context().and_then(|c| c.frame());
    let message = process_message_create(Some(&CefString::from(name)));
    let (Some(frame), Some(mut message)) = (frame, message) else {
        return false;
    };
    if let Some(args) = message.argument_list() {
        for (i, value) in arguments.iter().enumerate() {
            args.set_string(i, Some(value));
        }
    }
    frame.send_process_message(ProcessId::BROWSER, Some(&mut message));
    true
}

// `GM_xmlhttpRequest` for one script: the browser makes the request.
wrap_v8_handler! {
    struct RtGmXhrHandler {
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
            let Some([Some(request), Some(callback)]) = arguments.and_then(|a| a.get(..2)).map(|a| [a[0].clone(), a[1].clone()]) else {
                return 1;
            };
            if request.is_string() == 0 || callback.is_function() == 0 {
                return 1;
            }
            let Some(context) = v8_context_get_current_context() else { return 1 };
            let id = NEXT_XHR.with(|n| {
                let id = n.get();
                n.set(id + 1);
                id
            });
            XHR_CALLBACKS.with(|c| c.borrow_mut().insert(id, (context, callback)));
            let script = CefString::from(self.script.as_str());
            let id_text = CefString::from(id.to_string().as_str());
            let request = CefString::from(&request.string_value());
            if !send_to_browser(GM_XHR_MESSAGE, &[&script, &id_text, &request]) {
                XHR_CALLBACKS.with(|c| c.borrow_mut().remove(&id));
            }
            1
        }
    }
}

// `GM_openInTab` for one script: the browser opens the tab.
wrap_v8_handler! {
    struct RtGmOpenHandler {
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
            let arg = |i: usize| arguments.and_then(|a| a.get(i).cloned().flatten());
            let Some(url) = arg(0).filter(|v| v.is_string() != 0) else { return 1 };
            let background = arg(1).is_some_and(|v| v.is_bool() != 0 && v.bool_value() != 0);
            let script = CefString::from(self.script.as_str());
            let url = CefString::from(&url.string_value());
            let background = CefString::from(if background { "1" } else { "" });
            send_to_browser(GM_OPEN_MESSAGE, &[&script, &url, &background]);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn script(name: &str) -> Script {
        Script {
            name: name.into(),
            ..Script::default()
        }
    }

    #[test]
    fn an_older_script_list_never_replaces_a_newer_one() {
        let mut state = (2, vec![script("new")]);
        apply_scripts(
            &mut state,
            Scripts {
                generation: 1,
                scripts: vec![script("old")],
                rocker_gestures: false,
            },
        );
        assert_eq!(state.1[0].name, "new");
        apply_scripts(
            &mut state,
            Scripts {
                generation: 3,
                scripts: vec![script("newer")],
                rocker_gestures: false,
            },
        );
        assert_eq!((state.0, state.1[0].name.as_str()), (3, "newer"));
    }

    #[test]
    fn values_update_their_script_and_move_the_generation_on() {
        let mut state = (1, vec![script("a"), script("b")]);
        let values: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(r#"{"n": 2}"#).unwrap();
        apply_values(
            &mut state,
            ValuesUpdate {
                generation: 5,
                script: "b".into(),
                values: values.clone(),
            },
        );
        assert_eq!(state.0, 5);
        assert!(state.1[0].values.is_empty());
        assert_eq!(state.1[1].values, values);
        // A reload's original list (generation 1) is now ignored.
        apply_scripts(
            &mut state,
            Scripts {
                generation: 1,
                scripts: vec![script("b")],
                rocker_gestures: false,
            },
        );
        assert_eq!(state.1[1].values, values);
    }
}
