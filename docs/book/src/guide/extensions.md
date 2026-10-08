# Extensions

riptide runs Chrome extensions that use Manifest V3, such as uBlock Origin Lite, Bitwarden or KeePassXC-Browser. Firefox add-ons don't run (riptide is Chromium), and neither do Manifest V2 extensions such as the original uBlock Origin, which Chromium no longer supports.

## Installing

```
:extension-install https://chromewebstore.google.com/detail/ublock-origin-lite/ddkjiahejlhfcafbddmgiahcphecmpfh
```

`:extension-install` takes an extension's Web Store page, its id, or a `.crx` file (`/path/to/file.crx`). It downloads the extension and shows what it asks for, such as "read and change everything on every site you visit", then asks before installing. Extensions load when riptide starts, so it then offers to restart.

Installed extensions keep their Web Store id, so their settings carry over between versions, and desktop apps that talk to them recognise them. They're in `extensions/` in the data folder (`riptide --paths`).

## Managing them

`:extensions` opens the Extensions tab of `:settings`. It lists every extension with its version, where it's from and what it can do, and these buttons:

- **Popup** and **Options** open the extension's popup or options page in a tab.
- **Remove** deletes an installed one.
- **Check for updates** asks the Web Store for newer versions. **Update to …** then shows what the new version asks for that the old one didn't, before installing it.
- **Chrome's extensions page** turns extensions on or off, and shows their errors.

When something changed since riptide started, the tab says so, with a **Restart now** button.

| Command | |
|---|---|
| `:extension-install <page, id or file>` | install or update, after showing what it asks for |
| `:extension-open <name> [popup\|options]` | open its popup (or else its options) in a tab |
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

Content scripts, background workers and blocking rules work in every tab. So do extensions' options pages, their messages to the pages they run in, and what they show inside a page, such as an autofill menu on a login field.

## Limits

| Limit | What to do instead |
|---|---|
| **Popups can't act on the page you're on.** A popup opens as a tab (**Popup** or `:extension-open`), and extensions can't see riptide's tabs as "the current tab". Buttons such as uBlock Origin Lite's per-site switch or a password manager's "fill this page" don't reach the page. | For logins, the builtin [passwords plugin](passwords.md) fills them with a key. Settings that don't depend on the current page, such as filter lists, work from the popup or options. |
| **Only Manifest V3.** Chromium no longer runs Manifest V2 extensions, such as the original uBlock Origin, and riptide refuses to install them. Firefox add-ons don't run either. | uBlock Origin Lite, or riptide's own ad blocker (`content.blocking`). |
| **Changes need a restart.** Extensions load when riptide starts. | Accept the "Restart now?" question, or use **Restart now** on the Extensions tab. |
| **No automatic updates.** | Press **Check for updates** on the Extensions tab, or run `:extension-update`. |
| **Not in private windows.** | Use a normal window. |
| **Chrome's own Remove only hides an extension until the next start**, because riptide loads it from its folder each time. | **Remove** on the Extensions tab, or `:extension-remove`. |
| **Desktop apps are linked at startup.** riptide only finds a password manager's desktop app if the app set itself up for another Chromium browser first. | Turn on the app's browser integration for Chromium or Chrome, then restart riptide. |
| **Not tested yet:** extensions' keyboard shortcuts, passkeys, and autofill against a real vault. | Please report what you find. |

### What an install checks

riptide downloads extensions over HTTPS from Google, and checks that the file's key gives the extension id you asked for. It doesn't check the Web Store's signature on the file. A `.crx` file you install from disk has no id to check, so only install files you trust. Either way, you see what the extension asks for before anything is installed.

## Password managers' desktop apps

KeePassXC-Browser and 1Password talk to their desktop app through a "native messaging host" that the app installs for other browsers. When riptide starts, it links the hosts it finds for Chromium, Chrome, Brave, Edge and Vivaldi into its data folder. It only links a host that allows one of your installed extensions. In KeePassXC, turn on browser integration for Chromium.
