//! Code that runs in the renderer process.

use cef::*;

/// Renderer → browser: focus moved; argument 0 is whether the new node is editable.
pub const FOCUS_MESSAGE: &str = "hb.focus";
/// Browser → renderer: evaluate argument 1 (code) in the frame; argument 0 is a request id.
pub const EVAL_MESSAGE: &str = "hb.eval";
/// Renderer → browser: id, success flag, and the string result or error message.
pub const EVAL_RESULT_MESSAGE: &str = "hb.eval-result";

wrap_render_process_handler! {
    pub struct HbRenderProcessHandler {}

    impl RenderProcessHandler {
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
