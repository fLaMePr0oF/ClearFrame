# Development

Use a feature branch and a pull request for changes after the initial import. Keep Cargo.lock committed. Run `cargo test --locked` and `cargo build --release --locked`; inspect UI changes on Windows. Record developer validation in the PR.

Formal test planning and execution are owned by Steve. Do not edit raw test evidence to match implementation or describe developer checks as acceptance approval.

Dependency updates arrive as monthly Dependabot PRs and require normal review and validation.

## Releases

After independent acceptance, update the application version, Cargo package version/lockfile, and changelog consistently. Run the Windows workflow and verify the resulting executable. Publish an approved executable and checksum through GitHub Releases when requested. The CI workflow only uploads build artifacts; it does not publish releases automatically.

## Imported history

Earlier releases were developed outside Git. Their original source snapshots are preserved under history/. This import does not fabricate historical commit dates or imply every snapshot was acceptance-tested.
