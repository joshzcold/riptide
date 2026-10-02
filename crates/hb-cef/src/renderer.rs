//! Code that runs in the renderer process.

use cef::*;

/// Renderer → browser: focus moved; argument 0 is whether the new node is editable.
pub const FOCUS_MESSAGE: &str = "hb.focus";

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
    }
}
