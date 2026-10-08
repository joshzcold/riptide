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

Names complete with `Tab`. Remove extensions here or with `:extension-remove`, not with Remove on Chrome's extensions page: riptide loads them from their folder at every start, so Chrome's button only hides one until the next start.

To load an extension you're writing, or one you unpacked yourself, list its folder:

```toml
# config.toml
extensions.load = ["~/code/my-extension"]
```

## What works

Content scripts, background workers and blocking rules work in every tab. So do extensions' options pages, and their messages to the pages they run in. Two things are different from Chrome:

- **No toolbar buttons.** An extension's popup opens as a page (**Popup** or `:extension-open`), but extensions can't see riptide's tabs as "the current tab". Popup buttons that act on the page you're on, such as uBlock Origin Lite's per-site switch or a password manager's "fill this page", don't reach it. What an extension does inside the page, such as an autofill menu on a login field, works.
- **Private windows** don't run extensions.

For passwords, the builtin [passwords plugin](passwords.md) fills logins with a key and needs no popup.

## Password managers' desktop apps

KeePassXC-Browser and 1Password talk to their desktop app through a "native messaging host" that the app installs for other browsers. When riptide starts, it links the hosts it finds for Chromium, Chrome, Brave, Edge and Vivaldi into its data folder. It only links a host that allows one of your installed extensions. In KeePassXC, turn on browser integration for Chromium.
