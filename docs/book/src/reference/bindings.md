# Default key bindings

<!-- Generated from the command and binding registries; regenerate with UPDATE_LUA_TYPES=1 cargo test -p rt-config. -->

Change these with `:bind`, `[bindings.<mode>]` in `config.toml` or `rt.bind()` in `config.lua`. `:help bindings` shows your current bindings, with your changes marked.

## normal mode

| Keys | Command |
|---|---|
| `$` | `scroll-to-perc --horizontal 100` |
| `'` | `mode-enter jump_mark` |
| `+` | `zoom-in` |
| `-` | `zoom-out` |
| `.` | `repeat-command` |
| `/` | `cmd-set-text /` |
| `0` | `scroll-to-perc --horizontal 0` |
| `:` | `cmd-set-text :` |
| `;I` | `hint images tab` |
| `;O` | `hint links fill :open -t -r {hint-url}` |
| `;b` | `hint all tab-bg` |
| `;d` | `hint links download` |
| `;f` | `hint all tab` |
| `;h` | `hint all hover` |
| `;i` | `hint images current` |
| `;o` | `hint links fill :open {hint-url}` |
| `;r` | `hint --rapid links tab-bg` |
| `;t` | `hint inputs` |
| `;y` | `hint links yank` |
| `<Alt-1>` | `tab-focus 1` |
| `<Alt-2>` | `tab-focus 2` |
| `<Alt-3>` | `tab-focus 3` |
| `<Alt-4>` | `tab-focus 4` |
| `<Alt-5>` | `tab-focus 5` |
| `<Alt-6>` | `tab-focus 6` |
| `<Alt-7>` | `tab-focus 7` |
| `<Alt-8>` | `tab-focus 8` |
| `<Alt-9>` | `tab-focus -1` |
| `<Alt-m>` | `tab-mute` |
| `<Ctrl-PgDown>` | `tab-next` |
| `<Ctrl-PgUp>` | `tab-prev` |
| `<Ctrl-Return>` | `selection-follow -t` |
| `<Ctrl-T>` | `undo` |
| `<Ctrl-Tab>` | `tab-focus last` |
| `<Ctrl-^>` | `tab-focus last` |
| `<Ctrl-a>` | `navigate increment` |
| `<Ctrl-b>` | `scroll-page 0 -1` |
| `<Ctrl-d>` | `scroll-page 0 0.5` |
| `<Ctrl-f>` | `scroll-page 0 1` |
| `<Ctrl-p>` | `tab-pin` |
| `<Ctrl-q>` | `quit` |
| `<Ctrl-r>` | `reload -f` |
| `<Ctrl-t>` | `open -t` |
| `<Ctrl-u>` | `scroll-page 0 -0.5` |
| `<Ctrl-v>` | `mode-enter passthrough` |
| `<Ctrl-w>` | `tab-close` |
| `<Ctrl-x>` | `navigate decrement` |
| `<Down>` | `scroll down` |
| `<Escape>` | `clear-keychain` |
| `<F11>` | `fullscreen` |
| `<F1>` | `help` |
| `<F5>` | `reload` |
| `<Return>` | `selection-follow` |
| `<Up>` | `scroll up` |
| `=` | `zoom` |
| `?` | `cmd-set-text ?` |
| `@` | `macro-run` |
| `B` | `cmd-set-text -s :quickmark-load -t` |
| `F` | `hint all tab` |
| `G` | `scroll-to-perc` |
| `H` | `back` |
| `J` | `tab-next` |
| `K` | `tab-prev` |
| `L` | `forward` |
| `M` | `bookmark-add` |
| `N` | `search-prev` |
| `O` | `cmd-set-text -s :open -t` |
| `PP` | `open -t -- {primary}` |
| `Pp` | `open -t -- {clipboard}` |
| `R` | `reload -f` |
| `T` | `cmd-set-text -s :tab-select` |
| `V` | `mode-enter caret ;; selection-toggle --line` |
| `ZQ` | `quit` |
| `ZZ` | `quit --save` |
| `[[` | `navigate prev` |
| `]]` | `navigate next` |
| `` ` `` | `mode-enter set_mark` |
| `b` | `cmd-set-text -s :quickmark-load` |
| `co` | `tab-only` |
| `d` | `tab-close` |
| `f` | `hint` |
| `g$` | `tab-focus -1` |
| `g0` | `tab-focus 1` |
| `gB` | `cmd-set-text -s :bookmark-load -t` |
| `gD` | `tab-give` |
| `gJ` | `tab-move +` |
| `gK` | `tab-move -` |
| `gO` | `cmd-set-text :open -t -r {url}` |
| `gT` | `tab-prev` |
| `gU` | `navigate up -t` |
| `g^` | `tab-focus 1` |
| `gb` | `cmd-set-text -s :bookmark-load` |
| `gf` | `view-source` |
| `gg` | `scroll-to-perc 0` |
| `gm` | `tab-move` |
| `go` | `cmd-set-text :open {url}` |
| `gt` | `cmd-set-text -s :tab-select` |
| `gu` | `navigate up` |
| `h` | `scroll left` |
| `i` | `mode-enter insert` |
| `j` | `scroll down` |
| `k` | `scroll up` |
| `l` | `scroll right` |
| `m` | `cmd-set-text -s :quickmark-add {url}` |
| `n` | `search-next` |
| `o` | `cmd-set-text -s :open` |
| `pP` | `open -- {primary}` |
| `pp` | `open -- {clipboard}` |
| `q` | `macro-record` |
| `r` | `reload` |
| `u` | `undo` |
| `v` | `mode-enter caret` |
| `wi` | `devtools` |
| `yD` | `yank -s domain` |
| `yT` | `yank -s title` |
| `yY` | `yank -s` |
| `yd` | `yank domain` |
| `yt` | `yank title` |
| `yy` | `yank` |
| `{{` | `navigate prev -t` |
| `}}` | `navigate next -t` |

## insert mode

| Keys | Command |
|---|---|
| `<Ctrl-e>` | `open-editor` |
| `<Escape>` | `mode-leave` |

## command mode

| Keys | Command |
|---|---|
| `<Alt-Backspace>` | `rl-backward-kill-word` |
| `<Alt-b>` | `rl-backward-word` |
| `<Alt-d>` | `rl-kill-word` |
| `<Alt-f>` | `rl-forward-word` |
| `<Backspace>` | `rl-backward-delete-char` |
| `<Ctrl-C>` | `completion-item-yank --sel` |
| `<Ctrl-a>` | `rl-beginning-of-line` |
| `<Ctrl-b>` | `rl-backward-char` |
| `<Ctrl-c>` | `completion-item-yank` |
| `<Ctrl-d>` | `completion-item-del` |
| `<Ctrl-e>` | `rl-end-of-line` |
| `<Ctrl-f>` | `rl-forward-char` |
| `<Ctrl-h>` | `rl-backward-delete-char` |
| `<Ctrl-k>` | `rl-kill-line` |
| `<Ctrl-n>` | `command-history-next` |
| `<Ctrl-p>` | `command-history-prev` |
| `<Ctrl-u>` | `rl-unix-line-discard` |
| `<Ctrl-w>` | `rl-rubout` |
| `<Ctrl-y>` | `rl-yank` |
| `<Delete>` | `rl-delete-char` |
| `<Down>` | `command-history-next` |
| `<End>` | `rl-end-of-line` |
| `<Escape>` | `mode-leave` |
| `<Home>` | `rl-beginning-of-line` |
| `<Left>` | `rl-backward-char` |
| `<Return>` | `command-accept` |
| `<Right>` | `rl-forward-char` |
| `<Shift-Tab>` | `completion-item-focus prev` |
| `<Tab>` | `completion-item-focus next` |
| `<Up>` | `command-history-prev` |

## passthrough mode

| Keys | Command |
|---|---|
| `<Shift-Escape>` | `mode-leave` |

## hint mode

| Keys | Command |
|---|---|
| `<Escape>` | `mode-leave` |
| `<Return>` | `hint-follow` |

## prompt mode

| Keys | Command |
|---|---|
| `<Alt-Backspace>` | `rl-backward-kill-word` |
| `<Alt-b>` | `rl-backward-word` |
| `<Alt-d>` | `rl-kill-word` |
| `<Alt-e>` | `prompt-fileselect-external` |
| `<Alt-f>` | `rl-forward-word` |
| `<Backspace>` | `rl-backward-delete-char` |
| `<Ctrl-a>` | `rl-beginning-of-line` |
| `<Ctrl-b>` | `rl-backward-char` |
| `<Ctrl-e>` | `rl-end-of-line` |
| `<Ctrl-f>` | `rl-forward-char` |
| `<Ctrl-h>` | `rl-backward-delete-char` |
| `<Ctrl-k>` | `rl-kill-line` |
| `<Ctrl-u>` | `rl-unix-line-discard` |
| `<Ctrl-w>` | `rl-filename-rubout` |
| `<Ctrl-y>` | `rl-yank` |
| `<Delete>` | `rl-delete-char` |
| `<End>` | `rl-end-of-line` |
| `<Escape>` | `mode-leave` |
| `<Home>` | `rl-beginning-of-line` |
| `<Left>` | `rl-backward-char` |
| `<Return>` | `prompt-accept` |
| `<Right>` | `rl-forward-char` |
| `<Tab>` | `prompt-complete` |

## yesno mode

| Keys | Command |
|---|---|
| `<Escape>` | `mode-leave` |
| `<Return>` | `prompt-accept` |
| `A` | `prompt-accept --save yes` |
| `N` | `prompt-accept --save no` |
| `n` | `prompt-accept no` |
| `y` | `prompt-accept yes` |

## set_mark mode

| Keys | Command |
|---|---|
| `<Escape>` | `mode-leave` |

## jump_mark mode

| Keys | Command |
|---|---|
| `<Escape>` | `mode-leave` |

## record_macro mode

| Keys | Command |
|---|---|
| `<Escape>` | `mode-leave` |

## run_macro mode

| Keys | Command |
|---|---|
| `<Escape>` | `mode-leave` |

## caret mode

| Keys | Command |
|---|---|
| `$` | `move-to-end-of-line` |
| `0` | `move-to-start-of-line` |
| `<Ctrl-Space>` | `selection-drop` |
| `<Escape>` | `mode-leave` |
| `<Return>` | `yank selection` |
| `<Space>` | `selection-toggle` |
| `G` | `move-to-end-of-document` |
| `V` | `selection-toggle --line` |
| `Y` | `yank -s selection` |
| `[` | `move-to-start-of-prev-block` |
| `]` | `move-to-start-of-next-block` |
| `b` | `move-to-prev-word` |
| `e` | `move-to-end-of-word` |
| `gg` | `move-to-start-of-document` |
| `h` | `move-to-prev-char` |
| `j` | `move-to-next-line` |
| `k` | `move-to-prev-line` |
| `l` | `move-to-next-char` |
| `o` | `selection-reverse` |
| `v` | `selection-toggle` |
| `w` | `move-to-next-word` |
| `y` | `yank selection` |
| `{` | `move-to-end-of-prev-block` |
| `}` | `move-to-end-of-next-block` |
