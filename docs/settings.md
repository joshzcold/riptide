# Settings

<!-- Generated from crates/rt-core/src/settings.rs; regenerate with UPDATE_LUA_TYPES=1 cargo test -p rt-config. -->

Set these in `config.toml` (`hints.chars = "asdf"`), `config.lua` (`c.hints.chars = "asdf"`) or with `:set hints.chars asdf`.

| Setting | Type | Default | Description |
|---|---|---|---|
| `aliases` | table<string, string> | `{"q":"quit","qa":"quit","wq":"quit --save"}` | Command aliases: name → command |
| `auto_save.interval` | integer | `15000` | Milliseconds between crash-recovery saves of the open tabs (0 turns them off) |
| `auto_save.session` | boolean | `false` | Save the open tabs as the 'default' session on quit, and restore them at startup |
| `colors.webpage.darkmode.enabled` | boolean | `false` | Render light pages dark with Chromium's automatic dark mode (takes effect after a restart) |
| `colors.webpage.preferred_color_scheme` | auto \| light \| dark | `auto` | The color scheme pages see in prefers-color-scheme: auto follows the system |
| `completion.web_history.max_items` | integer | `100` | How many history entries :open completion shows (0 turns history completion off) |
| `content.blocking.adblock.lists` | string[] | `["https://easylist.to/easylist/easylist.txt","https://easylist.to/easylist/easyprivacy.txt"]` | Adblock Plus filter lists that :adblock-update downloads (https://, or file:// for local lists) |
| `content.blocking.enabled` | boolean | `true` | Block ads and trackers with the filter lists from content.blocking.adblock.lists |
| `content.blocking.whitelist` | string[] | `[]` | Sites where nothing is blocked, as host names; a host also covers its subdomains |
| `content.desktop_capture` | ask \| true \| false | `ask` | Let sites capture your screen or desktop audio: ask, true or false |
| `content.geolocation` | ask \| true \| false | `ask` | Let sites know your location: ask, true or false |
| `content.media.audio_capture` | ask \| true \| false | `ask` | Let sites use your microphone: ask, true or false |
| `content.media.video_capture` | ask \| true \| false | `ask` | Let sites use your camera: ask, true or false |
| `content.notifications.enabled` | ask \| true \| false | `ask` | Let sites show notifications: ask, true or false |
| `content.tls.certificate_errors` | ask \| block \| load-insecurely | `ask` | Pages whose TLS certificate isn't trusted: ask, block, or load-insecurely |
| `content.widevine` | boolean | `false` | Allow Widevine DRM: Chromium downloads Google's CDM once (takes effect after a restart) |
| `downloads.location.directory` | string | `` | Where downloads go; empty means the system Downloads folder |
| `downloads.location.prompt` | boolean | `true` | Ask where to save each download (false saves straight to the directory) |
| `editor.command` | string[] | `["gvim","-f","{file}","-c","normal {line}G{column0}l"]` | Editor for :open-editor; fields: {file}, {line}, {column}, {line0}, {column0} |
| `hints.chars` | string | `asdfghjkl` | Characters used for hint labels |
| `hints.mode` | letter \| number | `letter` | letter: labels from hints.chars; number: numbered labels, and typing letters filters by text |
| `hints.uppercase` | boolean | `false` | Show hint labels in upper case |
| `input.forward_unbound_keys` | all \| auto \| none | `auto` | Pass unbound keys to the page in normal mode (auto: all but plain letters and digits) |
| `input.insert_mode.auto_enter` | boolean | `true` | Enter insert mode when an editable element gets focus |
| `input.insert_mode.auto_leave` | boolean | `true` | Leave insert mode when focus leaves an editable element |
| `input.insert_mode.leave_on_load` | boolean | `true` | Leave insert mode when a new page starts loading |
| `keyhint.blacklist` | string[] | `[]` | Key chains the key hint popup leaves out, as globs on the whole chain (e.g. g* for every chain starting with g) |
| `keyhint.delay` | integer | `500` | How long after a partial key chain the popup listing its continuations appears, in milliseconds |
| `messages.timeout` | integer | `3000` | Milliseconds before a status bar message clears (0 keeps it) |
| `new_instance_open_target` | tab \| tab-bg \| window | `tab` | Where URLs from a second riptide invocation open |
| `search.ignore_case` | smart \| always \| never | `smart` | Case in searches: smart ignores it unless the text has a capital, always, or never |
| `search.incremental` | boolean | `true` | Search while typing after / or ? |
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
| `tabs.show` | always \| never \| multiple \| switching | `always` | When to show the tab bar: always, never, with more than one tab, or briefly after switching tabs |
| `tabs.show_switching_delay` | integer | `800` | How long the tab bar stays after switching tabs with tabs.show = switching, in milliseconds |
| `tabs.width` | integer | `200` | Width of the tab bar in pixels when tabs.position is left or right |
| `url.default_page` | string | `https://start.duckduckgo.com/` | Page for :open without a URL |
| `url.searchengines` | table<string, string> | `{"DEFAULT":"https://duckduckgo.com/?q={}"}` | Search engines; ':open g rust' uses the 'g' entry, anything else DEFAULT |
| `url.start_pages` | string[] | `["https://start.duckduckgo.com/"]` | Pages opened at startup when no URL is given |
| `url.yank_ignored_parameters` | string[] | `["ref","utm_source","utm_medium","utm_campaign","utm_term","utm_content","utm_name","fbclid","gclid"]` | Query parameters dropped when yanking a URL, such as tracking tags |
| `window.title_format` | string | `{current_title}{title_sep}Riptide` | Window title; fields: {current_title}, {title_sep}, {current_url}, {host}, {mode} |
| `zoom.default` | integer | `100` | Zoom in percent for pages, and what :zoom without a value resets to |
