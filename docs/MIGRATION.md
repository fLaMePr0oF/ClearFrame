# Repository migration

Imported from the local versioned source folders on 17 October 2026.

The root crate is the latest v4.3 layout revision. src/main.rs and Cargo.lock are copied from the working release, without changing processing or UI code during migration. Earlier source packages are preserved under history/. QA records and the user-supplied layout mockup are included. Original local folders remain intact.

Generated binaries, target directories, source ZIPs, and the Rust installer are excluded from Git. Rebuild the current executable locally or download a successful Windows Actions artifact.

Repository name: ClearFrame. Product and binary names remain CleanFrame / cleanframe.exe.

The crate already declares MIT in Cargo.toml. No new copyright attribution or standalone license grant was invented during migration; the owner can add the appropriate LICENSE file.
