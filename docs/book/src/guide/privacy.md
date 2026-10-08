# Privacy and content blocking

## Content blocking

Ads and trackers are blocked at the network level with Adblock Plus filter lists, using Brave's [adblock-rust](https://github.com/brave/adblock-rust). Run `:adblock-update` once to download the lists in `content.blocking.adblock.lists` (by default EasyList, EasyPrivacy, and uBlock Origin's own lists: uBlock filters, Privacy, Quick fixes and Unbreak, as uBlock Origin enables them; `file://` lists work too). The compiled engine is cached in the data directory and loads in the background at startup. Hosts files (lines like `0.0.0.0 ads.example.com`, as in [StevenBlack's lists](https://github.com/StevenBlack/hosts)) work in the same setting; riptide recognizes them and blocks each listed host:

```toml
"content.blocking.adblock.lists" = [
  "https://easylist.to/easylist/easylist.txt",
  "https://raw.githubusercontent.com/StevenBlack/hosts/master/hosts",
]
```

The status bar shows how many requests were blocked on the current page, e.g. `⊘12` (the `blocked` widget in `statusbar.widgets`).

- `content.blocking.enabled` turns blocking on or off.
- `content.blocking.whitelist` lists hosts where nothing is blocked (subdomains included).
- Top-level pages are never blocked, so a bad rule can't make a site unreachable. Their tracking parameters are taken out, though (`$removeparam`): `?utm_source=mail&id=7` loads as `?id=7`.
- Element-hiding rules (`##.ad`, `example.com##.sponsored`) are applied once a page loads: the site-specific ones, and the generic ones for the classes and ids the page uses, checked again 2 and 6 seconds later for ads that arrive late.
- Scriptlet rules (`example.com##+js(set-constant, adsEnabled, false)`) run in the page before its own scripts, which is how lists defeat anti-adblock walls and in-player video ads. `$redirect` rules answer a blocked request with a harmless stand-in (an empty script, a 1×1 image), so the page carries on as if it had loaded. Both use uBlock Origin's scriptlets and stand-ins, built into riptide. As in uBlock Origin, the "trusted" scriptlets, which can click page elements or set arbitrary values, only run for rules from uBlock Origin's own lists.

If you set `content.blocking.adblock.lists` yourself before uBlock Origin's lists became defaults, add them to get most of the scriptlet rules:

```toml
"content.blocking.adblock.lists" = [
  "https://easylist.to/easylist/easylist.txt",
  "https://easylist.to/easylist/easyprivacy.txt",
  "https://ublockorigin.github.io/uAssets/filters/filters.min.txt",
  "https://ublockorigin.github.io/uAssets/filters/privacy.min.txt",
  "https://ublockorigin.github.io/uAssets/filters/quick-fixes.min.txt",
  "https://ublockorigin.github.io/uAssets/filters/unbreak.min.txt",
]
```

Run `:adblock-update` after changing the lists.

Procedural element hiding works too, for elements CSS alone can't pick out: `:has-text()`, `:upward()`, `:matches-css()` (and `-before`, `-after`), `:matches-attr()`, `:matches-path()`, `:min-text-length()` and `:xpath()`, with the actions `:remove()`, `:style()`, `:remove-attr()` and `:remove-class()`. They're applied as the page loads and again whenever it changes, so content added later is caught too.

Frames get element hiding too, by the rules for their own site: an ad in a frame from another site is hidden like one in the page. The page's site still decides whether anything is blocked, so allowing a site (`content.blocking.whitelist`) covers its frames.

Not supported yet: scriptlets in frames from another site than the page (frames from the page's own site get them).

## Network traffic

Chromium calls Google in the background. riptide turns off the calls that only serve Google and keeps the security updates (`crates/rt-cef/src/privacy.rs`). Measured on a fresh profile left on `about:blank` for 90 seconds, with `--log-net-log`:

| Request | Purpose | Status |
|---|---|---|
| `update.googleapis.com`, `edgedl.me.gvt1.com` | Component updates (all of them for one run if you turn on `content.widevine`) | Only the components Chromium marks as security data still update: certificate revocation lists (CRLSets) and the subresource filter rules. The ~20 others no longer download, saving ~115 MB per profile. These include Widevine, optimization hints, the on-device suggest model, TTS and the password-strength data. |
| `clients2.google.com/time` | Secure network time, used to explain certificate date errors | kept |
| `redirector.gvt1.com/…/dict` | Spell-check dictionary | only once per language in `spellcheck.languages` (empty by default) |
| `www.google.com/async/folae` | AI Mode eligibility | off (`--disable-features=AimEnabled`) |
| `www.google.com` preconnects | Default search engine warm-up | off (Chrome's default search engine is disabled; riptide has its own `url.searchengines`) |
| `accounts.google.com/ListAccounts` | Google accounts in the cookie jar | **still sent** once at startup. Google sign-in is off, but something still asks for the cookie jar; it carries your google.com cookies if you have any. |

The preferences are written into the profile (`Local State`, `Default/Preferences`) before Chromium starts, since most of these services start within 100 ms. To check for yourself: `riptide --basedir /tmp/t --log-net-log=/tmp/net.json about:blank`, then `grep -o '"url":"[^"]*' /tmp/net.json | sort -u`.

## Proxy and network

| Setting | What it does |
|---|---|
| `content.proxy` | `system` (default), `none`, a proxy URL (`socks5://127.0.0.1:9050`, `http://proxy:3128`), or `pac+` and a PAC script's URL. Names are looked up through a SOCKS5 proxy, not locally |
| `content.webrtc_ip_handling_policy` | Which addresses video calls may reveal: `all-interfaces` (default), `default-public-and-private-interfaces`, `default-public-interface-only`, or `disable-non-proxied-udp` to keep WebRTC behind the proxy |
| `content.dns_prefetch` | `false` stops looking up the hosts of links before you follow them |
| `content.canvas_reading` | `false` stops pages reading back what they drew, a common fingerprinting trick; some sites break (after a restart) |
| `content.cache.size` | Disk cache size in bytes; `0` lets Chromium choose (after a restart) |
| `content.local_content_can_access_file_urls` | `true` lets `file://` pages read other local files (after a restart) |
| `content.webgl` | `false` turns off WebGL, which 3D graphics need and fingerprinting scripts use (after a restart) |

```toml
content.proxy = "socks5://127.0.0.1:9050"
content.webrtc_ip_handling_policy = "disable-non-proxied-udp"
```

## Cookies, JavaScript, images and the user agent

| Setting | What it does |
|---|---|
| `content.cookies.accept` | `all` (default), `no-3rdparty` to refuse cookies from other sites embedded in a page, or `never` |
| `content.cookies.store` | `false` makes every cookie last only until the browser closes |
| `content.javascript.enabled` | `false` turns JavaScript off; set it per site to block or allow it on chosen sites only |
| `content.headers.user_agent` | The user agent sites see, in requests and in `navigator.userAgent`; empty for Chromium's own. Set it per site for sites that check it |
| `content.headers.do_not_track` | Sends `DNT: 1` (the default); `false` stops it |
| `content.headers.referer` | `same-domain` (default) sends the Referer only within a site and its subdomains; `always` or `never` |
| `content.headers.accept_language` | The languages sites are asked for, e.g. `de-DE,de;q=0.9`. Requests follow a change at once; `navigator.languages` after a restart |
| `content.headers.custom` | Extra headers for every request, e.g. `{ "X-Requested-By" = "me" }` |
| `content.images` | `false` stops loading images |
| `content.autoplay` | `false` keeps videos from playing until you interact with the page (after a restart) |
| `content.pdf_viewer` | `false` downloads PDFs instead of showing them |
| `content.prefers_reduced_motion` | `true` asks pages for fewer animations (after a restart) |
| `content.javascript.can_close_tabs` | `false` stops pages closing their own tab with `window.close()` (login popups do this) |
| `content.javascript.log_message.levels` | Page console messages to show in the status bar and `:messages`, e.g. `["error", "warning"]`; per site too |
| `content.mute` | `true` mutes pages; `:tab-mute` mutes one tab instead |
| `content.javascript.can_open_tabs_automatically` | `true` lets pages open tabs without a click (popups) |
| `content.javascript.clipboard` | `none`, `access` (copy after a click, the default) or `access-paste` (read it too) |

`content.images`, `content.mute`, popups, the clipboard, JavaScript, console messages and the user agent can also be set per site:

```toml
content.cookies.accept = "no-3rdparty"

[per_domain."*.example.org"]
"content.javascript.enabled" = false
"content.images" = false
```

## Private windows

`:open -p url` opens a private window. Private windows share an in-memory profile: no cookies or cache on disk, no history, and they're left out of sessions. Their status bar is gray.

## The sandbox

Riptide runs Chromium's sandbox wherever Linux allows it. See [Installing](installing.md#the-sandbox) for how to enable it on Ubuntu.
