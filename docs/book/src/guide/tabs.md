# Tabs and windows

The tab keys are in [Keys and modes](keys.md#common-keys): `J`/`K` switch tabs, `d` closes one, `u` reopens it, `T` picks one by name.

## Pinned tabs

Pinned tabs stay at the left, shrink to their icon and number (`tabs.pinned.shrink`), and survive `d` and `co` unless you add `--force`. With `tabs.pinned.frozen` (the default), `:open` in a pinned tab opens a new tab instead. Sessions remember which tabs are pinned.

## The tab bar

The tab bar shows site icons (`tabs.favicons.show`: `always`, `never` or `pinned`) and works with the mouse: click to select, middle-click to close, scroll to switch (`tabs.mousewheel_switching`), and drag to reorder.

`tabs.position` puts the bar at the `top`, `bottom`, `left` or `right` (a vertical list, `tabs.width` pixels wide), and `tabs.show` hides it: `always`, `never`, `multiple` (only with more than one tab) or `switching` (briefly after switching tabs).

## New tabs and popups

Links that open new windows (`target=_blank`, `window.open`) open as tabs next to the current one, keeping `window.opener`. Closing the last tab is ignored, like qutebrowser.

Where new tabs go is set by `tabs.new_position.related` (tabs opened from a page) and `tabs.new_position.unrelated` (everything else).

## Modes per tab

With `tabs.mode_on_change = "restore"`, each tab keeps its own mode: leave a tab while typing in insert mode, and you're back in insert mode when you return. The default `normal` leaves insert mode on every switch, and `persist` keeps the current mode.

## Windows and private windows

`:open -w url` opens a new window and `:open -p url` a private one. Private windows use an in-memory profile shared by all private windows: no cookies or cache on disk, no history, and they're left out of sessions. Their status bar is gray. `:close` closes the current window and `:quit` closes all of them. Sessions save and restore every normal window.

`:tab-give` and `:tab-take` move tabs between windows, and `:tab-clone` duplicates one.
