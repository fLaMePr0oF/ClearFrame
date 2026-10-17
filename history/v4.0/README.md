# CleanFrame v4

A minimal, offline Windows desktop image-processing application written in Rust. Open or drag in a PNG, JPEG/JPG, or BMP; adjust four independent effects; inspect Input and Output side by side; and accept a result to make it the input for another pass.

**Research / experimentation notice:** ImageDetector.com scores are not guaranteed to change. These controls provide adjustable image transformations for subjective image-quality assessment and reproducible testing. Re-encoding strips embedded metadata but does not establish that an image was captured by a physical camera, and does not ensure that every provenance marker or watermark is gone. Do not present processed AI artwork as human-created or camera-original imagery.

## Build (Windows)

Install Rust from https://rustup.rs/ and the Microsoft C++ Build Tools (MSVC). Then double-click `build-windows.bat`, or open a terminal in the extracted project directory:

```powershell
cargo build --release
```

Run `target\release\cleanframe.exe`. Alternatively, for a development build run `cargo run`. Test with `cargo test`.

The full source is in `src/main.rs`; this package does not include a compiled Windows executable.

## Simple workflow

1. Drag a PNG/JPEG/BMP into the window, or click **Open…**.
2. Toggle any of the four effects and set each intensity between **0% and 100%**.
3. Click **Process** to generate an **Output** preview. The **Input** never changes at this stage.
4. Compare both panes. Adjust any slider or checkbox, then click **Process** again to replace only the Output. Changing settings leaves the earlier Output visible but marks it as outdated and disables Accept and Save for that outdated result.
5. Click **Accept → Input** to promote the last processed Output to Input. The Output pane clears. The next Process pass is genuinely cumulative.
6. Click **Save…** to save the current Output, or — after Accept, with no pending Output — the accepted Input. The original opened image file is protected from overwriting.

The selected processing settings remain in place after Accept. Use different strengths for successive accepted passes, or reapply exactly the same settings intentionally.

## Controls

| Effect | Toggle | Strength | Meaning of 100% |
|---|---|---:|---|
| Tone / colour remapping | Yes | 0–100% | Larger white-balance, tonal and saturation shifts |
| Photographic noise / texture | Yes | 0–100% | More luminance/chroma grain and reconstruction sharpening |
| Display recapture simulation | Yes | 0–100% | Stronger display subpixel sampling, optical low-pass, Bayer reconstruction and low-amplitude moiré |
| Photographic framing | Yes | 0–100% | Wider uneven border and greater keystone distortion |
| Metadata removal | Always on | N/A | Original EXIF/XMP/profiles are not copied into the output encoder |

An unchecked effect does nothing, whatever its slider value. An enabled effect at 0% also does nothing. Slider values are strengths within deliberately bounded presets; they are not physical measurements or predicted classifier probabilities. Defaults enable the first three effects at moderate strength and disable framing.

### Operation order

**Framing → Display recapture → Tone/colour → Noise/texture → Fresh format encoder.** This order follows the conceptual screen-to-camera rendering path. The internal pseudorandom texture uses a fixed seed for identical output when the input pixels and settings are unchanged; it is not stochastic across repeated Process clicks.

## Input and output behaviour

- PNG, JPG/JPEG and BMP are accepted, with the output in the same format and at the same pixel dimensions as the *visually oriented* decoded input. An EXIF orientation flag is applied before original metadata is discarded.
- Metadata is stripped by re-encoding the image from decoded pixels on **Save**. This also removes colour profiles (ICC), which can cause colour appearance changes in wide-gamut images.
- Transparent PNGs preserve their alpha channel when Framing is disabled (fully transparent pixel RGB values are normalised). Framing intentionally composites onto an opaque blue-white matte. JPEG has no transparency; alpha persistence in BMP may depend on external decoder support.
- Image processing and writing occur on worker threads so the window stays responsive. Images larger than 25 megapixels are rejected to reduce the risk of excessive RAM usage. Multiple expensive effects can still take time on large files.
- The app does **not** upload images or invoke any online detector.

## Evaluation

Use the included `testing-template.csv` to record input version, checkboxes, strengths, number of accepted passes, and external score. Test one variable at a time initially; avoid interpreting tiny changes in reported scores as meaningful without repeated trials. A photographed display of AI artwork is still a photograph of AI artwork, not evidence the underlying illustration originated in a camera.

## Validation status

The source has unit tests covering zero-strength/no-op behaviour, four effects, deterministic repeatability, cumulative passes, transparency, output dimensions and formats, and candidate/accepted input states. **Compilation and execution have not been verified in this environment** because Cargo/Rust is unavailable here. Run `cargo test` on your Windows machine before relying on it. Please share any compiler error verbatim for a precise fix.
