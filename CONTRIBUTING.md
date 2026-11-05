# Contributing

Thanks for your interest in Brasa. Bug reports, translations and pull requests are welcome.

## Before you start

- **Bugs:** open an issue using the bug report template. The fastest way to give us everything we
  need is **Report a problem** in Brasa (console, crash card or Settings → About). It uploads a
  redacted log and gives you a link.
- **Larger changes:** open an issue first, so we can agree on the approach before you invest time
  in it.
- **Security issues:** don't open a public issue. See [SECURITY.md](SECURITY.md).

## Development setup

You need Rust 1.91 or newer, plus the Linux packages listed in the [README](README.md#build-from-source).

```sh
cargo run                      # GUI
cargo run -- --help            # CLI
BRASA_HOME=/tmp/brasa-dev cargo run   # use a throwaway data directory
```

In debug builds, `BRASA_UI_DEMO=1` fills the GUI with sample states (a game preparing, another
running, a crash in the console), so you can work on the interface without launching Minecraft.

## Checks

CI runs the following, and a pull request must pass all of it:

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items
cargo test --locked
```

If you add or update a dependency, regenerate the notices:

```sh
python3 scripts/third_party.py > THIRD_PARTY_NOTICES.md
```

## Conventions

- **Language.** Code comments, docs inside `src/` and source UI strings are in Spanish. User-facing
  strings are wrapped in `t!("…")`, and their English translation goes in `i18n/en.tsv`, one
  `spanish<TAB>english` pair per line. A test fails if a string is missing from the table, and
  `python3 scripts/i18n_scan.py` lists unwrapped strings.
- **Architecture.** The GUI follows a model, view, controller split. Read
  [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) before adding a screen or an action.
- **Errors.** Use `anyhow` with context that a player can understand. A message shown in the UI
  should say what happened and what to do next.
- **Safety.** `unsafe` is denied crate-wide. The few platform calls that need it are annotated
  individually and come with a `SAFETY:` comment.
- **Tests.** New logic comes with tests, and tests must not need network access or a display.
- **Commits.** Keep commits focused, and write messages in the imperative ("Add …", "Fix …").

## License

By contributing, you agree that your contributions are licensed under the [MIT license](LICENSE).
