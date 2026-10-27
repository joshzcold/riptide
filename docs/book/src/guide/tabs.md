# Tabs and windows

The tab keys are in [Keys and modes](keys.md#common-keys): `J`/`K` switch tabs, `d` closes one, `u` reopens it, `T` picks one by name.

If the page warns about unsaved changes, `d` asks first: `y` closes the tab and `n` keeps it. `:tab-close --force` closes it without asking.

## Pinned tabs

Pinned tabs stay where you put them (drag or `:tab-move` them anywhere, between unpinned tabs too), shrink to their icon and number (`tabs.pinned.shrink`), and survive `co` unless you add `--force`. `d` on a pinned tab asks first (`tabs.pinned.close`: `ask`, `refuse` or `close`); `:tab-close --force` doesn't, and `u` reopens it pinned. With `tabs.pinned.frozen` (the default), `:open` in a pinned tab opens a new tab instead. Sessions remember which tabs are pinned.

## The tab bar

The tab bar shows site icons (`tabs.favicons.show`: `always`, `never` or `pinned`) and works with the mouse: click to select, middle-click to close (`tabs.close_mouse_button`: `middle`, `right` or `none`; on the empty part of the bar it opens a new tab, see `tabs.close_mouse_button_on_bar`), scroll to switch (`tabs.mousewheel_switching`), and drag to reorder: the tab shrinks and follows the pointer, and the others move aside to show where it lands. It takes the place of the tab whose left edge (top edge in a vertical bar) its own left edge has passed.

The current tab is the darkest in the bar. To also underline it (or, in a vertical bar, mark its right edge), set `colors.tabs.selected.accent` to a CSS color, e.g. `:set colors.tabs.selected.accent #2ec4b6`; `:config-unset colors.tabs.selected.accent` removes the line again.

Tab titles follow `tabs.title.format` (default `{audio}{media}{index}: {current_title}`), and shrunk pinned tabs follow `tabs.title.format_pinned` (default `{index}`). The fields are `{index}`, `{aligned_index}`, `{current_title}`, `{current_url}`, `{host}`, `{perc}` (loading progress), `{audio}` (`[M] ` on a muted tab), `{media}` and `{private}`.

`{media}`, and the status bar's `media` widget, show what a page is capturing: `[V] ` while it uses a camera, `[A] ` for a microphone, `[A/V] ` for both, and `[Share] ` while it shares your screen, a window or a tab (see [Sharing your screen](#sharing-your-screen)). `tabs.tooltips = false` turns off the title-and-URL tooltip.

In a top or bottom bar, tabs share the width evenly. `tabs.max_width` caps each tab and `tabs.min_width` keeps them from getting narrower; once they don't fit, the bar scrolls to keep the current tab in view. `tabs.title.alignment` (`left`, `center`, `right`) places the title, and `tabs.indicator.width` sets the loading indicator's width (`0` hides it):

```toml
tabs.max_width = 250
tabs.min_width = 120
tabs.title.alignment = "center"
```

`tabs.position` puts the bar at the `top`, `bottom`, `left` or `right` (a vertical list, `tabs.width` pixels wide), and `tabs.show` hides it: `always`, `never`, `multiple` (only with more than one tab) or `switching` (briefly after switching tabs).

## New tabs and popups

Links that open new windows (`target=_blank`, `window.open`) open as tabs next to the current one, keeping `window.opener`. Closing the last tab is ignored.

After you close the current tab, `tabs.select_on_remove` picks the next one to show: `next` (default), `prev`, or `last-used`. `J`/`K` wrap around from the last tab to the first unless `tabs.wrap = false`. `u` can reopen the last `tabs.undo_stack_size` closed tabs (100 by default).

`:tab-focus last` goes back to the tab you were on before. To go further back, `:tab-focus stack-prev` steps back through the tabs you focused, and `:tab-focus stack-next` steps forward again, like back and forward in a page's history. Switching to a tab any other way starts a new path. `tabs.focus_stack_size` sets how many tabs it remembers (10 by default; -1 for no limit). Neither has a default key, so bind them, for example:

```toml
[bindings.normal]
"<Alt-[>" = "tab-focus stack-prev"
"<Alt-]>" = "tab-focus stack-next"
```

Where new tabs go is set by `tabs.new_position.related` (tabs opened from a page) and `tabs.new_position.unrelated` (everything else).

## Modes per tab

With `tabs.mode_on_change = "restore"`, each tab keeps its own mode: leave a tab while typing in insert mode, and you're back in insert mode when you return. The default `normal` leaves insert mode on every switch, and `persist` keeps the current mode.

## Splits

Splits show several panes side by side in one window, each with tabs of its own. `sv` (`:vsplit [url]`) puts a new pane on the right and `ss` (`:split [url]`) one below, with `url`, or the default page, in a new tab. The new pane takes half of the focused pane's room and gets the focus. To move the current tab into the new pane instead, add `-m`: `:vsplit -m` leaves the old pane its other tabs, or gives it a new one when it has no other (or when you give a `url`). Tab after `:vsplit ` lists `-m` and then URLs, and after `:split-focus ` or `:tab-move-pane ` the directions.

| Key | Command | Does |
|---|---|---|
| `sv` | `:vsplit [url]` | Split side by side |
| `ss` | `:split [url]` | Split one above the other |
| `<Ctrl-h>` `<Ctrl-j>` `<Ctrl-k>` `<Ctrl-l>` | `:split-focus left\|down\|up\|right` | Move to the pane on that side |
| `sw` | `:split-focus next` | Move to the next pane |
| `sm` | `:tab-move-pane next` | Move the current tab to the next pane |
| `s=` | `:split-equal` | Give every pane the same room |
| `s>` `s<` | `:split-resize width +10` / `-10` | Make the focused pane wider or narrower |
| `s+` `s-` | `:split-resize height +10` / `-10` | Make the focused pane taller or shorter |
| `sc` | `:split-close`, or `:q` | Close the focused pane |
| `so` | `:only` | Keep only the focused pane |

A thin line in the theme's accent color runs along the top of the focused pane, and keys, hints, the status bar and every command act on it. Clicking a pane focuses it, and dragging the border between two panes resizes them.

Tab commands work on the focused pane's tabs: `J`, `K`, `:tab-select`, tab numbers and `d` go through its tabs only, and new tabs, such as `:open -t` or `:open -b`, open in it. The tab bar shows the focused pane's tabs. `:tab-move-pane left|down|up|right|next` moves the current tab to another pane, which takes the focus; a pane left without tabs closes.

Closing a pane (`sc`, `:q`) moves its tabs to the pane that takes its room, and `:only` gathers every pane's tabs into the focused one, so nothing closes with a pane. Closing a pane's last tab closes the pane.

Sessions save each window's panes, their sizes and their tabs, so a restored window comes back split. With `session.lazy_restore`, the tab each pane shows loads straight away.

## Windows and private windows

`:open -w url` opens a new window and `:open -p url` a private one. Private windows use an in-memory profile shared by all private windows: no cookies or cache on disk, no history, and they're left out of sessions. Their status bar is gray. `:close` (`:q`) closes the current window, or only the focused pane when it's split, and `:quit` closes all of them. Sessions save and restore every normal window.

Windows remember their size. At startup the first window opens where the last one closed, at the same size and maximized if it was, even without `auto_save.session`. A new window from `:open -w` takes the size of the window you're in, and the window manager places it. If the window was last on a monitor that's no longer connected, it keeps its size and the window manager places it instead. Tiling window managers set the size themselves.

### Call windows

`:open --call url` opens a video call in a call window. When the call's page shares your screen, Chrome's own picker opens, so you can share a single window or the whole screen, or a tab with its sound. In an ordinary tab, screen sharing asks in the status bar and always shares the whole screen.

```
:open --call https://meet.google.com/abc-defg-hij
```

Well-known call services open in a call window by themselves: Google Meet, Microsoft Teams, Zoom's web client and join links, Webex, Jitsi Meet and Whereby. Their URL patterns are the default of `content.call_sites`; `:open` and links in ordinary tabs send matching pages to a call window, and once the page has loaded the status bar says so. `:tab-call` reopens any other tab in one.

To add a service, extend the list. To stop opening calls in their own window, clear it; screen sharing on those sites then shares the whole screen, unless you open the call with `:open --call` or `:tab-call`.

```lua
-- Setting the list replaces it, so list every site you want, e.g. your own Jitsi:
c.content.call_sites = { "meet.google.com", "teams.microsoft.com", "meet.example.org" }
-- Or opt out:
c.content.call_sites = {}
```

The settings page (`:settings`) and `:help content.call_sites` show the default list.

- **Only the first tab:** only the tab a call window opened with shares this way. Further tabs you open in that window are ordinary ones.
- **Popups:** popups from the call tab open in call windows of their own.
- **The picker's Tab list** only offers call tabs, not riptide's ordinary tabs.
- **Sessions:** a saved call window comes back as a call window, with its call tab first and its other tabs after it.
- **Turning it off:** `content.desktop_capture = false` still refuses screen sharing everywhere, call windows included.
- **In the background:** a call keeps running at full speed in a tab you've switched away from, as in Chrome, since it plays sound. A silent page in a background tab has its timers slowed to once a second.
- **Stopping a share:** see [Sharing your screen](#sharing-your-screen).

### Sharing your screen

While a page shares your screen, a window or a tab, its tab shows `[Share] ` in place of `[V] ` (`[Share/A] ` with a microphone). The status bar's `sharing` widget says what's being shared, such as `[sharing your screen]`, whichever tab you're on. When a share starts, the status bar names the site.

`:share-stop` stops every share, in any tab or window. The page is told as if you'd pressed Chrome's "Stop sharing", so the call site updates its own buttons. You can still stop from the site or from Chrome's bar in a call window.

`:share-switch` shares something else instead, without stopping: riptide goes to the call, Chrome's picker opens again, and what you pick goes out in the same call, so the others never see the share end. It works in call windows, the only ones with a picker; `:tab-call` moves a call into one. Cancelling the picker keeps the current share.

### Picture-in-picture

`gp` (`:pip`) floats the page's main video in a small window that stays on top while you work in other tabs, such as a call or a talk; `gp` again brings it back. Sites' own picture-in-picture buttons work too, including Meet's floating call window.

Some tiling window managers don't float it by themselves; it's titled "Picture in picture". In dwm, a rule floats it:

```c
{ NULL, NULL, "Picture in picture", 0, 1, 0, 0, -1 },
```

### Muting a call from another tab

`cm` (`:call-mute`) mutes or unmutes your microphone in the call while you're in another tab, or in another window such as when the call has its own call window. It presses the call site's own mute key in the tab that's using the microphone, so the site's mute button stays right, and you stay where you were. `content.call_mute_keys` has each site's key (Meet `<Ctrl-d>`, Teams `<Ctrl-Shift-m>`, Zoom `<Alt-a>`, Webex `<Ctrl-m>`, Jitsi `m`); add others the same way.

- **A flash of the call:** Chromium only takes keys in the tab that's showing, so the call tab shows for a moment and then the tab you were on comes back. The first time, it shows for under a second, while the page gets ready for keys.
- **Another window:** riptide lends the call's window the keyboard for the key press, without raising it, then gives it back to yours. The message says "(another window)".

### Choosing the camera, microphone and speaker

`:media-devices` picks the camera, microphone and speaker the current site gets: pick which one, then the device. A call in progress switches at once, without rejoining, and the site's own mute button keeps working. The choice is kept for the site, so its next call starts on it, and the permission question names it ("wants to use your microphone (USB Mic)").

Device names show once the site may use your camera or microphone, so pick them after the first call starts. "The site's own choice" hands it back to the site's own settings from its next call. The choices are the per-site settings `content.media.camera`, `content.media.microphone` and `content.media.speaker`, which hold the device's name; set one globally for every site:

```lua
c.content.media.microphone = "USB Mic"
```

Your choice wins over the device the site's own settings pick. A site that copies the camera into a stream of its own may still show the old camera in its preview while sending the new one.

`tabs.tabs_are_windows = true` opens every tab, and every popup, in its own window and hides the tab bar, which suits tiling window managers that arrange windows themselves.

`window.hide_decoration = true` asks the window manager for windows without a title bar or borders, which suits tiling window managers. It applies to windows opened after the change.

`:tab-give` and `:tab-take` move tabs between windows, and `:tab-clone` duplicates one.
