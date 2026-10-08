# Passwords

The password plugins fill logins from your password manager's command line tool. Password manager [extensions](extensions.md#password-managers) can't fill logins in riptide, so this is the way to use Bitwarden, KeePassXC or pass here. They live in [riptide-plugins](https://github.com/joshzcold/riptide-plugins): add the one for your password manager in `config.lua`.

```lua
rt.pack.add({ "https://github.com/joshzcold/riptide-plugins", subdir = "pass" })
```

Or open `:plugins`, press **Browse** and **Add** the one you want. riptide installs it the first time it starts with this (it needs `git`), along with the `passwords` plugin it fills through, and asks you to allow each: the password manager plugin to run programs (your password manager), and `passwords` to fill in pages. Like other plugins from git, they stay on the version installed until you update them from `:plugins`. Add more than one, and the picker lists the logins of all of them.

| Key | Command | Fills |
|---|---|---|
| `<Space>pp` | `:password-fill` | the username and the password |
| `<Space>pu` | `:password-fill-username` | only the username |
| `<Space>pw` | `:password-fill-password` | only the password |

It finds the logins saved for the site you're on. If there's more than one, it asks which, each with a key: `1`, `2`, …. It fills the form around the focused field, or the first form with a password field, so you don't need to click into it first.

A login saved for `example.com` fills on `example.com` and its subdomains, and never on a name that only contains it, such as `example.com.evil.net`. If the tab moves to another site while your password manager answers, nothing is filled. Passwords aren't kept in the command history, `:messages` or riptide's log.

## Password managers

| Plugin (`subdir`) | Tool | Unlocking |
|---|---|---|
| `pass` | [pass](https://www.passwordstore.org/), or [gopass](https://www.gopass.pw/) with `gopass = true` | GPG's own pinentry |
| `rbw` | [rbw](https://github.com/doy/rbw), for Bitwarden | rbw's agent and pinentry |
| `bitwarden` | the [Bitwarden CLI](https://bitwarden.com/help/cli/) (`bw`) | riptide asks for the master password once and keeps the session until riptide quits or `:password-lock` |
| `keepassxc` | `keepassxc-cli`, with `database = "~/Passwords.kdbx"` | riptide asks for the database password on every fill, or keeps it for `remember = 300` seconds |

The master and database passwords go to the tool on its input or in its environment, never on its command line, where other programs could see them. For pass and gopass, use a graphical pinentry (such as `pinentry-gnome3` or `pinentry-qt`): riptide has no terminal for a text one.

### How entries are matched

- **pass and gopass:** an entry belongs to a site when part of its path is the site, such as `websites/example.com/alice`. The first line is the password. The username is a `login:`, `user:`, `username:` or `email:` line, or else the part of the path after the site (`alice`).
- **Bitwarden and KeePassXC:** an entry belongs to a site when one of its saved addresses (or its name) is the site.

## Options

Each password manager plugin takes its own options, and `passwords` takes the ones about filling; add `passwords` yourself only to pass those:

```lua
rt.pack.add({
  { "https://github.com/joshzcold/riptide-plugins", subdir = "keepassxc", opts = {
    database = "~/Passwords.kdbx",
    keyfile = "~/Passwords.key",  -- optional
    remember = 300,               -- seconds to keep the database password
    command = "/opt/bin/keepassxc-cli",  -- the tool, if it isn't on your PATH
  } },
  { "https://github.com/joshzcold/riptide-plugins", subdir = "passwords", opts = {
    submit = true,                -- press the form's submit button after :password-fill
    keys = { login = "<Ctrl-Shift-l>" },  -- your own keys; false for none
  } },
})
```

`pass` also takes `store = "~/.password-store"` (if not `$PASSWORD_STORE_DIR`) and `gopass = true`; `rbw` and `bitwarden` take `command`.
