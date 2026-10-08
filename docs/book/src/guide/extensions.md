# Extensions

riptide runs Chrome extensions that use Manifest V3, such as uBlock Origin Lite, Bitwarden or KeePassXC-Browser. Firefox add-ons don't run (riptide is Chromium), and neither do Manifest V2 extensions such as the original uBlock Origin, which Chromium no longer supports.

## Installing

1. Find the extension on the [Chrome Web Store](https://chromewebstore.google.com/category/extensions). Ignore its "Switch to Chrome" banner; riptide shows a message saying how to install instead.
2. On the extension's page, run `:extension-install`. If the store offers a download (**Add to Chrome**), that works too: riptide installs the `.crx` file instead of asking where to save it.
3. riptide shows what the extension asks for, such as "read and change everything on every site you visit", and asks before installing. Extensions load when riptide starts, so it then offers to restart.

`:extension-install` also takes a store page's address or an extension's id from anywhere, or a `.crx` file (`/path/to/file.crx`). The Extensions tab (`:extensions`) has the same steps, and a box to paste a store page into.

```
:extension-install https://chromewebstore.google.com/detail/ublock-origin-lite/ddkjiahejlhfcafbddmgiahcphecmpfh
```

Installed extensions keep their Web Store id, so their settings carry over between versions, and desktop apps that talk to them recognise them. They're in `extensions/` in the data folder (`riptide --paths`).

## Managing them

`:extensions` opens the Extensions tab of `:settings`. It lists every extension with its version, where it's from and what it can do, and these buttons:

- **Popup** opens the extension's popup over the top right of the page, as Chrome does. Escape or its ✕ closes it, and so does switching tabs. **Options** opens its options page in a tab.
- **Remove** deletes an installed one.
- **Check for updates** asks the Web Store for newer versions. **Update to …** then shows what the new version asks for that the old one didn't, before installing it.
- **Chrome's extensions page** turns extensions on or off, and shows their errors.

When something changed since riptide started, the tab says so, with a **Restart now** button.

| Command | |
|---|---|
| `:extension-install [page, id or file]` | install or update (the store page you're on, without an argument), after showing what it asks for |
| `:extension-open <name> [popup\|options]` | open its popup over the page (or, without one, its options in a tab) |
| `:extension-update [name]` | check every installed extension for updates, or update one |
| `:extension-remove <name or id>` | delete an installed extension |
| `:extensions` | the Extensions tab |

Names complete with `Tab`. See [Limits](#limits) for what works differently from Chrome.

To load an extension you're writing, or one you unpacked yourself, list its folder:

```toml
# config.toml
extensions.load = ["~/code/my-extension"]
```

## What works

Blocking, and changes an extension makes to every page (such as dark mode or hiding page elements), work in every tab. So do extensions' options pages and settings.

What doesn't work is anything that needs the extension to know which site you're on. riptide's tabs aren't Chrome's, so to an extension you're never "on" a site. See the first row of [Limits](#limits), and [Password managers](#password-managers).

## Limits

| Limit | What to do instead |
|---|---|
| **Extensions can't tell which site you're on.** Anything that depends on it doesn't work: uBlock Origin Lite's per-site switch, filtering level and element picker (its popup says "not a website"), and password managers' login suggestions. | Settings that don't depend on the site, such as filter lists and the default blocking level, work from the popup or options. For logins, see [Password managers](#password-managers). |
| **Only Manifest V3.** Chromium no longer runs Manifest V2 extensions, such as the original uBlock Origin, and riptide refuses to install them. Firefox add-ons don't run either. | uBlock Origin Lite, or riptide's own ad blocker (`content.blocking`). |
| **Changes need a restart.** Extensions load when riptide starts. | Accept the "Restart now?" question, or use **Restart now** on the Extensions tab. |
| **No automatic updates.** | Press **Check for updates** on the Extensions tab, or run `:extension-update`. |
| **Not in private windows.** | Use a normal window. |
| **Chrome's own Remove only hides an extension until the next start**, because riptide loads it from its folder each time. | **Remove** on the Extensions tab, or `:extension-remove`. |
| **Desktop apps are linked at startup.** riptide only finds a password manager's desktop app if the app set itself up for another Chromium browser first. | Turn on the app's browser integration for Chromium or Chrome, then restart riptide. |
| **Not tested yet:** extensions' keyboard shortcuts and passkeys. | Please report what you find. |

### What an install checks

riptide downloads extensions over HTTPS from Google, and checks that the file's key gives the extension id you asked for. It doesn't check the Web Store's signature on the file. A `.crx` file you install from disk has no id to check, so only install files you trust. Either way, you see what the extension asks for before anything is installed.

## Password managers

Password manager extensions install, unlock and sync, but they can't fill logins. To suggest a login, a password manager asks which site you're on, and in riptide the answer is always "none". So its menu in a login field says "No items to show", and its "fill this page" button does nothing. Your vault is fine; the extension just never learns which site's logins to offer.

To fill logins, use the [passwords plugin](passwords.md) instead. It reads the same vault through your password manager's command line tool, finds the logins for the site you're on, and fills them with `<Space>pp`:

| Password manager | Use the passwords plugin with |
|---|---|
| Bitwarden | `rbw` or the Bitwarden CLI (`bw`) |
| KeePassXC | `keepassxc-cli` |
| pass, gopass | `pass` or `gopass` |

1Password and Proton Pass have no backend in the passwords plugin yet.

### Desktop apps

KeePassXC-Browser and 1Password talk to their desktop app through a "native messaging host" that the app installs for other browsers. When riptide starts, it links the hosts it finds for Chromium, Chrome, Brave, Edge and Vivaldi into its data folder. It only links a host that allows one of your installed extensions. In KeePassXC, turn on browser integration for Chromium.
