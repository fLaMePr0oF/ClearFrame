# CleanFrame

Offline Windows image processing in Rust, hosted in the **ClearFrame** repository. Current application release: **v4.3** (Cargo package version `0.4.3`, retaining the existing version convention).

## Build and run

Install stable Rust for `x86_64-pc-windows-msvc` and Microsoft C++ Build Tools with the Desktop development with C++ workload and Windows SDK.

```powershell
git clone https://github.com/fLaMePr0oF/ClearFrame.git
cd ClearFrame
cargo test --locked
cargo build --release --locked
.\target\release\cleanframe.exe
```

Alternatively, double-click `build-windows.bat`. GitHub Actions tests and builds on Windows, then provides a **CleanFrame-windows-x64** artifact containing the executable and SHA-256 checksum. Download it from a successful run under Actions. These are development artifacts; formal acceptance is separate.

## Workflow

1. Upload or drag in a PNG, JPEG, or BMP (up to 25 megapixels).
2. Enable effects and adjust sliders. Process creates an Output preview without changing Input.
3. Adjust settings and Process again, or choose **Accept as Input** to make the result the input for a cumulative pass.
4. Save the current output, or the accepted input after Accept. The originally opened file is protected from overwriting.

Changing settings makes the existing output stale and disables accepting/saving that result until it is processed again. Processing is deterministic for identical pixels and settings. Images stay local; the application does not call an online detector.

## Slider mappings

Displayed sliders run from 0–100. Their underlying strengths use the original v4 scale:

| Effect | Effective range | Default effective value | Default checkbox |
|---|---:|---:|---|
| Tone / colour | -5–80 | 37.5 | On |
| Noise / texture | 51–88 | 69.5 | On |
| Display recapture | 0–115 | 95.5 | On |
| Framing | 0–100 | 25 | Off |

Effective strength = minimum + displayed percentage / 100 × (maximum − minimum). Recap's default displays approximately 83%; Tone and Noise display 50%. Turning a checkbox off bypasses the effect. Slider zero means the lower end of its range, which is not necessarily no effect.

Framing → Recapture → Tone → Noise → fresh encoding. Negative Tone and Recap over 100 extrapolate the existing formulas. Metadata is not copied when saving; embedded colour profiles are also removed, so appearance can change. Without framing, transparent PNG alpha is preserved. Framing composites onto an opaque matte. JPEG has no transparency.

External detector scores are experimental observations, not guarantees of image origin or classification. The UI's requested introductory wording should be read in that context.

## Project layout

- `src/main.rs`: current application and developer tests.
- `QA/`: original exploratory/pairwise records and recording template.
- `history/`: source snapshots of earlier releases, excluding build products.
- `docs/`: development notes and the v4.3 layout reference.
- `.github/`: Windows CI, dependency updates, and issue/PR templates.

The title loads Arial Bold from Windows' font directory, with an available-font fallback. No Microsoft font files are redistributed.

See [CONTRIBUTING.md](CONTRIBUTING.md), [CHANGELOG.md](CHANGELOG.md), and [QA/README.md](QA/README.md).
