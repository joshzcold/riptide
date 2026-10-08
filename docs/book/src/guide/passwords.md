# Passwords

The `passwords` plugin ships with riptide and fills logins from your password manager's command line tool. Turn it on in `config.lua`:

```lua
rt.pack.add({ builtin = "passwords", opts = { backend = "pass" } })
```

The first time it loads, riptide asks you to allow it to run programs (your password manager) and to fill in pages.

| Key | Command | Fills |
|---|---|---|
| `<Space>pp` | `:password-fill` | the username and the password |
| `<Space>pu` | `:password-fill-username` | only the username |
| `<Space>pw` | `:password-fill-password` | only the password |

It finds the logins saved for the site you're on. If there's more than one, it asks which, each with a key: `1`, `2`, …. It fills the form around the focused field, or the first form with a password field, so you don't need to click into it first.

A login saved for `example.com` fills on `example.com` and its subdomains, and never on a name that only contains it, such as `example.com.evil.net`. If the tab moves to another site while your password manager answers, nothing is filled. Passwords aren't kept in the command history, `:messages` or riptide's log.

## Password managers

| `backend` | Tool | Unlocking |
|---|---|---|
| `"pass"` (the default) | [pass](https://www.passwordstore.org/) | GPG's own pinentry |
| `"gopass"` | [gopass](https://www.gopass.pw/) | GPG's own pinentry |
| `"rbw"` | [rbw](https://github.com/doy/rbw), for Bitwarden | rbw's agent and pinentry |
| `"bw"` | the [Bitwarden CLI](https://bitwarden.com/help/cli/) | riptide asks for the master password once and keeps the session until riptide quits or `:password-lock` |
| `"keepassxc"` | `keepassxc-cli`, with `database = "~/Passwords.kdbx"` | riptide asks for the database password on every fill, or keeps it for `remember = 300` seconds |

The master and database passwords go to the tool on its input or in its environment, never on its command line, where other programs could see them. For `pass` and `gopass`, use a graphical pinentry (such as `pinentry-gnome3` or `pinentry-qt`): riptide has no terminal for a text one.

### How entries are matched

- **pass and gopass:** an entry belongs to a site when part of its path is the site, such as `websites/example.com/alice`. The first line is the password. The username is a `login:`, `user:`, `username:` or `email:` line, or else the part of the path after the site (`alice`).
- **Bitwarden and KeePassXC:** an entry belongs to a site when one of its saved addresses (or its name) is the site.

## Options

```lua
rt.pack.add({ builtin = "passwords", opts = {
  backend = "keepassxc",
  database = "~/Passwords.kdbx",
  keyfile = "~/Passwords.key",  -- keepassxc only, optional
  remember = 300,               -- keepassxc only: seconds to keep the database password
  submit = true,                -- press the form's submit button after :password-fill
  command = "/opt/bin/rbw",     -- the tool, if it isn't on your PATH
  store = "~/.password-store",  -- pass only, if not $PASSWORD_STORE_DIR
  keys = { login = "<Ctrl-Shift-l>" },  -- your own keys; false for none
} })
```
