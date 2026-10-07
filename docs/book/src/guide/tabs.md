# Tabs and windows

The tab keys are in [Keys and modes](keys.md#common-keys): `J`/`K` switch tabs, `d` closes one, `u` reopens it, `T` picks one by name.

## Pinned tabs

Pinned tabs stay where you put them (drag or `:tab-move` them anywhere, between unpinned tabs too), shrink to their icon and number (`tabs.pinned.shrink`), and survive `co` unless you add `--force`. `d` on a pinned tab asks first (`tabs.pinned.close`: `ask`, `refuse` or `close`); `:tab-close --force` doesn't, and `u` reopens it pinned. With `tabs.pinned.frozen` (the default), `:open` in a pinned tab opens a new tab instead. Sessions remember which tabs are pinned.

## The tab bar

The tab bar shows site icons (`tabs.favicons.show`: `always`, `never` or `pinned`) and works with the mouse: click to select, middle-click to close (`tabs.close_mouse_button`: `middle`, `right` or `none`; on the empty part of the bar it opens a new tab, see `tabs.close_mouse_button_on_bar`), scroll to switch (`tabs.mousewheel_switching`), and drag to reorder: the tab shrinks and follows the pointer, and the others move aside to show where it lands.

The current tab is the darkest in the bar. To also underline it (or, in a vertical bar, mark its right edge), set `colors.tabs.selected.accent` to a CSS color, e.g. `:set colors.tabs.selected.accent #2ec4b6`; `:config-unset colors.tabs.selected.accent` removes the line again.

Tab titles follow `tabs.title.format` (default `{audio}{media}{index}: {current_title}`), and shrunk pinned tabs follow `tabs.title.format_pinned` (default `{index}`). The fields are `{index}`, `{aligned_index}`, `{current_title}`, `{current_url}`, `{host}`, `{perc}` (loading progress), `{audio}` (`[M] ` on a muted tab), `{media}` and `{private}`.

`{media}`, and the status bar's `media` widget, show what a page is capturing: `[V] ` while it uses a camera or shares the screen, `[A] ` for a microphone, and `[A/V] ` for both. Chromium doesn't say whether video comes from a camera or the screen. `tabs.tooltips = false` turns off the title-and-URL tooltip.

In a top or bottom bar, tabs share the width evenly. `tabs.max_width` caps each tab and `tabs.min_width` keeps them from getting narrower; once they don't fit, the bar scrolls to keep the current tab in view. `tabs.title.alignment` (`left`, `center`, `right`) places the title, and `tabs.indicator.width` sets the loading indicator's width (`0` hides it):

```toml
tabs.max_width = 250
tabs.min_width = 120
tabs.title.alignment = "center"
```

`tabs.position` puts the bar at the `top`, `bottom`, `left` or `right` (a vertical list, `tabs.width` pixels wide), and `tabs.show` hides it: `always`, `never`, `multiple` (only with more than one tab) or `switching` (briefly after switching tabs).

## New tabs and popups

Links that open new windows (`target=_blank`, `window.open`) open as tabs next to the current one, keeping `window.opener`. Closing the last tab is ignored, like qutebrowser.

After you close the current tab, `tabs.select_on_remove` picks the next one to show: `next` (default), `prev`, or `last-used`. `J`/`K` wrap around from the last tab to the first unless `tabs.wrap = false`. `u` can reopen the last `tabs.undo_stack_size` closed tabs (100 by default).

Where new tabs go is set by `tabs.new_position.related` (tabs opened from a page) and `tabs.new_position.unrelated` (everything else).

## Modes per tab

With `tabs.mode_on_change = "restore"`, each tab keeps its own mode: leave a tab while typing in insert mode, and you're back in insert mode when you return. The default `normal` leaves insert mode on every switch, and `persist` keeps the current mode.

## Windows and private windows

`:open -w url` opens a new window and `:open -p url` a private one. Private windows use an in-memory profile shared by all private windows: no cookies or cache on disk, no history, and they're left out of sessions. Their status bar is gray. `:close` closes the current window and `:quit` closes all of them. Sessions save and restore every normal window.

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
- **Sessions:** call windows are saved as ordinary windows, so a restored call shares the whole screen until you reopen it with `--call`.
- **Turning it off:** `content.desktop_capture = false` still refuses screen sharing everywhere, call windows included.
- **In the background:** a call keeps running at full speed in a tab you've switched away from, as in Chrome, since it plays sound. A silent page in a background tab has its timers slowed to once a second.
- **Stopping a share:** in a call window, use the "Stop sharing" bar Chrome shows. riptide can't stop a page's capture from outside; close or reload the tab instead.

### Muting a call from another tab

`cm` (`:call-mute`) mutes or unmutes your microphone in the call while you're in another tab. It presses the call site's own mute key in the tab that's using the microphone, so the site's mute button stays right. `content.call_mute_keys` has each site's key (Meet `<Ctrl-d>`, Teams `<Ctrl-Shift-m>`, Zoom `<Alt-a>`, Webex `<Ctrl-m>`, Jitsi `m`); add others the same way.

- **A flash of the call:** Chromium only takes keys in the tab that's showing, so the call tab shows for a moment and then the tab you were on comes back. The first time, it shows for under a second, while the page gets ready for keys.
- **Another window:** a call in another window isn't reached; riptide names the key to press there.

`tabs.tabs_are_windows = true` opens every tab, and every popup, in its own window and hides the tab bar, which suits tiling window managers that arrange windows themselves.

`window.hide_decoration = true` asks the window manager for windows without a title bar or borders, which suits tiling window managers. It applies to windows opened after the change.

`:tab-give` and `:tab-take` move tabs between windows, and `:tab-clone` duplicates one.
