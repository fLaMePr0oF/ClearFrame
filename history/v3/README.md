# CleanFrame v3 — local image processing (Rust)

CleanFrame is a deliberately minimal desktop application for exploring how re-encoding and different pixel-processing pipelines affect image appearance and third-party classifiers. It is **not** an AI-origin verifier and makes **no guarantee** about how any specific detector will classify an image.

All processing runs locally. Drag-and-drop a file anywhere in the window or click **Open…**, tick any combination of methods, choose **Process**, and finally **Save…**. There is no upload, analytics, account, web request, hidden auto-detection or external AI service in the application.

## Processing controls

| Checkbox | Initially | Action |
| --- | --- | --- |
| Frequency-aware detail processing | Off | Attenuates two fine-frequency bands, protecting significant edges. |
| Subpixel resampling | Off | Two opposite fractional-pixel interpolations, preserving overall position. |
| Controlled compression and reconstruction | Off | Internal quality-94 JPEG round trip, blended with the input. The final output remains in its original format. |
| Camera capture simulation | **On** | Models lower effective resolution (84%), optical blur, Bayer RGGB colour sampling, demosaicing, slight colour response, vignette and deterministic sensor noise. |
| Photographic framing | Off | Adds a thin pale blue-white border, asymmetric margins and subtle perspective/keystone distortion. |

The earlier three processing stages are retained from v2. Processing order is **frequency → subpixel → controlled compression → framing → camera simulation**, skipping any unchecked method. The framing runs *before* the camera stage so the border is also photographed when both are selected. All five methods can operate independently, including **no methods checked** (metadata re-encoding only).

The default is *Camera capture simulation only* so experiments can isolate its effect. This is intentionally different from v2's two enabled defaults.

## Repeatability and interpretation

Sensor and border noise are generated using a **fixed internal seed**, `0xC1EA4F3A`, and a coordinate-based hash. The same decoded image, choices and application version should produce identical pixel output over repeated Process clicks; no clock or random generator state is used. Output JPEG encoding is also configured at a fixed quality of 96. The fixed seed is not a user control, to keep the UI minimalist.

A lower classifier score after a transformation is **not evidence** that an image originated from a real camera. Likewise, camera-style processing cannot guarantee a lower detector score. The app never fabricates camera EXIF claims, timestamps or provenance metadata.

For controlled experiments, start with one original image, record the complete checkbox combination, process each configuration and save with distinct names. Upload the **same saved file** repeatedly when testing detector variability; do not resave it between repeats. See `testing-template.csv` for a suggested logging sheet.

## Formats / file behaviour

- Accepts actual PNG, JPEG (JPG/JPEG) and BMP files, regardless of the extension used to open them.
- Writes the **same format**, with the same original pixel width and height; the save dialog proposes a `_cleaned` filename and does not allow overwriting the opened source file.
- Decodes pixels afresh and encodes a new image, leaving behind auxiliary EXIF, XMP, IPTC, C2PA/JUMBF and most other ancillary metadata structures. Ordinary mandatory JPEG/PNG/BMP format markers remain. This is not guaranteed removal of pixel-level watermarks.
- Applies orientation found in EXIF *before* removing the metadata.
- Without photographic framing, PNG/BMP alpha values are retained. Transparent edge pixels are protected from filters; fully transparent RGB payloads are set to zero.
- With **Photographic framing**, image content is composited onto an **opaque** blue-white matte, intentionally discarding any original transparency. Disable it to preserve transparency.
- The default JPEG output quality is 96, so any JPEG input is re-encoded lossy even with all checkboxes disabled.

## Build for Windows

1. Install [Rust / rustup](https://rustup.rs/) for Windows, using the stable **MSVC** toolchain. If prompted, install Microsoft's C++ Build Tools.
2. Extract the ZIP archive.
3. Double-click `build-windows.bat`, or open PowerShell in the extracted directory and run:

   ```powershell
   cargo build --release
   ```

4. Start `target\release\cleanframe.exe`. To run the automated tests:

   ```powershell
   cargo test
   ```

Rust dependencies are downloaded from crates.io at build time. The application itself does not require a network connection.

### Dependencies

- `eframe` 0.33.3 — desktop UI / drag-and-drop
- `image` 0.25.10 — PNG, JPEG, BMP codecs and resampling
- `rfd` 0.15.4 — native file selection dialogs

## Limitations

- The simulated sensor is a **simplified RGB/Bayer model**, not a physical simulation of a particular phone or display; it does not simulate real sensor RAW, display spectral response, all display subpixels, actual JPEG camera ISP processing, motion or lens geometry.
- The photographic border changes composition by slightly reducing the area covered by the original artwork. Some small text and hard edges may become softer, especially when camera simulation is enabled.
- Decoder conversion to 8-bit RGBA can lose precision on 16-bit PNGs. Animated or multipage images are read as a single image.
- Metadata removal also drops colour profiles without full ICC/P3/CMYK-to-sRGB colour management. Colours can change on wide-gamut inputs.
- Results from an online image detector may vary for reasons unrelated to the processing; its output percentages need not be calibrated probabilities. Uploading to third-party testing services is **not** performed by the app.
- Large or damaged files can use substantial memory or cause processing errors. Save failures (permissions, disk capacity) are reported in the UI.

**Build verification:** The source was assembled and checked for packaging consistency in an environment without the Rust compiler or Cargo. It has not been compiled, executed or runtime-tested there. Please run `cargo test` and `cargo build --release` on your machine; if a compiler error appears, provide the full console output for correction.

## Project files

```text
cleanframe-v3-rust/
├── Cargo.toml
├── README.md
├── build-windows.bat
├── testing-template.csv
└── src/
    └── main.rs
```
