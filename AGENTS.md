# Project conventions

- The repository is ClearFrame; the application name remains CleanFrame.
- Active code is src/main.rs. history/ contains frozen source snapshots, not active crates.
- Preserve input/output, stale-result, Accept, and original-file protection semantics.
- Preserve slider mappings unless the user explicitly requests changes.
- Keep image processing offline. No detector service integration is configured.
- Run cargo test --locked and cargo build --release --locked for relevant changes.
- Verify UI changes visually on Windows when available.
- Formal test planning, execution, and acceptance belong to Steve. Keep developer validation separate; preserve raw QA evidence verbatim.
- Do not commit target directories, executables, generated ZIPs, credentials, or private test images.
