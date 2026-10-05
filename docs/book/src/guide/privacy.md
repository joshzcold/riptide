# Privacy and content blocking

## Content blocking

Ads and trackers are blocked at the network level with Adblock Plus filter lists, using Brave's [adblock-rust](https://github.com/brave/adblock-rust). Run `:adblock-update` once to download the lists in `content.blocking.adblock.lists` (EasyList and EasyPrivacy by default; `file://` lists work too). The compiled engine is cached in the data directory and loads in the background at startup.

- `content.blocking.enabled` turns blocking on or off.
- `content.blocking.whitelist` lists hosts where nothing is blocked (subdomains included).
- Top-level pages are never blocked, so a bad rule can't make a site unreachable.
- Element-hiding rules (`##.ad`, `example.com##.sponsored`) are applied once a page loads: the site-specific ones, and the generic ones for the classes and ids the page uses, checked again 2 and 6 seconds later for ads that arrive late.

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

## Cookies, JavaScript, images and the user agent

| Setting | What it does |
|---|---|
| `content.cookies.accept` | `all` (default), `no-3rdparty` to refuse cookies from other sites embedded in a page, or `never` |
| `content.cookies.store` | `false` makes every cookie last only until the browser closes |
| `content.javascript.enabled` | `false` turns JavaScript off; set it per site to block or allow it on chosen sites only |
| `content.headers.user_agent` | The user agent sites see, in requests and in `navigator.userAgent`; empty for Chromium's own. Set it per site for sites that check it |
| `content.images` | `false` stops loading images |
| `content.mute` | `true` mutes pages; `:tab-mute` mutes one tab instead |
| `content.javascript.can_open_tabs_automatically` | `true` lets pages open tabs without a click (popups) |
| `content.javascript.clipboard` | `none`, `access` (copy after a click, the default) or `access-paste` (read it too) |
| `content.register_protocol_handler` | `ask` (default), `true` or `false` for sites that offer to handle links like `mailto:` |

All but the cookie settings can also be set per site:

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
