# CleanFrame

**Local image processing for photographic effects and AI image detection experiments.**

[**Try CleanFrame online →**](https://imagedisguiser.com/)

CleanFrame adjusts tone and colour, photographic noise and texture, screen recapture effects, and framing. Use it to compare visual changes and investigate how image processing affects external AI image detection results.

**CleanFrame** is the application name; **ClearFrame** is the name of this GitHub repository. This repository contains the offline Windows application written in Rust. The browser version is available at [ImageDisguiser.com](https://imagedisguiser.com/) and runs its processing locally using WebAssembly.

Current application release: **v4.3** (Cargo package version `0.4.3`, retaining the existing version convention).

[![CleanFrame web interface with input and output previews](https://imagedisguiser.com/cleanframe-social-preview.png)](https://imagedisguiser.com/)

## Use CleanFrame in your browser

Open [CleanFrame — AI Image Disguiser](https://imagedisguiser.com/) to use the web tool without installing the Windows application.

1. Open or drag in a PNG, JPEG or BMP image, up to 25 megapixels.
2. Adjust the four processing channels and select **Process**.
3. Compare the Input and Output previews.
4. Select **Save**, or **Accept as Input** to apply another processing pass.

Image processing happens on your device; image files are not uploaded for processing. The interface supports English, Español, Français, Italiano, Deutsch and 中文. Original metadata is not copied into saved images.

CleanFrame processes images; it does not provide an AI detection score. To test classification changes, compare original and processed files with an external detector. Results depend on the image, settings and detector, and no particular score is guaranteed.

## Reproducible AI image detection testing

For useful comparisons, keep an untouched original and record the image format, enabled channels, slider values, number of accepted passes, detector name and test date. Change one factor at a time where practical, and record repeated results rather than relying on a single score.

The [QA documentation and recording template](QA/README.md) provide a starting point for contributing test observations. See the slider mappings below for the effective strengths behind the displayed percentages.

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
