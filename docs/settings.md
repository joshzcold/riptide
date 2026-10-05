# Settings

<!-- Generated from crates/rt-core/src/settings.rs; regenerate with UPDATE_LUA_TYPES=1 cargo test -p rt-config. -->

Set these in `config.toml` (`hints.chars = "asdf"`), `config.lua` (`c.hints.chars = "asdf"`) or with `:set hints.chars asdf`.

| Setting | Type | Default | Description |
|---|---|---|---|
| `aliases` | table<string, string> | `{"q":"quit","qa":"quit","wq":"quit --save"}` | Command aliases: name → command |
| `auto_save.interval` | integer | `15000` | Milliseconds between crash-recovery saves of the open tabs (0 turns them off) |
| `auto_save.session` | boolean | `false` | Save the open tabs as the 'default' session on quit, and restore them at startup |
| `bindings.key_mappings` | table<string, string> | `{"<Ctrl-6>":"<Ctrl-^>","<Ctrl-[>":"<Escape>","<Ctrl-i>":"<Tab>","<Ctrl-j>":"<Return>","<Ctrl-m>":"<Return>","<Shift-Return>":"<Return>"}` | Keys treated as other keys in every mode, before bindings are looked up, e.g. Ctrl-[ as Escape |
| `colors.webpage.darkmode.enabled` | boolean | `false` | Render light pages dark with Chromium's automatic dark mode (takes effect after a restart) |
| `colors.webpage.preferred_color_scheme` | auto \| light \| dark | `auto` | The color scheme pages see in prefers-color-scheme: auto follows the system |
| `completion.cmd_history_max_items` | integer | `100` | How many command lines Up and Down remember |
| `completion.height` | string | `12` | Height of the completion list: rows (12) or a percentage of the window (50%) |
| `completion.min_chars` | integer | `0` | Characters to type after a command before its arguments complete |
| `completion.open_categories` | string[] | `["searchengines","quickmarks","bookmarks","history","filesystem"]` | What :open completes from, in order: searchengines, quickmarks, bookmarks, history, filesystem |
| `completion.show` | always \| auto \| never | `always` | When to show completions: always, only after pressing Tab (auto), or never |
| `completion.web_history.exclude` | string[] | `[]` | URL globs (e.g. *://*.bank.example/*) that :open never suggests from history |
| `completion.web_history.max_items` | integer | `100` | How many history entries :open completion shows (0 turns history completion off) |
| `confirm_quit` | string[] | `["never"]` | Ask before quitting: always, multiple-tabs (more than one tab open), downloads (downloads still running), or never |
| `content.blocking.adblock.lists` | string[] | `["https://easylist.to/easylist/easylist.txt","https://easylist.to/easylist/easyprivacy.txt"]` | Adblock Plus filter lists that :adblock-update downloads (https://, or file:// for local lists) |
| `content.blocking.enabled` | boolean | `true` | Block ads and trackers with the filter lists from content.blocking.adblock.lists |
| `content.blocking.whitelist` | string[] | `[]` | Sites where nothing is blocked, as host names; a host also covers its subdomains |
| `content.cookies.accept` | all \| no-3rdparty \| no-unknown-3rdparty \| never | `all` | Which cookies sites may set: all, none from other sites (no-3rdparty; no-unknown-3rdparty is the same here), or never |
| `content.cookies.store` | boolean | `true` | Keep cookies after the browser closes; false makes every cookie last only for the session |
| `content.desktop_capture` | ask \| true \| false | `ask` | Let sites capture your screen or desktop audio: ask, true or false |
| `content.geolocation` | ask \| true \| false | `ask` | Let sites know your location: ask, true or false |
| `content.headers.user_agent` | string | `` | User agent sent to sites and shown to their scripts; empty for Chromium's own. Can be set per site |
| `content.javascript.enabled` | boolean | `true` | Run JavaScript on pages; can be set per site |
| `content.media.audio_capture` | ask \| true \| false | `ask` | Let sites use your microphone: ask, true or false |
| `content.media.video_capture` | ask \| true \| false | `ask` | Let sites use your camera: ask, true or false |
| `content.notifications.enabled` | ask \| true \| false | `ask` | Let sites show notifications: ask, true or false |
| `content.tls.certificate_errors` | ask \| block \| load-insecurely | `ask` | Pages whose TLS certificate isn't trusted: ask, block, or load-insecurely |
| `content.widevine` | boolean | `false` | Allow Widevine DRM: Chromium downloads Google's CDM once (takes effect after a restart) |
| `downloads.location.directory` | string | `` | Where downloads go; empty means the system Downloads folder |
| `downloads.location.prompt` | boolean | `true` | Ask where to save each download (false saves straight to the directory) |
| `editor.command` | string[] | `["gvim","-f","{file}","-c","normal {line}G{column0}l"]` | Editor for :open-editor; fields: {file}, {line}, {column}, {line0}, {column0} |
| `fileselect.folder.command` | string[] | `["xterm","-e","ranger","--choosedir={}"]` | Program that picks a folder for fileselect.handler = external; {} is the file it writes the path to |
| `fileselect.handler` | default \| external | `default` | File pickers for upload fields: Chromium's own (default), or the fileselect.*.command programs (external) |
| `fileselect.multiple_files.command` | string[] | `["xterm","-e","ranger","--choosefiles={}"]` | Program that picks several files for fileselect.handler = external; {} is the file it writes the paths to, one per line |
| `fileselect.single_file.command` | string[] | `["xterm","-e","ranger","--choosefile={}"]` | Program that picks a file for fileselect.handler = external; {} is the file it writes the path to |
| `hints.auto_follow` | always \| unique-match \| full-match \| never | `unique-match` | When a hint is followed without Return: when one is left (unique-match), only when its label is typed in full (full-match), always, or never |
| `hints.auto_follow_timeout` | integer | `0` | Ignore keys for this many milliseconds after following a hint, so extra typing doesn't reach the page |
| `hints.chars` | string | `asdfghjkl` | Characters used for hint labels |
| `hints.mode` | letter \| number | `letter` | letter: labels from hints.chars; number: numbered labels, and typing letters filters by text |
| `hints.selectors` | table<string, string> | `{"all":"a, area, textarea, select, input:not([type=hidden]), button, iframe, summary, [contenteditable]:not([contenteditable=false]), [onclick], [onmousedown], [role=link], [role=option], [role=button], [role=tab], [role=checkbox], [role=switch], [role=menuitem], [role=menuitemcheckbox], [role=menuitemradio], [role=treeitem], [aria-haspopup], [tabindex]:not([tabindex='-1'])","images":"img","inputs":"input:not([type]), input[type=text], input[type=search], input[type=email], input[type=url], input[type=tel], input[type=password], input[type=number], input[type=date], input[type=datetime-local], input[type=month], input[type=time], input[type=week], textarea, [contenteditable]:not([contenteditable=false])","links":"a[href], area[href], [role=link][href]","media":"audio, img, video"}` | Hint groups for :hint, as CSS selector lists; your entries are added to the built-in all, links, images, media and inputs |
| `hints.uppercase` | boolean | `false` | Show hint labels in upper case |
| `input.forward_unbound_keys` | all \| auto \| none | `auto` | Pass unbound keys to the page in normal mode (auto: all but plain letters and digits) |
| `input.insert_mode.auto_enter` | boolean | `true` | Enter insert mode when an editable element gets focus |
| `input.insert_mode.auto_leave` | boolean | `true` | Leave insert mode when focus leaves an editable element |
| `input.insert_mode.auto_load` | boolean | `false` | Enter insert mode when a page focuses a text field by itself, as autofocus does on load |
| `input.insert_mode.leave_on_load` | boolean | `true` | Leave insert mode when a new page starts loading |
| `keyhint.blacklist` | string[] | `[]` | Key chains the key hint popup leaves out, as globs on the whole chain (e.g. g* for every chain starting with g) |
| `keyhint.delay` | integer | `500` | How long after a partial key chain the popup listing its continuations appears, in milliseconds |
| `messages.timeout` | integer | `3000` | Milliseconds before a status bar message clears (0 keeps it) |
| `new_instance_open_target` | tab \| tab-bg \| window | `tab` | Where URLs from a second riptide invocation open |
| `scrolling.smooth` | boolean | `false` | Animate scrolling by keys instead of jumping |
| `search.ignore_case` | smart \| always \| never | `smart` | Case in searches: smart ignores it unless the text has a capital, always, or never |
| `search.incremental` | boolean | `true` | Search while typing after / or ? |
| `search.wrap` | boolean | `true` | Go on from the top when a search passes the last match (or from the bottom, searching up) |
| `search.wrap_messages` | boolean | `true` | Say when a search wraps around the page |
| `session.lazy_restore` | boolean | `false` | When restoring a session, load background tabs only when they are first shown |
| `spellcheck.languages` | string[] | `[]` | Spell-check languages such as en-US (empty: off); Chromium downloads each dictionary from Google once |
| `statusbar.position` | top \| bottom | `bottom` | Where the status bar is |
| `statusbar.show` | always \| never \| in-mode | `always` | When to show the status bar: always, only while typing a command or answering a prompt (never), or also outside normal mode and while a message is shown (in-mode) |
| `statusbar.widgets` | string[] | `["keypress","downloads","muted","zoom","search_match","url","scroll","history","tabs","progress"]` | What the right side of the status bar shows, in order: keypress, downloads, muted, zoom, search_match, url, scroll, scroll_raw, history, tabs, progress, clock[:strftime format], text:… |
| `tabs.favicons.show` | always \| never \| pinned | `always` | Show site icons in the tab bar: always, never, or only on pinned tabs |
| `tabs.last_close` | ignore \| blank \| startpage \| default-page \| close | `ignore` | What closing the last tab does |
| `tabs.mode_on_change` | normal \| persist \| restore | `normal` | Mode after switching tabs: normal, persist (keep insert/passthrough), or restore (the mode the tab was left in) |
| `tabs.mousewheel_switching` | boolean | `true` | Switch tabs with the mouse wheel over the tab bar |
| `tabs.new_position.related` | prev \| next \| first \| last | `next` | Where tabs opened from a page go (popups, hints) |
| `tabs.new_position.unrelated` | prev \| next \| first \| last | `last` | Where other new tabs go (:open -t) |
| `tabs.pinned.frozen` | boolean | `true` | Keep pinned tabs on their page: :open in a pinned tab opens a new tab |
| `tabs.pinned.shrink` | boolean | `true` | Shrink pinned tabs to their icon and number |
| `tabs.position` | top \| bottom \| left \| right | `top` | Where the tab bar is; left and right list the tabs vertically |
| `tabs.select_on_remove` | next \| prev \| last-used | `next` | Which tab to show after closing the current one: the next, the previous, or the one used before |
| `tabs.show` | always \| never \| multiple \| switching | `always` | When to show the tab bar: always, never, with more than one tab, or briefly after switching tabs |
| `tabs.show_switching_delay` | integer | `800` | How long the tab bar stays after switching tabs with tabs.show = switching, in milliseconds |
| `tabs.title.format` | string | `{audio}{index}: {current_title}` | Tab titles; fields: {index}, {aligned_index}, {current_title}, {current_url}, {host}, {perc}, {audio}, {private} |
| `tabs.title.format_pinned` | string | `{index}` | Titles of pinned tabs while tabs.pinned.shrink shrinks them; same fields as tabs.title.format |
| `tabs.tooltips` | boolean | `true` | Show a tab's title and URL when the mouse rests on it |
| `tabs.undo_stack_size` | integer | `100` | How many closed tabs u can reopen; 0 keeps none |
| `tabs.width` | integer | `200` | Width of the tab bar in pixels when tabs.position is left or right |
| `tabs.wrap` | boolean | `true` | Wrap around from the last tab to the first (and back) when switching tabs |
| `url.default_page` | string | `https://start.duckduckgo.com/` | Page for :open without a URL |
| `url.searchengines` | table<string, string> | `{"DEFAULT":"https://duckduckgo.com/?q={}"}` | Search engines; ':open g rust' uses the 'g' entry, anything else DEFAULT |
| `url.start_pages` | string[] | `["https://start.duckduckgo.com/"]` | Pages opened at startup when no URL is given |
| `url.yank_ignored_parameters` | string[] | `["ref","utm_source","utm_medium","utm_campaign","utm_term","utm_content","utm_name","fbclid","gclid"]` | Query parameters dropped when yanking a URL, such as tracking tags |
| `window.title_format` | string | `{current_title}{title_sep}Riptide` | Window title; fields: {current_title}, {title_sep}, {current_url}, {host}, {mode} |
| `zoom.default` | integer | `100` | Zoom in percent for pages, and what :zoom without a value resets to |
| `zoom.levels` | string[] | `["25%","33%","50%","67%","75%","90%","100%","110%","125%","150%","175%","200%","250%","300%","400%","500%"]` | The zoom levels + and - step through, in percent |
