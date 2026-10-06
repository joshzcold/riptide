# Settings

<!-- Generated from crates/rt-core/src/settings.rs; regenerate with UPDATE_LUA_TYPES=1 cargo test -p rt-config. -->

Set these in `config.toml` (`hints.chars = "asdf"`), `config.lua` (`c.hints.chars = "asdf"`) or with `:set hints.chars asdf`.

| Setting | Type | Default | Description |
|---|---|---|---|
| `aliases` | table<string, string> | `{"q":"quit","qa":"quit","wq":"quit --save"}` | Command aliases: name → command |
| `auto_save.interval` | integer | `15000` | Milliseconds between crash-recovery saves of the open tabs (0 turns them off) |
| `auto_save.session` | boolean | `false` | Save the open tabs as the 'default' session on quit, and restore them at startup |
| `bindings.key_mappings` | table<string, string> | `{"<Ctrl-6>":"<Ctrl-^>","<Ctrl-[>":"<Escape>","<Ctrl-i>":"<Tab>","<Ctrl-j>":"<Return>","<Ctrl-m>":"<Return>","<Shift-Return>":"<Return>"}` | Keys treated as other keys in every mode, before bindings are looked up, e.g. Ctrl-[ as Escape |
| `changelog_after_upgrade` | major \| minor \| patch \| never | `minor` | Open the changelog in a tab after an upgrade of at least this size: major, minor, patch or never |
| `colors.completion.category.bg` | string | `` | Background of completion category headers; empty uses ui.theme's |
| `colors.completion.category.fg` | string | `` | Text of completion category headers; empty uses ui.theme's |
| `colors.completion.description.fg` | string | `` | Descriptions and details in the completion list; empty uses ui.theme's |
| `colors.completion.fg` | string | `` | Completion list text; empty uses ui.theme's |
| `colors.completion.item.selected.bg` | string | `` | Background of the selected completion; empty uses ui.theme's |
| `colors.completion.item.selected.fg` | string | `` | Text of the selected completion; empty uses ui.theme's |
| `colors.completion.odd.bg` | string | `` | Completion list background; empty uses ui.theme's |
| `colors.hints.bg` | string | `` | Background of hint labels; empty uses ui.theme's |
| `colors.hints.border` | string | `` | Border of hint labels; empty uses ui.theme's |
| `colors.hints.fg` | string | `` | Text of hint labels; empty uses ui.theme's |
| `colors.hints.match.fg` | string | `` | The typed part of hint labels; empty uses ui.theme's |
| `colors.keyhint.suffix.fg` | string | `` | The keys still to type in the key hint popup; empty uses ui.theme's |
| `colors.messages.error.bg` | string | `` | Background of error messages; empty uses ui.theme's |
| `colors.messages.error.fg` | string | `` | Text of error messages; empty uses ui.theme's |
| `colors.messages.warning.bg` | string | `` | Background of warnings; empty uses ui.theme's |
| `colors.messages.warning.fg` | string | `` | Text of warnings; empty uses ui.theme's |
| `colors.prompts.bg` | string | `` | Background of prompts; empty uses ui.theme's |
| `colors.prompts.border` | string | `` | Frame, title and keys of floating prompts; empty uses ui.theme's |
| `colors.prompts.fg` | string | `` | Text of prompts; empty uses ui.theme's |
| `colors.prompts.key.bg` | string | `` | Background of a floating prompt's keys; empty uses ui.theme's |
| `colors.statusbar.insert.bg` | string | `` | Status bar background in insert mode; empty uses ui.theme's |
| `colors.statusbar.insert.fg` | string | `` | Status bar text in insert mode; empty uses ui.theme's |
| `colors.statusbar.normal.bg` | string | `` | Status bar background; empty uses ui.theme's |
| `colors.statusbar.normal.fg` | string | `` | Status bar text; empty uses ui.theme's |
| `colors.statusbar.passthrough.bg` | string | `` | Status bar background in passthrough mode; empty uses ui.theme's |
| `colors.statusbar.passthrough.fg` | string | `` | Status bar text in passthrough mode; empty uses ui.theme's |
| `colors.statusbar.private.bg` | string | `` | Status bar background in private windows; empty uses ui.theme's |
| `colors.statusbar.private.fg` | string | `` | Status bar text in private windows; empty uses ui.theme's |
| `colors.statusbar.url.error.fg` | string | `` | The address of a page that failed to load; empty uses ui.theme's |
| `colors.statusbar.url.success.http.fg` | string | `` | An http:// address in the status bar; empty uses ui.theme's |
| `colors.statusbar.url.success.https.fg` | string | `` | An https:// address in the status bar; empty uses ui.theme's |
| `colors.tabs.bar.bg` | string | `` | Tab bar background behind the tabs; empty uses ui.theme's |
| `colors.tabs.even.bg` | string | `` | Background of even-numbered tabs; empty uses ui.theme's |
| `colors.tabs.indicator.error` | string | `` | A tab's indicator when its page failed to load; empty uses ui.theme's |
| `colors.tabs.indicator.start` | string | `` | A tab's loading indicator; empty uses ui.theme's |
| `colors.tabs.odd.bg` | string | `` | Background of odd-numbered tabs; empty uses ui.theme's |
| `colors.tabs.odd.fg` | string | `` | Text of tabs; empty uses ui.theme's |
| `colors.tabs.pinned.odd.bg` | string | `` | Background of pinned tabs; empty uses ui.theme's |
| `colors.tabs.selected.accent` | string | `` | Color of the line marking the current tab, any CSS color such as #2ec4b6; empty matches the tab, so no line shows |
| `colors.tabs.selected.odd.bg` | string | `` | Background of the current tab; empty uses ui.theme's |
| `colors.tabs.selected.odd.fg` | string | `` | Text of the current tab; empty uses ui.theme's |
| `colors.webpage.darkmode.enabled` | boolean | `false` | Render light pages dark with Chromium's automatic dark mode (takes effect after a restart) |
| `colors.webpage.preferred_color_scheme` | auto \| light \| dark | `auto` | The color scheme pages see in prefers-color-scheme: auto follows the system |
| `completion.cmd_history_max_items` | integer | `100` | How many command lines Up and Down remember |
| `completion.delay` | integer | `0` | Milliseconds to wait after a key press before updating completions |
| `completion.height` | string | `12` | Height of the completion list: rows (12) or a percentage of the window (50%) |
| `completion.min_chars` | integer | `0` | Characters to type after a command before its arguments complete |
| `completion.open_categories` | string[] | `["searchengines","quickmarks","bookmarks","history","filesystem"]` | What :open completes from, in order: searchengines, quickmarks, bookmarks, history, filesystem |
| `completion.quick` | boolean | `true` | When only one command or setting name is left, Tab takes it and moves on to completing the next part |
| `completion.show` | always \| auto \| never | `always` | When to show completions: always, only after pressing Tab (auto), or never |
| `completion.shrink` | boolean | `true` | Shrink the completion list to its items; false keeps it completion.height tall |
| `completion.timestamp_format` | string | `%Y-%m-%d %H:%M` | strftime format of the last-visit time shown next to history completions; empty hides it |
| `completion.use_best_match` | boolean | `false` | Return runs the first command that starts with an unknown command name, so :rel runs :reload |
| `completion.web_history.exclude` | string[] | `[]` | URL globs (e.g. *://*.bank.example/*) that :open never suggests from history |
| `completion.web_history.max_items` | integer | `100` | How many history entries :open completion shows (0 turns history completion off) |
| `confirm_quit` | string[] | `["never"]` | Ask before quitting: always, multiple-tabs (more than one tab open), downloads (downloads still running), or never |
| `content.autoplay` | boolean | `true` | Let videos play by themselves; false waits until you interact with the page (after a restart) |
| `content.blocking.adblock.lists` | string[] | `["https://easylist.to/easylist/easylist.txt","https://easylist.to/easylist/easyprivacy.txt"]` | Adblock Plus filter lists that :adblock-update downloads (https://, or file:// for local lists) |
| `content.blocking.enabled` | boolean | `true` | Block ads and trackers with the filter lists from content.blocking.adblock.lists |
| `content.blocking.whitelist` | string[] | `[]` | Sites where nothing is blocked, as host names; a host also covers its subdomains |
| `content.cache.size` | integer | `0` | Disk cache size in bytes; 0 lets Chromium choose (takes effect after a restart) |
| `content.canvas_reading` | boolean | `true` | Let pages read back what they drew on a canvas; false blocks a common fingerprinting trick but breaks some sites (after a restart) |
| `content.cookies.accept` | all \| no-3rdparty \| no-unknown-3rdparty \| never | `all` | Which cookies sites may set: all, none from other sites (no-3rdparty; no-unknown-3rdparty is the same here), or never |
| `content.cookies.store` | boolean | `true` | Keep cookies after the browser closes; false makes every cookie last only for the session |
| `content.desktop_capture` | ask \| true \| false | `ask` | Let sites capture your screen or desktop audio: ask, true or false |
| `content.dns_prefetch` | boolean | `true` | Look up the hosts of links before you follow them, which is faster but tells your DNS server about them |
| `content.geolocation` | ask \| true \| false | `ask` | Let sites know your location: ask, true or false |
| `content.headers.accept_language` | string | `` | Languages sites are asked for, e.g. en-US,en;q=0.9 (also navigator.languages); empty for the system's |
| `content.headers.custom` | table<string, string> | `{}` | Extra headers sent with every request: name → value |
| `content.headers.do_not_track` | boolean | `true` | Send DNT: 1 with every request, asking sites not to track you |
| `content.headers.referer` | always \| never \| same-domain | `same-domain` | When to send the Referer header: always, never, or only within the same domain and its subdomains |
| `content.headers.user_agent` | string | `` | User agent sent to sites and shown to their scripts; empty for Chromium's own. Can be set per site |
| `content.images` | boolean | `true` | Load images; can be set per site |
| `content.javascript.can_close_tabs` | boolean | `true` | Let a page close its own tab with window.close(), as login popups do |
| `content.javascript.can_open_tabs_automatically` | boolean | `false` | Let pages open tabs and windows without a click (popups); can be set per site |
| `content.javascript.clipboard` | none \| access \| access-paste | `access` | What pages may do with the clipboard: nothing, copy with a click (access), or also read it (access-paste); can be set per site |
| `content.javascript.enabled` | boolean | `true` | Run JavaScript on pages; can be set per site |
| `content.javascript.log_message.levels` | string[] | `[]` | Console messages from pages shown in the status bar and :messages, by level: debug, info, warning, error (can be set per site) |
| `content.local_content_can_access_file_urls` | boolean | `false` | Let file:// pages read other local files, which a downloaded page could misuse (after a restart) |
| `content.media.audio_capture` | ask \| true \| false | `ask` | Let sites use your microphone: ask, true or false |
| `content.media.video_capture` | ask \| true \| false | `ask` | Let sites use your camera: ask, true or false |
| `content.mouse_lock` | ask \| true \| false | `ask` | Let sites lock your mouse pointer, as games do: ask, true or false |
| `content.mute` | boolean | `false` | Mute pages; can be set per site |
| `content.notifications.enabled` | ask \| true \| false | `ask` | Let sites show notifications: ask, true or false |
| `content.notifications.presenter` | auto \| messages | `auto` | Where page notifications show: auto (the desktop's notifications) or messages (riptide's status bar) |
| `content.notifications.show_origin` | boolean | `true` | Start notification messages with the site they came from (presenter = messages) |
| `content.pdf_viewer` | boolean | `true` | Show PDFs in the browser; false downloads them instead |
| `content.prefers_reduced_motion` | boolean | `false` | Tell pages you prefer less motion, so they can tone down animations (after a restart) |
| `content.proxy` | string | `system` | Proxy: system, none, a proxy URL such as socks5://127.0.0.1:9050, or pac+ and a PAC script's URL |
| `content.register_protocol_handler` | ask \| true \| false | `ask` | Let sites register to handle links like mailto: : ask, true or false |
| `content.tls.certificate_errors` | ask \| block \| load-insecurely | `ask` | Pages whose TLS certificate isn't trusted: ask, block, or load-insecurely |
| `content.unknown_url_scheme_policy` | ask \| allow-all \| disallow | `ask` | Links to schemes the browser can't show (mailto:, magnet:, zoommtg:): ask before handing them to xdg-open, always hand them over, or never |
| `content.webgl` | boolean | `true` | Allow WebGL, which 3D graphics need and fingerprinting scripts use (after a restart) |
| `content.webrtc_ip_handling_policy` | all-interfaces \| default-public-and-private-interfaces \| default-public-interface-only \| disable-non-proxied-udp | `all-interfaces` | Which IP addresses WebRTC (video calls) may reveal; disable-non-proxied-udp keeps it behind content.proxy |
| `content.widevine` | boolean | `false` | Allow Widevine DRM: Chromium downloads Google's CDM once (takes effect after a restart) |
| `downloads.location.directory` | string | `` | Where downloads go; empty means the system Downloads folder |
| `downloads.location.prompt` | boolean | `true` | Ask where to save each download (false saves straight to the directory) |
| `downloads.location.remember` | boolean | `true` | Start the save prompt in the folder the last download went to |
| `downloads.location.suggestion` | both \| path \| filename | `both` | What the save prompt starts with: the folder and file name (both), the folder (path), or the file name |
| `downloads.open_dispatcher` | string | `` | Program that opens downloads (:download-open); {} is the file, or it's added at the end. Empty for the desktop's default |
| `downloads.remove_finished` | integer | `-1` | Take finished downloads off the list after this many milliseconds; -1 keeps them |
| `editor.command` | string[] | `["gvim","-f","{file}","-c","normal {line}G{column0}l"]` | Editor for :open-editor; fields: {file}, {line}, {column}, {line0}, {column0} |
| `editor.remove_file` | boolean | `true` | Delete the temporary file after the editor closes; false keeps it, e.g. to recover text |
| `fileselect.folder.command` | string[] | `["xterm","-e","ranger","--choosedir={}"]` | Program that picks a folder for fileselect.handler = external; {} is the file it writes the path to |
| `fileselect.handler` | default \| external | `default` | File pickers for upload fields: Chromium's own (default), or the fileselect.*.command programs (external) |
| `fileselect.multiple_files.command` | string[] | `["xterm","-e","ranger","--choosefiles={}"]` | Program that picks several files for fileselect.handler = external; {} is the file it writes the paths to, one per line |
| `fileselect.single_file.command` | string[] | `["xterm","-e","ranger","--choosefile={}"]` | Program that picks a file for fileselect.handler = external; {} is the file it writes the path to |
| `hints.auto_follow` | always \| unique-match \| full-match \| never | `unique-match` | When a hint is followed without Return: when one is left (unique-match), only when its label is typed in full (full-match), always, or never |
| `hints.auto_follow_timeout` | integer | `0` | Ignore keys for this many milliseconds after following a hint, so extra typing doesn't reach the page |
| `hints.chars` | string | `asdfghjkl` | Characters used for hint labels |
| `hints.dictionary` | string | `/usr/share/dict/words` | Word list for hints.mode = word, one word per line |
| `hints.hide_unmatched_rapid_hints` | boolean | `true` | In rapid hint mode (:hint --rapid), hide the labels that don't match what's typed |
| `hints.leave_on_load` | boolean | `true` | Leave hint mode when the page starts loading something new |
| `hints.min_chars` | integer | `1` | The shortest hint label, in characters |
| `hints.mode` | letter \| number \| word | `letter` | letter: labels from hints.chars; number: numbered labels, and typing letters filters by text; word: dictionary words from each link's text |
| `hints.next_regexes` | string[] | `["\\bnext\\b","\\bmore\\b","\\bnewer\\b","\\b[>→≫]\\b","\\b(>>\|»)\\b","\\bcontinue\\b"]` | Link texts ]] follows to the next page, as JavaScript regular expressions (case doesn't matter) |
| `hints.prev_regexes` | string[] | `["\\bprev(ious)?\\b","\\bback\\b","\\bolder\\b","\\b[<←≪]\\b","\\b(<<\|«)\\b"]` | Link texts [[ follows to the previous page, as JavaScript regular expressions (case doesn't matter) |
| `hints.scatter` | boolean | `true` | Spread hint labels over the alphabet so neighbours differ; false labels in order |
| `hints.selectors` | table<string, string> | `{"all":"a, area, textarea, select, input:not([type=hidden]), button, iframe, summary, [contenteditable]:not([contenteditable=false]), [onclick], [onmousedown], [role=link], [role=option], [role=button], [role=tab], [role=checkbox], [role=switch], [role=menuitem], [role=menuitemcheckbox], [role=menuitemradio], [role=treeitem], [aria-haspopup], [tabindex]:not([tabindex='-1'])","images":"img","inputs":"input:not([type]), input[type=text], input[type=search], input[type=email], input[type=url], input[type=tel], input[type=password], input[type=number], input[type=date], input[type=datetime-local], input[type=month], input[type=time], input[type=week], textarea, [contenteditable]:not([contenteditable=false])","links":"a[href], area[href], [role=link][href]","media":"audio, img, video"}` | Hint groups for :hint, as CSS selector lists; your entries are added to the built-in all, links, images, media and inputs |
| `hints.uppercase` | boolean | `false` | Show hint labels in upper case |
| `input.forward_unbound_keys` | all \| auto \| none | `auto` | Pass unbound keys to the page in normal mode (auto: all but plain letters and digits) |
| `input.insert_mode.auto_enter` | boolean | `true` | Enter insert mode when an editable element gets focus |
| `input.insert_mode.auto_leave` | boolean | `true` | Leave insert mode when focus leaves an editable element |
| `input.insert_mode.auto_load` | boolean | `false` | Enter insert mode when a page focuses a text field by itself, as autofocus does on load |
| `input.insert_mode.leave_on_load` | boolean | `true` | Leave insert mode when a new page starts loading |
| `input.match_counts` | boolean | `true` | Read digits typed before a binding as a count (3j); false lets digits be bindings themselves |
| `input.media_keys` | boolean | `true` | Let the keyboard's media keys (play, pause, next) control audio and video in pages (after a restart) |
| `input.mode_override` | none \| normal \| insert \| passthrough | `none` | Mode to enter when a page loads or its tab is focused; set it per site, e.g. passthrough for a web terminal |
| `input.mouse.rocker_gestures` | boolean | `false` | Hold the right button and click the left to go back, or the other way round to go forward; turns off the page's context menu |
| `input.partial_timeout` | integer | `0` | Milliseconds before a half-typed key chain or count is forgotten; 0 waits forever |
| `input.spatial_navigation` | boolean | `false` | Move focus between links and fields with the arrow keys, as on a TV (after a restart) |
| `keyhint.blacklist` | string[] | `[]` | Key chains the key hint popup leaves out, as globs on the whole chain (e.g. g* for every chain starting with g) |
| `keyhint.delay` | integer | `500` | How long after a partial key chain the popup listing its continuations appears, in milliseconds |
| `messages.timeout` | integer | `3000` | Milliseconds before a status bar message clears (0 keeps it) |
| `new_instance_open_target` | tab \| tab-bg \| window | `tab` | Where URLs from a second riptide invocation open |
| `new_instance_open_target_window` | first-opened \| last-opened \| last-focused | `last-focused` | Which window URLs from a second riptide invocation open in |
| `prompt.position` | bottom \| docked | `bottom` | Where questions (permissions, logins, downloads, page dialogs) appear: bottom, a box floating near the bottom of the page, or docked above the status bar |
| `prompt.width` | integer | `640` | Width in pixels of a floating prompt (prompt.position = bottom), at most the page's |
| `scrolling.bar` | always \| never \| overlay | `always` | Page scrollbars: always, never, or overlay (thin, shown while scrolling; after a restart) |
| `scrolling.smooth` | boolean | `false` | Animate scrolling by keys instead of jumping |
| `search.ignore_case` | smart \| always \| never | `smart` | Case in searches: smart ignores it unless the text has a capital, always, or never |
| `search.incremental` | boolean | `true` | Search while typing after / or ? |
| `search.wrap` | boolean | `true` | Go on from the top when a search passes the last match (or from the bottom, searching up) |
| `search.wrap_messages` | boolean | `true` | Say when a search wraps around the page |
| `session.default_name` | string | `` | Session that :session-save, :wq and auto_save.session use; empty means the last one loaded, or default |
| `session.lazy_restore` | boolean | `false` | When restoring a session, load background tabs only when they are first shown |
| `spellcheck.languages` | string[] | `[]` | Spell-check languages such as en-US (empty: off); Chromium downloads each dictionary from Google once |
| `statusbar.position` | top \| bottom | `bottom` | Where the status bar is |
| `statusbar.show` | always \| never \| in-mode | `always` | When to show the status bar: always, only while typing a command or answering a prompt (never), or also outside normal mode and while a message is shown (in-mode) |
| `statusbar.widgets` | string[] | `["keypress","downloads","muted","zoom","search_match","url","scroll","history","tabs","progress"]` | What the right side of the status bar shows, in order: keypress, downloads, muted, zoom, search_match, url, scroll, scroll_raw, history, tabs, progress, clock[:strftime format], text:… |
| `tabs.close_mouse_button` | middle \| right \| none | `middle` | Which mouse button closes a tab clicked in the tab bar |
| `tabs.close_mouse_button_on_bar` | new-tab \| close-current \| close-last \| ignore | `new-tab` | What tabs.close_mouse_button does on the empty part of the tab bar |
| `tabs.favicons.show` | always \| never \| pinned | `always` | Show site icons in the tab bar: always, never, or only on pinned tabs |
| `tabs.indicator.width` | integer | `3` | Width in pixels of the loading indicator at the left of each tab (0 hides it) |
| `tabs.last_close` | ignore \| blank \| startpage \| default-page \| close | `ignore` | What closing the last tab does |
| `tabs.max_width` | integer | `-1` | Largest width in pixels of a tab in a top or bottom tab bar (-1 for no limit) |
| `tabs.min_width` | integer | `-1` | Smallest width in pixels of a tab in a top or bottom tab bar; tabs that don't fit scroll (-1 for no minimum) |
| `tabs.mode_on_change` | normal \| persist \| restore | `normal` | Mode after switching tabs: normal, persist (keep insert/passthrough), or restore (the mode the tab was left in) |
| `tabs.mousewheel_switching` | boolean | `true` | Switch tabs with the mouse wheel over the tab bar |
| `tabs.new_position.related` | prev \| next \| first \| last | `next` | Where tabs opened from a page go (popups, hints) |
| `tabs.new_position.unrelated` | prev \| next \| first \| last | `last` | Where other new tabs go (:open -t) |
| `tabs.pinned.close` | ask \| refuse \| close | `ask` | Closing a pinned tab without --force: ask first, refuse, or just close it |
| `tabs.pinned.frozen` | boolean | `true` | Keep pinned tabs on their page: :open in a pinned tab opens a new tab |
| `tabs.pinned.shrink` | boolean | `true` | Shrink pinned tabs to their icon and number |
| `tabs.position` | top \| bottom \| left \| right | `top` | Where the tab bar is; left and right list the tabs vertically |
| `tabs.select_on_remove` | next \| prev \| last-used | `next` | Which tab to show after closing the current one: the next, the previous, or the one used before |
| `tabs.show` | always \| never \| multiple \| switching | `always` | When to show the tab bar: always, never, with more than one tab, or briefly after switching tabs |
| `tabs.show_switching_delay` | integer | `800` | How long the tab bar stays after switching tabs with tabs.show = switching, in milliseconds |
| `tabs.tabs_are_windows` | boolean | `false` | Open every tab in its own window and hide the tab bar, for tiling window managers |
| `tabs.title.alignment` | left \| center \| right | `left` | Where tab titles sit in their tab: left, center or right |
| `tabs.title.format` | string | `{audio}{index}: {current_title}` | Tab titles; fields: {index}, {aligned_index}, {current_title}, {current_url}, {host}, {perc}, {audio}, {private} |
| `tabs.title.format_pinned` | string | `{index}` | Titles of pinned tabs while tabs.pinned.shrink shrinks them; same fields as tabs.title.format |
| `tabs.tooltips` | boolean | `true` | Show a tab's title and URL when the mouse rests on it |
| `tabs.undo_stack_size` | integer | `100` | How many closed tabs u can reopen; 0 keeps none |
| `tabs.width` | integer | `200` | Width of the tab bar in pixels when tabs.position is left or right |
| `tabs.wrap` | boolean | `true` | Wrap around from the last tab to the first (and back) when switching tabs |
| `ui.theme` | riptide \| riptide-light \| gruvbox-dark \| gruvbox-light \| catppuccin-mocha \| catppuccin-latte \| nord \| dracula \| solarized-dark \| solarized-light \| tokyo-night | `riptide` | Colors of riptide's bars, prompts and hints: riptide, riptide-light, gruvbox, catppuccin, nord, dracula, solarized or tokyo-night (:theme) |
| `url.auto_search` | naive \| schemeless \| never | `naive` | When :open searches: text that doesn't look like an address (naive), anything without a scheme:// (schemeless), or never |
| `url.default_page` | string | `https://start.duckduckgo.com/` | Page for :open without a URL |
| `url.incdec_segments` | string[] | `["path","query"]` | Parts of the URL Ctrl-a and Ctrl-x change: host, port, path, query, anchor |
| `url.open_base_url` | boolean | `false` | Open a search engine's home page when :open gets just its name |
| `url.searchengines` | table<string, string> | `{"DEFAULT":"https://duckduckgo.com/?q={}"}` | Search engines; ':open g rust' uses the 'g' entry, anything else DEFAULT |
| `url.start_pages` | string[] | `["https://start.duckduckgo.com/"]` | Pages opened at startup when no URL is given |
| `url.yank_ignored_parameters` | string[] | `["ref","utm_source","utm_medium","utm_campaign","utm_term","utm_content","utm_name","fbclid","gclid"]` | Query parameters dropped when yanking a URL, such as tracking tags |
| `window.hide_decoration` | boolean | `false` | Ask the window manager for no title bar or borders (applies to new windows) |
| `window.title_format` | string | `{current_title}{title_sep}Riptide` | Window title; fields: {current_title}, {title_sep}, {current_url}, {host}, {mode} |
| `zoom.default` | integer | `100` | Zoom in percent for pages, and what :zoom without a value resets to |
| `zoom.levels` | string[] | `["25%","33%","50%","67%","75%","90%","100%","110%","125%","150%","175%","200%","250%","300%","400%","500%"]` | The zoom levels + and - step through, in percent |
