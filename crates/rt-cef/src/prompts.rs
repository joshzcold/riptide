//! Links engine prompts to the CEF callbacks waiting for their answers.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use rt_core::prompt::{Prompt, PromptAnswer, PromptKind};

use crate::shell;

/// JavaScript dialogs are withdrawn when their page navigates; other prompts
/// (downloads, logins, permissions) only when their tab closes.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    JsDialog,
    Other,
}

struct Pending {
    browser: Option<i32>,
    scope: Scope,
    on_answer: Box<dyn FnOnce(PromptAnswer)>,
}

thread_local! {
    static NEXT_ID: Cell<u64> = const { Cell::new(1) };
    static PENDING: RefCell<HashMap<u64, Pending>> = RefCell::new(HashMap::new());
}

/// Queue a prompt; `on_answer` runs once with the user's answer.
pub fn ask(
    browser: Option<i32>,
    scope: Scope,
    title: impl Into<String>,
    message: impl Into<String>,
    kind: PromptKind,
    on_answer: impl FnOnce(PromptAnswer) + 'static,
) -> u64 {
    ask_about(browser, scope, title, message, kind, None, false, on_answer)
}

/// [`ask`], with the URL `prompt-yank` copies and whether it's a download's
/// save prompt (`prompt-open-download`).
#[allow(clippy::too_many_arguments)]
pub fn ask_about(
    browser: Option<i32>,
    scope: Scope,
    title: impl Into<String>,
    message: impl Into<String>,
    kind: PromptKind,
    url: Option<String>,
    download: bool,
    on_answer: impl FnOnce(PromptAnswer) + 'static,
) -> u64 {
    let id = NEXT_ID.with(|n| {
        let id = n.get();
        n.set(id + 1);
        id
    });
    PENDING.with(|p| {
        p.borrow_mut().insert(
            id,
            Pending {
                browser,
                scope,
                on_answer: Box::new(on_answer),
            },
        )
    });
    let prompt = Prompt {
        id,
        title: title.into(),
        message: message.into(),
        kind,
        url,
        download,
    };
    if let Some(effects) = shell::with(|s| s.engine.push_prompt(prompt)) {
        shell::apply(effects);
    }
    id
}

pub fn answered(id: u64, answer: PromptAnswer) {
    if let Some(pending) = PENDING.with(|p| p.borrow_mut().remove(&id)) {
        (pending.on_answer)(answer);
    }
}

/// Withdraw one prompt without answering it (e.g. Chromium dismissed it).
pub fn withdraw(id: u64) {
    let removed = PENDING.with(|p| p.borrow_mut().remove(&id));
    // Dropping a CEF callback without calling it cancels the request.
    drop(removed);
    if let Some(effects) = shell::with(|s| s.engine.cancel_prompt(id)) {
        shell::apply(effects);
    }
}

/// Withdraw a browser's prompts: JavaScript dialogs only, or everything.
pub fn withdraw_for_browser(browser: i32, scope: Option<Scope>) {
    let ids: Vec<u64> = PENDING.with(|p| {
        p.borrow()
            .iter()
            .filter(|(_, pending)| {
                pending.browser == Some(browser) && scope.is_none_or(|s| s == pending.scope)
            })
            .map(|(id, _)| *id)
            .collect()
    });
    for id in ids {
        withdraw(id);
    }
}
