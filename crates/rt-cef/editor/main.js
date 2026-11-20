// The editor on riptide://edit/, built into ui/editor.bundle.js by
// scripts/update-editor-bundle.sh. It exposes window.RtEditor.create().

import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { cpp } from "@codemirror/lang-cpp";
import { css } from "@codemirror/lang-css";
import { go } from "@codemirror/lang-go";
import { html } from "@codemirror/lang-html";
import { java } from "@codemirror/lang-java";
import { javascript } from "@codemirror/lang-javascript";
import { json } from "@codemirror/lang-json";
import { markdown } from "@codemirror/lang-markdown";
import { python } from "@codemirror/lang-python";
import { rust } from "@codemirror/lang-rust";
import { sql } from "@codemirror/lang-sql";
import { xml } from "@codemirror/lang-xml";
import { yaml } from "@codemirror/lang-yaml";
import {
  bracketMatching,
  defaultHighlightStyle,
  foldGutter,
  foldKeymap,
  HighlightStyle,
  indentOnInput,
  StreamLanguage,
  syntaxHighlighting,
} from "@codemirror/language";
import { dockerFile } from "@codemirror/legacy-modes/mode/dockerfile";
import { lua } from "@codemirror/legacy-modes/mode/lua";
import { nginx } from "@codemirror/legacy-modes/mode/nginx";
import { properties } from "@codemirror/legacy-modes/mode/properties";
import { shell } from "@codemirror/legacy-modes/mode/shell";
import { toml } from "@codemirror/legacy-modes/mode/toml";
import { highlightSelectionMatches, searchKeymap } from "@codemirror/search";
import { Compartment, EditorState } from "@codemirror/state";
import {
  crosshairCursor,
  drawSelection,
  EditorView,
  highlightActiveLine,
  highlightActiveLineGutter,
  highlightSpecialChars,
  keymap,
  lineNumbers,
  rectangularSelection,
} from "@codemirror/view";
import { tags } from "@lezer/highlight";
import { Vim, vim } from "@replit/codemirror-vim";

const legacy = (mode) => () => StreamLanguage.define(mode);

const BY_EXTENSION = {
  js: javascript, mjs: javascript, cjs: javascript,
  jsx: () => javascript({ jsx: true }),
  ts: () => javascript({ typescript: true }),
  tsx: () => javascript({ jsx: true, typescript: true }),
  rs: rust, py: python, pyi: python, json: json, jsonc: json,
  md: markdown, markdown: markdown,
  html: html, htm: html, xhtml: html, vue: html, svelte: html,
  css: css, scss: css, less: css,
  yml: yaml, yaml: yaml, go: go,
  c: cpp, h: cpp, cc: cpp, cpp: cpp, cxx: cpp, hpp: cpp, hh: cpp,
  java: java, kt: java, sql: sql,
  xml: xml, svg: xml, plist: xml, xsl: xml,
  sh: legacy(shell), bash: legacy(shell), zsh: legacy(shell), fish: legacy(shell),
  toml: legacy(toml), lua: legacy(lua),
  ini: legacy(properties), conf: legacy(properties), cfg: legacy(properties),
  properties: legacy(properties), env: legacy(properties),
};

const BY_NAME = {
  dockerfile: legacy(dockerFile), containerfile: legacy(dockerFile),
  "nginx.conf": legacy(nginx), ".bashrc": legacy(shell), ".zshrc": legacy(shell),
  ".profile": legacy(shell), pkgbuild: legacy(shell), "cargo.lock": legacy(toml),
};

/** The language for a file name, or null for plain text. */
function languageFor(path) {
  const name = path.split(/[\\/]/).pop().toLowerCase();
  const make = BY_NAME[name] || BY_EXTENSION[name.includes(".") ? name.split(".").pop() : ""];
  return make ? make() : null;
}

// Colors come from riptide's theme (scheme.rs adds them to the page).
const theme = EditorView.theme({
  "&": { height: "100%", color: "var(--fg)", backgroundColor: "var(--base)" },
  ".cm-scroller": { fontFamily: "var(--mono)", lineHeight: "1.5" },
  ".cm-content": { caretColor: "var(--accent)" },
  ".cm-gutters": { backgroundColor: "var(--bg)", color: "var(--muted)", border: "none" },
  ".cm-activeLine": { backgroundColor: "color-mix(in srgb, var(--fg) 6%, transparent)" },
  ".cm-activeLineGutter": { backgroundColor: "color-mix(in srgb, var(--fg) 10%, transparent)" },
  "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection": {
    backgroundColor: "color-mix(in srgb, var(--accent) 30%, transparent) !important",
  },
  ".cm-fat-cursor": { background: "var(--accent) !important", color: "var(--base) !important" },
  "&:not(.cm-focused) .cm-fat-cursor": { outline: "1px solid var(--accent)", background: "none !important" },
  ".cm-panels": { backgroundColor: "var(--bg)", color: "var(--fg)" },
  ".cm-panels input": { color: "var(--fg)", backgroundColor: "transparent", font: "inherit" },
  ".cm-vim-panel": { fontFamily: "var(--mono)", padding: "2px 6px" },
});

