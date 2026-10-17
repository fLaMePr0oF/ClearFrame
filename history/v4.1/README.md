# CleanFrame v4.1

Focused controls for exploratory testing. Each slider displays 0–100% and its effective v4 strength:

| Control | Slider 0% | Slider 50% (default) | Slider 100% |
|---|---:|---:|---:|
| Tone | -5% | 37.5% | 80% |
| Noise | 51% | 69.5% | 88% |
| Recap | 86% | 95.5% | 105% |

Mapping is linear: effective = minimum + slider / 100 × (maximum - minimum).
Each whole slider step changes Tone by 0.85, Noise by 0.37, and Recap by 0.19 original percentage points. Effective strengths are displayed to two decimal places.

The three effects remain independently switchable. Turning an effect off bypasses it completely; slider 0% means the minimum focused strength, not off. Tone -5% extrapolates the tone formula slightly in the opposite direction (not an exact inverse). Recap 105% extrapolates the existing formula beyond its former maximum. Neither endpoint is silently clipped to the old range.

All three focused sliders default to 50%. Framing retains its v4 0–100% range, starts disabled at 25%, and is independent of these mappings.

Run cleanframe.exe, or build with build-windows.bat / cargo build --release. Developer checks: cargo test.

Open or drag in PNG/JPEG/BMP. Process creates an output preview. Accept promotes it to input for cumulative processing. Changing settings makes an existing output outdated. Save writes the current result, protecting the original input file. Processing is offline and deterministic for the same input/settings; dimensions and formats follow v4 behaviour. Maximum input size: 25 megapixels.

For comparison with v4, record the effective strengths, source image, output format, enabled effects, framing, and accepted passes. Focused defaults are new and should not be compared directly with v4 defaults. Formal detector and image-quality acceptance testing remains with the tester.
