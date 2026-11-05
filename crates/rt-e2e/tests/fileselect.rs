//! Upload fields: riptide's file picker, external pickers' context and the
//! Lua hooks.

use std::path::Path;

use rt_e2e::Browser;

/// Files for the pickers to offer, under `{scratch}/files`.
fn files(scratch: &Path) {
    let root = scratch.join("files");
    std::fs::create_dir_all(root.join("docs")).unwrap();
    for file in ["notes.md", "other.txt", "docs/chosen.txt", "docs/guide.md"] {
        std::fs::write(root.join(file), "x\n").unwrap();
    }
}

/// The names the picker shows now.
fn listed(s: &rt_e2e::State) -> Vec<String> {
    s.prompt
        .as_ref()
        .and_then(|p| p["files"]["items"].as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|item| item["name"].as_str().map(String::from))
        .collect()
}

fn executable(path: &Path, script: &str) {
    std::fs::write(path, script).unwrap();
    std::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn the_prompt_picker_narrows_files_and_answers_the_field() {
    let b = Browser::launch()
        .toml("fileselect.handler = \"prompt\"\nfileselect.directory = \"{scratch}/files\"\n")
        .start("upload.html");
    files(&b.scratch());
    b.follow_hint("hint", |_| true);
    b.wait_mode("prompt");
    b.wait_until("the folder is listed", |s| {
        listed(s).contains(&"docs/chosen.txt".to_string())
    });
    b.keys("chosen");
    b.wait_until("typing narrows the list", |s| {
        listed(s) == ["docs/chosen.txt"]
    });
    b.keys("<Return>");
    b.wait_eval("document.title", "picked=chosen.txt");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn the_prompt_picker_marks_several_files_of_the_accepted_type() {
    let b = Browser::launch()
        .toml("fileselect.handler = \"prompt\"\nfileselect.directory = \"{scratch}/files\"\n")
        .start("upload-multi.html");
    files(&b.scratch());
    b.follow_hint("hint", |_| true);
    b.wait_mode("prompt");
    b.keys(".md");
    // accept=".md": other files are hidden.
    b.wait_until("only markdown files match", |s| {
        listed(s) == ["notes.md", "docs/guide.md"]
    });
    b.keys("<Tab><Tab><Return>");
    b.wait_eval("document.title", "picked=guide.md,notes.md");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn external_pickers_get_the_folder_and_accepted_extensions() {
    let b = Browser::launch()
        .toml(
            "fileselect.handler = \"external\"\nfileselect.directory = \"{scratch}/files\"\n\
             fileselect.multiple_files.command = [\"{scratch}/picker.sh\", \"{}\", \"{dir}\", \"{extensions}\"]\n",
        )
        .start("upload-multi.html");
    files(&b.scratch());
    // Makes and picks `<dir>/<mode>-<extensions>.md`.
    executable(
        &b.scratch().join("picker.sh"),
        "#!/bin/sh\nf=\"$2/$RIPTIDE_FILESELECT_MODE-$3.md\"\n: >\"$f\"\nprintf '%s\\n' \"$f\" >\"$1\"\n",
    );
    b.follow_hint("hint", |_| true);
    b.wait_eval("document.title", "picked=files-md.md");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn a_lua_hook_takes_the_field_and_rt_ui_pick_files_asks_from_lua() {
    let b = Browser::launch()
        .toml("fileselect.directory = \"{scratch}/files\"\n")
        .lua(
            r#"
rt.on("fileselect", function(e)
  rt.answer_fileselect(e.id, { e.dir .. "/notes.md" })
  return true
end)
rt.command("pick", function()
  rt.ui.pick_files({ dir = "{scratch}/files/docs", extensions = { "txt" } }, function(paths)
    rt.notify("picked " .. table.concat(paths or { "nothing" }, ","))
  end)
end)
"#,
        )
        .start("upload.html");
    files(&b.scratch());
    b.follow_hint("hint", |_| true);
    b.wait_eval("document.title", "picked=notes.md");

    b.run("pick");
    b.wait_mode("prompt");
    b.wait_until("only text files are offered", |s| {
        listed(s) == ["chosen.txt"]
    });
    b.keys("<Return>");
    let chosen = b.scratch().join("files/docs/chosen.txt");
    b.wait_until("Lua gets the path", |s| {
        s.message() == Some(&format!("picked {}", chosen.display()))
    });
}
