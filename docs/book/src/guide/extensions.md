# Extensions

riptide runs Chrome extensions that use Manifest V3, such as uBlock Origin Lite, Bitwarden or KeePassXC-Browser. Firefox add-ons don't run (riptide is Chromium), and neither do Manifest V2 extensions such as the original uBlock Origin, which Chromium no longer supports.

## Installing

```
:extension-install https://chromewebstore.google.com/detail/ublock-origin-lite/ddkjiahejlhfcafbddmgiahcphecmpfh
```

`:extension-install` takes an extension's Web Store page, its id, or a `.crx` file (`/path/to/file.crx`). It downloads the extension and shows what it asks for, such as "read and change everything on every site you visit", then asks before installing. Extensions load when riptide starts, so `:restart` to use it.

Installed extensions keep their Web Store id, so their settings carry over between versions, and desktop apps that talk to them recognise them. They're in `extensions/` in the data folder (`riptide --paths`). To update one, install it again.

| Command | |
|---|---|
| `:extension-install <page, id or file>` | install, after showing what it asks for |
| `:extension-remove <name or id>` | delete it; it's gone after `:restart` |
| `:extensions` | Chrome's extensions page: turn extensions on or off, see their errors, open their options |

To load an extension you're writing, or one you unpacked yourself, list its folder:

```toml
# config.toml
extensions.load = ["~/code/my-extension"]
```

## What works

Content scripts, background workers and blocking rules work in every tab. So do extensions' options pages, and their messages to the pages they run in. Two things are different from Chrome:

- **No toolbar buttons.** An extension's popup can be opened as a page (`chrome-extension://<id>/popup.html`), but extensions can't see riptide's tabs as "the current tab". Popup buttons that act on the page you're on, such as uBlock Origin Lite's per-site switch or a password manager's "fill this page", don't reach it. What an extension does inside the page, such as an autofill menu on a login field, works.
- **Private windows** don't run extensions.

For passwords, the builtin [passwords plugin](passwords.md) fills logins with a key and needs no popup.

## Password managers' desktop apps

KeePassXC-Browser and 1Password talk to their desktop app through a "native messaging host" that the app installs for other browsers. When riptide starts, it links the hosts it finds for Chromium, Chrome, Brave, Edge and Vivaldi into its data folder. It only links a host that allows one of your installed extensions. In KeePassXC, turn on browser integration for Chromium.
