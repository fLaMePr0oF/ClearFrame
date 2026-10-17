# CleanFrame

A deliberately minimal, offline image-processing desktop app in **Rust**.

- **Input:** drag and drop one image anywhere in the window, or click **Open…**.
- **Process:** click **Process**. A small preview is displayed.
- **Output:** click **Save…**. It suggests `<original_name>_cleaned.<original_extension>` and always uses the **same actual file format** as the source: PNG, JPEG or BMP. The original is not overwritten.

There is no strength slider, profile chooser, account, telemetry or network operation. Large images are processed in a worker thread.

## Building on Windows

1. Install a current stable Rust toolchain from <https://rustup.rs/>. Choose the **MSVC** toolchain on Windows; install the Microsoft C++ build tools if rustup asks for them.
2. Extract the downloaded source ZIP into a directory.
3. Double-click **`build-windows.bat`** from that directory. Alternatively, open PowerShell in the directory and run:

   ```powershell
   cargo build --release
   ```

4. Run **`target\release\cleanframe.exe`**. The release executable does not open a console window.

To run tests (once Rust is installed): `cargo test`.

## What it actually does

1. Checks the file format from the file contents and decodes its pixels.
2. Reads and **applies EXIF orientation** before dropping the source metadata, so portrait JPEGs stay upright.
3. For fully opaque images of at least 64 × 64 pixels, makes a very slight Lanczos down/up resampling pass (to ~98.5% of the original dimensions and back) blended at 26% with the original. Final output dimensions are unchanged.
4. Adds extremely low-amplitude pseudo-random luminance and chroma grain to opaque pixels. Semi-transparent edge pixels and their alpha are preserved; RGB values of **fully transparent pixels are zeroed** to eliminate hidden pixel payloads.
5. Creates a **fresh PNG, JPEG or BMP** using the `image` library's encoder. EXIF, XMP, IPTC, C2PA/JUMBF blocks, textual PNG metadata, GPS tags, editing-history tags and ICC profile bytes from the source are **not copied** into the output. Encoders can include their own necessary format headers; this does not promise literally zero nonpixel header bytes.

For JPEG, re-encoding uses quality 96, which is necessarily lossy. PNG and BMP outputs use lossless encoding of the processed 8-bit RGBA pixels.

The source file stays untouched. All work is local.

## Limits / important caveats

- **No guarantee against AI-image detectors.** There is no reliable general-purpose transformation that makes all generated images undetectable without substantially changing appearance. Content-level cues and robust invisible/embedded pixel watermarks may remain. The app does not detect or specifically erase any watermarking algorithm.
- **Colour profiles:** the app removes embedded ICC profiles but does **not** convert their colour spaces to sRGB first. Wide-gamut or custom-profiled source images may therefore look different in colour-managed viewers. This is an intentional limitation of this simple implementation.
- **Bit depth:** imported 16-bit PNG images are converted to 8-bit per channel. Animated PNGs and multipage images are not processed as animations.
- **Transparency:** transparent PNG/BMP images skip the resizing pass to avoid alpha halos; their opacity is unchanged. Pixel RGB values at fully transparent locations are cleared.
- **Format fidelity:** output preserves **format**, not file size, exact compressed bytes, JPEG quantization tables, PNG palette mode, metadata, or BMP header version.
- **Very large or corrupt files:** decoding and resampling can use substantial memory. Some malformed files may be rejected; as with any image decoder, do not treat it as a secure file-sanitization boundary.
- **Saving:** make sure the chosen output path is writable and contains sufficient space. If writing fails midway, a partial destination file may remain. The original input file is protected from direct overwrite by the app.

## Source layout

```text
cleanframe-rust/
├── Cargo.toml
├── README.md
├── build-windows.bat
└── src/
    └── main.rs
```

## Dependencies

Rust libraries: `eframe` 0.33.3 (egui window), `image` 0.25 (decoding/encoding), `rfd` 0.15 (system dialogs) and `rand` 0.8 (noise generation). These dependencies are downloaded by Cargo when you build the project.
