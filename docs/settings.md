# Settings

<!-- Generated from crates/hb-core/src/settings.rs; regenerate with UPDATE_LUA_TYPES=1 cargo test -p hb-config. -->

Set these in `config.toml` (`hints.chars = "asdf"`), `config.lua` (`c.hints.chars = "asdf"`) or with `:set hints.chars asdf`.

| Setting | Type | Default | Description |
|---|---|---|---|
| `aliases` | table<string, string> | `{"q":"quit","qa":"quit","wq":"quit --save"}` | Command aliases: name → command |
| `auto_save.session` | boolean | `false` | Save the open tabs as the 'default' session on quit, and restore them at startup |
| `completion.web_history.max_items` | integer | `100` | How many history entries :open completion shows (0 turns history completion off) |
| `content.desktop_capture` | ask \| true \| false | `ask` | Let sites capture your screen or desktop audio: ask, true or false |
| `content.geolocation` | ask \| true \| false | `ask` | Let sites know your location: ask, true or false |
| `content.media.audio_capture` | ask \| true \| false | `ask` | Let sites use your microphone: ask, true or false |
| `content.media.video_capture` | ask \| true \| false | `ask` | Let sites use your camera: ask, true or false |
| `content.notifications.enabled` | ask \| true \| false | `ask` | Let sites show notifications: ask, true or false |
| `downloads.location.directory` | string | `` | Where downloads go; empty means the system Downloads folder |
| `downloads.location.prompt` | boolean | `true` | Ask where to save each download (false saves straight to the directory) |
| `hints.chars` | string | `asdfghjkl` | Characters used for hint labels |
| `hints.uppercase` | boolean | `false` | Show hint labels in upper case |
| `input.forward_unbound_keys` | all \| auto \| none | `auto` | Pass unbound keys to the page in normal mode (auto: all but plain letters and digits) |
| `input.insert_mode.auto_enter` | boolean | `true` | Enter insert mode when an editable element gets focus |
| `input.insert_mode.auto_leave` | boolean | `true` | Leave insert mode when focus leaves an editable element |
| `input.insert_mode.leave_on_load` | boolean | `true` | Leave insert mode when a new page starts loading |
| `messages.timeout` | integer | `3000` | Milliseconds before a status bar message clears (0 keeps it) |
| `tabs.last_close` | ignore \| blank \| startpage \| default-page \| close | `ignore` | What closing the last tab does |
| `tabs.mode_on_change` | normal \| persist | `normal` | Mode after switching tabs: back to normal, or keep insert/passthrough |
| `tabs.new_position.related` | prev \| next \| first \| last | `next` | Where tabs opened from a page go (popups, hints) |
| `tabs.new_position.unrelated` | prev \| next \| first \| last | `last` | Where other new tabs go (:open -t) |
| `url.default_page` | string | `https://start.duckduckgo.com/` | Page for :open without a URL |
| `url.searchengines` | table<string, string> | `{"DEFAULT":"https://duckduckgo.com/?q={}"}` | Search engines; ':open g rust' uses the 'g' entry, anything else DEFAULT |
| `url.start_pages` | string[] | `["https://start.duckduckgo.com/"]` | Pages opened at startup when no URL is given |
