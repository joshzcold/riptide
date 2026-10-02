//! Code that runs in the renderer process.

use cef::*;

/// Renderer → browser: focus moved; argument 0 is whether the new node is editable.
pub const FOCUS_MESSAGE: &str = "hb.focus";
/// Browser → renderer: evaluate argument 1 (code) in the frame; argument 0 is a request id.
pub const EVAL_MESSAGE: &str = "hb.eval";
/// Renderer → browser: id, success flag, and the string result or error message.
pub const EVAL_RESULT_MESSAGE: &str = "hb.eval-result";
/// Renderer → browser, from `hb://ui/` pages only: message name and JSON payload.
pub const UI_MESSAGE: &str = "hb.ui";

wrap_render_process_handler! {
    pub struct HbRenderProcessHandler {}

    impl RenderProcessHandler {
        /// Give the browser's own UI pages (and only those) `hb.send(name, json)`.
        fn on_context_created(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            context: Option<&mut V8Context>,
        ) {
            let (Some(frame), Some(context)) = (frame, context) else { return };
            let url = CefString::from(&frame.url()).to_string();
            if !url.starts_with(hb_core::ui_message::UI_PREFIX) {
                return;
            }
            let Some(global) = context.global() else { return };
            let Some(mut hb) = v8_value_create_object(None, None) else { return };
            let mut handler = HbSendHandler::new();
            let Some(mut send) = v8_value_create_function(Some(&CefString::from("send")), Some(&mut handler)) else {
                return;
            };
            let fixed = V8Propertyattribute::from(
                sys::cef_v8_propertyattribute_t::V8_PROPERTY_ATTRIBUTE_READONLY
                    | sys::cef_v8_propertyattribute_t::V8_PROPERTY_ATTRIBUTE_DONTDELETE,
            );
            hb.set_value_bykey(Some(&CefString::from("send")), Some(&mut send), fixed);
            global.set_value_bykey(Some(&CefString::from("hb")), Some(&mut hb), fixed);
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
            let Some(mut message) = process_message_create(Some(&CefString::from(FOCUS_MESSAGE))) else {
                return;
            };
            if let Some(args) = message.argument_list() {
                args.set_bool(0, editable.into());
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
            if CefString::from(&message.name()).to_string() != EVAL_MESSAGE {
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
    struct HbSendHandler {}

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
                    *exception = CefString::from("hb.send(name, json) takes two strings");
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
