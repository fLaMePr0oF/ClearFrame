# CleanFrame v4.2

Based on v4.1. Only Recap's available range is changed: the displayed 0–100% slider maps linearly to effective v4 strength 0–115%. Recap's processing formula is extended to 115% without clipping at the old maximum.

| Control | Effective minimum | Effective maximum | Default effective strength |
|---|---:|---:|---:|
| Tone | -5% | 80% | 37.5% |
| Noise | 51% | 88% | 69.5% |
| Recap | 0% | 115% | 95.5% |
| Framing | 0% | 100% | 25% (disabled) |

Tone, Noise, and Framing mappings and defaults are unchanged from v4.1. Recap preserves its previous effective default of 95.5%, now displayed at approximately 83% of the expanded slider. One displayed Recap percentage point changes effective strength by 1.15 percentage points. Effective strengths appear beside the sliders to two decimal places. Effect toggles bypass processing completely. Tone and Noise slider zero means their focused minimum; Recap and Framing zero mean no effect.

For comparison: displayed 100/100/100/25 in v4.2 means effective 80/88/115/25. Enable Framing explicitly when comparing framed results. Slider percentages from v4.1 are not directly comparable for Recap; use effective strengths.

Run cleanframe.exe. Build from source with build-windows.bat or cargo build --release. Developer validation: cargo test (11 passed) including mapping endpoints and Recap 0/115 support. Formal acceptance and external detector testing remain separate.

Open or drag in PNG/JPEG/BMP, Process, compare Input/Output, then optionally Accept to promote output to input for a cumulative pass. Save protects the original opened file. Changing settings marks existing output outdated. Processing is offline and deterministic for identical input/settings; files are re-encoded without copying original metadata. Framing composites transparency onto its matte. Input limit: 25 megapixels.