// Readable on dark and light themes alike: the default style is for light ones.
const darkHighlight = HighlightStyle.define([
  { tag: tags.keyword, color: "#c792ea" },
  { tag: [tags.string, tags.special(tags.string)], color: "#c3e88d" },
  { tag: [tags.number, tags.bool, tags.null, tags.atom], color: "#f78c6c" },
  { tag: tags.comment, color: "#7f8c98", fontStyle: "italic" },
  { tag: [tags.function(tags.variableName), tags.function(tags.propertyName)], color: "#82aaff" },
  { tag: [tags.typeName, tags.className, tags.namespace], color: "#ffcb6b" },
  { tag: [tags.propertyName, tags.attributeName], color: "#89ddff" },
  { tag: [tags.tagName, tags.heading], color: "#f07178", fontWeight: "bold" },
  { tag: [tags.operator, tags.punctuation], color: "#a6accd" },
  { tag: tags.link, color: "#82aaff", textDecoration: "underline" },
  { tag: tags.invalid, color: "#ff5370" },
]);

/**
 * Make the editor. `opts`: parent, doc, path, vim, dark, readOnly, and the
 * callbacks onSave(text, force) (a promise of whether it was written),
 * onQuit(force) and onChange(dirty).
 */
function create(opts) {
  const dark = opts.dark;
  const language = languageFor(opts.path);
  const readOnly = new Compartment();
  let saved = opts.doc;
  let lastDirty = false;
  let view;

  const isDirty = () => view.state.sliceDoc() !== saved;
  const save = (force) => opts.onSave(view.state.sliceDoc(), force);

  const bang = (params) => (params.argString || "").trim() === "!";
  Vim.defineEx("write", "w", (_cm, params) => save(bang(params)));
  Vim.defineEx("update", "up", () => (isDirty() ? save(false) : undefined));
  // onQuit(true) closes the tab; onQuit(false) is a refusal because of unwritten changes.
  const saveAndQuit = () => save(false).then((ok) => ok && opts.onQuit(true));
  Vim.defineEx("xit", "x", () => (isDirty() ? saveAndQuit() : opts.onQuit(true)));
  Vim.defineEx("wq", "wq", saveAndQuit);
  Vim.defineEx("quit", "q", (_cm, params) => opts.onQuit(bang(params) || !isDirty()));

  view = new EditorView({
    parent: opts.parent,
    state: EditorState.create({
      doc: opts.doc,
      extensions: [
        opts.vim ? vim({ status: true }) : [],
        lineNumbers(),
        highlightActiveLineGutter(),
        highlightSpecialChars(),
        history(),
        foldGutter(),
        drawSelection(),
        EditorState.allowMultipleSelections.of(true),
        // Keep Windows line endings: CodeMirror writes \n otherwise.
        opts.doc.includes("\r\n") ? EditorState.lineSeparator.of("\r\n") : [],
        indentOnInput(),
        syntaxHighlighting(dark ? darkHighlight : defaultHighlightStyle, { fallback: true }),
        bracketMatching(),
        rectangularSelection(),
        crosshairCursor(),
        highlightActiveLine(),
        highlightSelectionMatches(),
        keymap.of([
          { key: "Mod-s", run: () => (save(false), true) },
          indentWithTab,
          ...defaultKeymap,
          ...searchKeymap,
          ...historyKeymap,
          ...foldKeymap,
        ]),
        language || [],
        theme,
        readOnly.of(EditorState.readOnly.of(!!opts.readOnly)),
        EditorView.updateListener.of((update) => {
          if (!update.docChanged) return;
          const dirty = isDirty();
          if (dirty !== lastDirty) opts.onChange((lastDirty = dirty));
        }),
      ],
    }),
  });

  return {
    view,
    isDirty,
    /** The text was written: it's the new clean state. */
    markSaved(text) {
      saved = text;
      opts.onChange((lastDirty = isDirty()));
    },
    focus() {
      view.focus();
    },
  };
}

window.RtEditor = { create, languageFor };
