# Command and maintainer reference

## Commands

| Command | Description |
| --- | --- |
| `ncal auth login` | OAuth sign-in flow |
| `ncal auth from-app` | Import tokens from the desktop app |
| `ncal auth import-user-json` | Import browser LocalStorage `user.auth.currentUser` JSON |
| `ncal auth status` | Show credential source and expiry |
| `ncal auth refresh` | Refresh stored credentials |
| `ncal auth logout` | Remove stored credentials |
| `ncal whoami` | Show current authenticated user |
| `ncal accounts list` | List connected accounts |
| `ncal accounts set-primary <ID>` | Set primary account |
| `ncal accounts remove <ID>` | Disconnect account |
| `ncal calendars list` | List calendars |
| `ncal calendars colors` | Show calendar colors |
| `ncal events list` | List events |
| `ncal events get <ID>` | Show an event |
| `ncal events create` | Create an event |
| `ncal events update <ID>` | Update an event |
| `ncal events delete <ID>` | Cancel or delete an event |
| `ncal holds ...` | Manage scheduling holds |
| `ncal sync` | Fetch incremental changes |
| `ncal contacts` | List contacts |
| `ncal preferences get` | Show user preferences |

## Output contract

The CLI is designed for both humans and automation:

- Default output is human-readable tables or key-value text.
- `--json` writes compact JSON data to stdout.
- Errors and hints go to stderr.
- JSON errors use `{ "error", "code", "hint" }`.
- Exit codes are stable: `0` success, `2` usage, `3` auth, `4` API/network/I/O.

## Auth and stability

`ncal auth from-app` reads the Notion Calendar desktop app's Chromium LocalStorage LevelDB from `~/Library/Application Support/Notion Calendar/Local Storage/leveldb/` on macOS. CLI credentials are stored in the OS keychain when available, with file storage as a headless fallback. Do not commit tokens, cookies, captures, credential files, or personal calendar data.

The backend API base is `https://calendar-api.notion.so`. This is an unpublished Notion Calendar API surface and can change without notice. The research notes and API contract in `research/` are the source of truth for what has been confirmed.

## Development

```sh
make hooks   # install pre-commit/pre-push hooks
make fix     # rustfmt + clippy --fix
make ci      # fmt-check + clippy + tests
make build   # release build
```

The pre-commit and pre-push hooks auto-run `cargo fmt` and `cargo clippy --fix`. If they change files, they stop so you can review and stage the edits before continuing.

## Releasing

The release workflow is configured with [cargo-dist](https://github.com/axodotdev/cargo-dist). It is intended to build macOS and Linux binaries, create GitHub Release assets, publish checksums and an installer script, and push the Homebrew formula to `olegpustovit/homebrew-tap`.

Maintainer commands:

| Goal | Command |
| --- | --- |
| Bump patch + tag + push | `make ship-patch` |
| Bump minor + tag + push | `make ship-minor` |
| Validate release plan | `make dist-plan` |

Homebrew publishing requires a repo secret:

```sh
gh secret set HOMEBREW_TAP_TOKEN
```

The token must have write access to `olegpustovit/homebrew-tap`.

## License

MIT - see [LICENSE](../LICENSE).
