# Update signing key (F-34)

`fermata-update.pub` holds the **public** Ed25519 key (64 hex digits) that Mac and Windows builds check update files
against. It is built into the app (`src/update.rs`, `include_str!`). While it is empty, a build installs no updates:
every update fails the security check.

Before a release build:

1. Once: `cargo run --release -p sign-update -- keygen <folder outside the repository>` and copy the
   `fermata-update.pub` it writes here. The secret `fermata-update.key` never goes into the repository or a message.
2. Build (`make_mac_app.sh`, `make_windows.sh`), then sign each update file:
   `cargo run --release -p sign-update -- sign <secret key> dist/Fermata-macOS-update.zip` and the same for
   `dist/Fermata-Windows-x64.zip`. Upload both files and their `.sig` to the release.
