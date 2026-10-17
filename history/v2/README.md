# CleanFrame v0.2 — minimal offline image cleaner

A small native desktop application written in Rust. Drop a PNG, JPEG or BMP image onto the window (or use **Open…**), select any combination of the three processing methods, click **Process**, then **Save…**.

The window has no sliders, accounts, telemetry or network processing. The image is processed locally, in a background worker thread.

## Controls

- **Frequency-aware detail processing** — *on by default*. Uses two Gaussian scales to separate broad colour/shape from medium and very fine detail. Slightly attenuates the higher-frequency components, with protection for prominent edges.
- **Subpixel resampling** — *on by default*. Makes a small fractional-pixel interpolation pass and a reverse pass, keeping the original width, height and overall position while changing fine-scale sampling correlations.
- **Controlled compression and reconstruction** — *off by default*. Encodes decoded RGB pixels temporarily as quality-94 JPEG, decodes the result, then blends most of that reconstruction back into opaque pixels. This works even when the final output must be PNG or BMP. This step can visibly affect fine text, high-contrast edges and flat graphics, which is why it starts unchecked.

All steps are independent. They are applied in the order above when checked. Unchecking **all** still produces freshly encoded output without the source metadata; fully transparent pixels have their hidden RGB values cleared.

Changing a checkbox invalidates the previously processed output, requiring another click on **Process** before saving. You can process the same image repeatedly with different selections without reopening it.

## Windows build

1. Install [Rust](https://rustup.rs/) using the **stable MSVC** toolchain (and Microsoft C++ Build Tools if prompted).
2. Extract this project.
3. Double-click `build-windows.bat`, or open PowerShell in the project folder and run:

   ```powershell
   cargo build --release
   ```

4. Launch `target\release\cleanframe.exe`.

For source-level tests, run `cargo test`.

The binary has no console window in release mode. Dependencies are fetched from crates.io by Cargo at build time.

## Loading, transparency and saving

- Accepted inputs: **PNG, JPG/JPEG, BMP** (determined from the actual bytes, not just the extension).
- EXIF orientation is applied to decoded pixels so portrait photos remain upright after EXIF is discarded.
- Processing retains image dimensions and, for PNG/BMP, the alpha channel. Filtering is blocked near nonopaque areas to avoid altering antialiased transparency edges. Fully transparent pixels are saved with RGB set to zero.
- Output is a newly encoded image of the **same actual file format** as the input, with a suggested name like `photo_cleaned.jpg` or `graphic_cleaned.png`. The app prevents writing directly over the currently opened source image.
- Source EXIF, XMP, IPTC, C2PA/JUMBF and other auxiliary metadata blocks are not copied into the newly encoded file. The new encoder still writes the mandatory format structures. **This is metadata removal, not a guarantee that pixel-based provenance watermarks are absent.**
- JPEG output uses quality 96. JPEG files necessarily undergo a final lossy encode even when the compression checkbox is off.

## Limits

- **No detector-evasion guarantee.** This is an image-processing experiment, not a confirmed way of defeating ImageDetector.com or other AI-generated-image classifiers. Detectors may use semantic content, robust pixel-level watermarks and model-dependent statistics; some transformations may increase detection confidence.
- **Metadata removal can discard colour profiles.** The program does not colour-manage ICC/P3/CMYK images to sRGB before dropping their profiles. Wide-gamut input can change appearance in colour-managed viewers.
- Decoding converts source pixels to **8-bit RGBA**. A 16-bit PNG is therefore quantised to 8 bits. Animated/multipage inputs are treated as a single image.
- BMP alpha is encoded, but support for BMP transparency varies across other applications.
- JPEG reconstruction of an image with transparency only affects pixels sufficiently far from transparent boundaries; the alpha channel is not encoded in the intermediate JPEG.
- Large images consume additional memory for the Gaussian scales, interpolation and possible JPEG round trip. Inputs are not sandboxed, and damaged or huge images can fail. Insufficient disk space or permissions may leave a partial new output file.

## Project contents

```text
cleanframe-rust/
├── Cargo.toml
├── README.md
├── build-windows.bat
└── src/
    └── main.rs
```

## Dependencies

`eframe` 0.33.3 for native GUI, `image` 0.25.10 for codecs and transforms, and `rfd` 0.15.4 for native file-open/save dialogs.

The Rust toolchain is not present in the environment used to generate this source archive, so this version has **not been compiled or runtime-tested here**. The supplied `cargo test` suite covers independent checkbox transforms, dimension/alpha preservation and format encoding once built locally.
