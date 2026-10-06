# Tabs and windows

The tab keys are in [Keys and modes](keys.md#common-keys): `J`/`K` switch tabs, `d` closes one, `u` reopens it, `T` picks one by name.

## Pinned tabs

Pinned tabs stay where you put them (drag or `:tab-move` them anywhere, between unpinned tabs too), shrink to their icon and number (`tabs.pinned.shrink`), and survive `co` unless you add `--force`. `d` on a pinned tab asks first (`tabs.pinned.close`: `ask`, `refuse` or `close`); `:tab-close --force` doesn't, and `u` reopens it pinned. With `tabs.pinned.frozen` (the default), `:open` in a pinned tab opens a new tab instead. Sessions remember which tabs are pinned.

## The tab bar

The tab bar shows site icons (`tabs.favicons.show`: `always`, `never` or `pinned`) and works with the mouse: click to select, middle-click to close (`tabs.close_mouse_button`: `middle`, `right` or `none`), scroll to switch (`tabs.mousewheel_switching`), and drag to reorder: the tab shrinks and follows the pointer, and the others move aside to show where it lands.

The current tab is the darkest in the bar. To also underline it (or, in a vertical bar, mark its right edge), set `colors.tabs.selected.accent` to a CSS color, e.g. `:set colors.tabs.selected.accent #2ec4b6`; `:config-unset colors.tabs.selected.accent` removes the line again.

Tab titles follow `tabs.title.format` (default `{audio}{index}: {current_title}`), and shrunk pinned tabs follow `tabs.title.format_pinned` (default `{index}`). The fields are `{index}`, `{aligned_index}`, `{current_title}`, `{current_url}`, `{host}`, `{perc}` (loading progress), `{audio}` (`[M] ` on a muted tab) and `{private}`. `tabs.tooltips = false` turns off the title-and-URL tooltip.

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

`window.hide_decoration = true` asks the window manager for windows without a title bar or borders, which suits tiling window managers. It applies to windows opened after the change.

`:tab-give` and `:tab-take` move tabs between windows, and `:tab-clone` duplicates one.
