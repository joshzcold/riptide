//! The test channel: end-to-end tests drive the browser through the command
//! socket (`rt_config::remote::TestRequest`). Only test builds answer, i.e.
//! debug builds or the `test-control` feature; release builds refuse.

#[cfg(not(any(debug_assertions, feature = "test-control")))]
pub fn handle(
    _request: rt_config::remote::TestRequest,
) -> Result<Option<serde_json::Value>, String> {
    Err("test control isn't built in; use a debug build or --features test-control".into())
}

#[cfg(any(debug_assertions, feature = "test-control"))]
pub use enabled::handle;

#[cfg(any(debug_assertions, feature = "test-control"))]
mod enabled {
    use std::cell::RefCell;
    use std::sync::mpsc::{self, Sender};
    use std::time::Duration;

    use cef::*;
    use rt_config::remote::TestRequest;
    use rt_core::Key;
    use serde_json::{Value, json};

    use crate::shell;

    type Reply = Result<Option<Value>, String>;

    /// Long enough for a slow page's eval; the socket client waits longer.
    const TIMEOUT: Duration = Duration::from_secs(20);

    /// Runs on the socket's thread: the work happens on the UI thread, and
    /// this waits for its answer.
    pub fn handle(request: TestRequest) -> Reply {
        let (tx, rx) = mpsc::channel();
        let mut task = RunTest::new(RefCell::new(Some((request, tx))));
        if post_task(ThreadId::UI, Some(&mut task)) == 0 {
            return Err("the browser is shutting down".into());
        }
        rx.recv_timeout(TIMEOUT)
            .map_err(|_| "no answer from the UI thread".to_string())?
    }

    fn run(request: TestRequest, tx: Sender<Reply>) {
        let reply = match request {
            TestRequest::Keys { keys } => press(&keys),
            TestRequest::Run { command } => {
                crate::remote::run_commands(&[command]);
                Ok(None)
            }
            TestRequest::State => Ok(Some(state())),
            TestRequest::Eval { code, tab } => return eval(&code, tab, tx),
            TestRequest::EvalBar { code, bar } => return eval_bar(&code, &bar, tx),
            TestRequest::CrashTab => crash_tab(),
            TestRequest::Panic => panic!("a panic the test channel asked for"),
        };
        let _ = tx.send(reply);
    }

    fn press(keys: &str) -> Reply {
        let keys = Key::parse_sequence(keys).map_err(|e| e.to_string())?;
        for key in keys {
            crate::client::press(key);
        }
        Ok(None)
    }

    fn eval(code: &str, tab: Option<usize>, tx: Sender<Reply>) {
        let browser = shell::with(|s| match tab {
            Some(index) => s.tabs.get(index).and_then(|t| t.view.browser()),
            None => s.current_browser(),
        })
        .flatten();
        let Some(browser) = browser else {
            let _ = tx.send(Err(format!("no tab {tab:?}")));
            return;
        };
        crate::eval::eval(&browser, code, move |result| {
            let _ = tx.send(result.map(|text| Some(Value::String(text))));
        });
    }

    /// The renderer aborts, as a real crash would; `chrome://crash` doesn't
    /// crash a sandboxed renderer.
    fn crash_tab() -> Reply {
        let frame = shell::with(|s| s.current_browser())
            .flatten()
            .and_then(|b| b.main_frame())
            .ok_or("no current tab")?;
        let mut message =
            process_message_create(Some(&CefString::from(crate::renderer::CRASH_MESSAGE)))
                .ok_or("could not create process message")?;
        frame.send_process_message(ProcessId::RENDERER, Some(&mut message));
        Ok(None)
    }

    fn eval_bar(code: &str, bar: &str, tx: Sender<Reply>) {
        let view = shell::with(|s| match bar {
            "tabbar" => s.tabbar.clone(),
            "statusbar" => s.statusbar.clone(),
            "completion" => s.completion.clone(),
            "crash_notice" => s.crash_notice.clone(),
            _ => None,
        })
        .flatten();
        let Some(browser) = view.and_then(|v| v.browser()) else {
            let _ = tx.send(Err(format!("no {bar}")));
            return;
        };
        crate::eval::eval(&browser, code, move |result| {
            let _ = tx.send(result.map(|text| Some(Value::String(text))));
        });
    }

    /// Everything a test asserts on, in one snapshot.
    fn state() -> Value {
        shell::with(|s| {
            let windows: Vec<Value> = s
                .windows
                .iter()
                .map(|w| {
                    let tabs: Vec<Value> = w
                        .tabs
                        .iter()
                        .enumerate()
                        .map(|(i, t)| {
                            json!({
                                "url": t.url,
                                "title": t.title,
                                "pinned": w.tabs.is_pinned(i),
                                "loading": t.progress.is_some(),
                                "mode": t.mode,
                                "zoom": t.zoom,
                                "crashed": t.crashed.is_some(),
                                "search_match": t.search_match,
                                "muted": t.muted,
                                "sharing": t.sharing,
                            })
                        })
                        .collect();
                    let crash_notice = w
                        .crash_notice
                        .as_ref()
                        .is_some_and(|v| cef::View::from(v).is_visible() != 0);
                    json!({
                        "private": w.private,
                        "call": w.call,
                        "current_tab": w.tabs.current_index(),
                        "tabs": tabs,
                        "crash_notice": crash_notice,
                    })
                })
                .collect();
            // Labels with the element's text and link, so tests can pick one.
            let hints: Vec<Value> = s
                .engine
                .hint_session()
                .map(|h| {
                    h.items
                        .iter()
                        .zip(&h.labels)
                        .filter(|(_, label)| !label.is_empty())
                        .map(|(item, label)| json!({ "label": label, "text": item.text, "url": item.url }))
                        .collect()
                })
                .unwrap_or_default();
            json!({
                "mode": s.engine.mode(),
                "hints": hints,
                "status": s.engine.status(),
                "completion": s.engine.completions(),
                "prompt": s.engine.prompt_view(),
                "current_window": s.active,
                "windows": windows,
                "floats": crate::float::test_state(),
                "popup": crate::popup::test_state(),
                "panels": crate::panel::test_state(),
            })
        })
        .unwrap_or(Value::Null)
    }

    wrap_task! {
        struct RunTest {
            request: RefCell<Option<(TestRequest, Sender<Reply>)>>,
        }

        impl Task {
            fn execute(&self) {
                if let Some((request, tx)) = self.request.borrow_mut().take() {
                    run(request, tx);
                }
            }
        }
    }
}
