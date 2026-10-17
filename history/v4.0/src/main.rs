#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::{mpsc, Arc},
    thread,
};

use eframe::egui;
use image::{
    codecs::{bmp::BmpEncoder, jpeg::JpegEncoder, png::PngEncoder},
    imageops,
    DynamicImage, ExtendedColorType, ImageDecoder, ImageEncoder, ImageFormat, ImageReader,
    RgbaImage,
};

const OUTPUT_JPEG_QUALITY: u8 = 96;
const PROCESSING_SEED: u32 = 0xC1EA_4F3A;
const MAX_PIXELS: u64 = 25_000_000;

// Checkbox enables/disables a whole effect. Slider is an effect strength, not
// a prediction about external classifiers. A zero-strength effect is a no-op.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Effect {
    enabled: bool,
    strength: f32, // 0.0..=1.0
}

impl Effect {
    fn amount(self) -> f32 {
        if self.enabled { self.strength.clamp(0.0, 1.0) } else { 0.0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ProcessingOptions {
    tone: Effect,
    texture: Effect,
    recapture: Effect,
    framing: Effect,
}

impl Default for ProcessingOptions {
    fn default() -> Self {
        Self {
            tone: Effect { enabled: true, strength: 0.35 },
            texture: Effect { enabled: true, strength: 0.35 },
            recapture: Effect { enabled: true, strength: 0.50 },
            framing: Effect { enabled: false, strength: 0.25 },
        }
    }
}

struct Document {
    source: PathBuf,
    format: ImageFormat,
    input: Arc<RgbaImage>,
    candidate: Option<Arc<RgbaImage>>,
    candidate_options: Option<ProcessingOptions>,
    accepted_passes: usize,
}

impl Document {
    fn candidate_is_current(&self, options: ProcessingOptions) -> bool {
        self.candidate.is_some() && self.candidate_options == Some(options)
    }
}

enum JobResult {
    Opened(Result<Document, String>),
    Processed(Result<RgbaImage, String>, ProcessingOptions),
    Saved(Result<PathBuf, String>),
}

struct CleanFrame {
    document: Option<Document>,
    input_preview: Option<egui::TextureHandle>,
    output_preview: Option<egui::TextureHandle>,
    sender: mpsc::Sender<JobResult>,
    receiver: mpsc::Receiver<JobResult>,
    options: ProcessingOptions,
    busy: bool,
    status: String,
}

impl CleanFrame {
    fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            document: None,
            input_preview: None,
            output_preview: None,
            sender,
            receiver,
            options: ProcessingOptions::default(),
            busy: false,
            status: "Drop an image, or click Open".into(),
        }
    }

    fn poll_jobs(&mut self, ctx: &egui::Context) {
        while let Ok(result) = self.receiver.try_recv() {
            self.busy = false;
            match result {
                JobResult::Opened(Ok(doc)) => {
                    self.input_preview = Some(make_preview(ctx, &doc.input, "input-preview"));
                    self.output_preview = None;
                    self.document = Some(doc);
                    self.status = "Ready — adjust the settings and click Process".into();
                }
                JobResult::Opened(Err(err)) => {
                    self.document = None;
                    self.input_preview = None;
                    self.output_preview = None;
                    self.status = format!("Open failed: {err}");
                }
                JobResult::Processed(Ok(image), applied_options) => {
                    self.output_preview = Some(make_preview(ctx, &image, "output-preview"));
                    if let Some(doc) = &mut self.document {
                        doc.candidate = Some(Arc::new(image));
                        doc.candidate_options = Some(applied_options);
                    }
                    self.status = "Compare both previews, then Accept or adjust settings".into();
                }
                JobResult::Processed(Err(err), _) => {
                    // Preserve the last successful candidate if processing failed.
                    self.status = format!("Processing failed: {err}");
                }
                JobResult::Saved(Ok(path)) => {
                    self.status = format!("Saved: {}", path.display());
                }
                JobResult::Saved(Err(err)) => self.status = format!("Save failed: {err}"),
            }
        }
    }

    fn open_path(&mut self, path: PathBuf, ctx: &egui::Context) {
        if self.busy { return; }
        self.document = None;
        self.input_preview = None;
        self.output_preview = None;
        self.busy = true;
        self.status = "Opening image…".into();
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let result = read_document(path);
            let _ = sender.send(JobResult::Opened(result));
            ctx.request_repaint();
        });
    }

    fn process(&mut self, ctx: &egui::Context) {
        if self.busy { return; }
        let Some(doc) = &self.document else { return; };
        let input = Arc::clone(&doc.input);
        let applied_options = self.options;
        self.busy = true;
        self.status = "Processing…".into();
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let result = process_pixels(&input, applied_options);
            let _ = sender.send(JobResult::Processed(result, applied_options));
            ctx.request_repaint();
        });
    }

    fn accept(&mut self, ctx: &egui::Context) {
        if self.busy { return; }
        let Some(doc) = &mut self.document else { return; };
        if !doc.candidate_is_current(self.options) { return; }
        let Some(accepted) = doc.candidate.take() else { return; };
        doc.input = accepted;
        doc.candidate_options = None;
        doc.accepted_passes += 1;
        self.input_preview = Some(make_preview(ctx, &doc.input, "input-preview"));
        self.output_preview = None;
        self.status = format!(
            "Accepted pass {} — new input ready for another pass", doc.accepted_passes
        );
    }

    fn save(&mut self, ctx: &egui::Context) {
        if self.busy { return; }
        let Some(doc) = &self.document else { return; };

        // When a current candidate exists, save what the Output pane shows.
        // After Accept, save the accepted Input (not the original source file).
        let image = if doc.candidate_is_current(self.options) {
            doc.candidate.as_ref().map(Arc::clone)
        } else if doc.candidate.is_none() && doc.accepted_passes > 0 {
            Some(Arc::clone(&doc.input))
        } else {
            None
        };
        let Some(image) = image else { return; };

        let format = doc.format;
        let source = doc.source.clone();
        let ext = preferred_extension(&source, format);
        let stem = source.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
        let suggested = format!("{stem}_cleaned.{ext}");
        let dialog = rfd::FileDialog::new()
            .add_filter(format_name(format), &[ext])
            .set_file_name(&suggested);
        let dialog = if let Some(parent) = source.parent() {
            dialog.set_directory(parent)
        } else { dialog };
        let Some(mut path) = dialog.save_file() else { return; };
        if !path.extension().is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case(ext)) {
            path.set_extension(ext);
        }
        // Never overwrite the image we originally opened.
        if let (Ok(original), Ok(destination)) = (fs::canonicalize(&source), fs::canonicalize(&path)) {
            if original == destination {
                self.status = "Choose another filename — the original input is protected".into();
                return;
            }
        }

        self.busy = true;
        self.status = "Saving…".into();
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let result = write_image(&path, format, &image)
                .map(|()| path)
                .map_err(|e| e.to_string());
            let _ = sender.send(JobResult::Saved(result));
            ctx.request_repaint();
        });
    }
}

impl eframe::App for CleanFrame {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_jobs(ctx);
        let dropped = ctx.input(|i| i.raw.dropped_files.iter().find_map(|f| f.path.clone()));
        if let Some(path) = dropped {
            if self.busy {
                self.status = "Finish the current operation before opening another image".into();
            } else {
                self.open_path(path, ctx);
            }
        }
        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(8.0);
                    ui.heading("CleanFrame");
                    ui.label("PNG  ·  JPG  ·  BMP");
                    ui.add_space(8.0);

                    let pane_width = ((ui.available_width() - 36.0) * 0.5).clamp(225.0, 510.0);
                    ui.horizontal(|ui| {
                        draw_preview_pane(ui, "Input", self.input_preview.as_ref(), pane_width, hovering, false);
                        ui.add_space(7.0);
                        let stale = self.document.as_ref().is_some_and(|doc| {
                            doc.candidate.is_some() && !doc.candidate_is_current(self.options)
                        });
                        draw_preview_pane(ui, "Output", self.output_preview.as_ref(), pane_width, false, stale);
                    });
                    ui.add_space(12.0);

                    ui.vertical(|ui| {
                        ui.label("Processing intensity");
                        ui.add_space(4.0);
                        let mut changed = false;
                        changed |= effect_row(ui, "Tone / colour remapping", &mut self.options.tone, self.busy);
                        changed |= effect_row(ui, "Photographic noise / texture", &mut self.options.texture, self.busy);
                        changed |= effect_row(ui, "Display recapture simulation", &mut self.options.recapture, self.busy);
                        changed |= effect_row(ui, "Photographic framing", &mut self.options.framing, self.busy);
                        if changed && self.document.as_ref().is_some_and(|d| d.candidate.is_some()) {
                            self.status = "Settings changed — click Process to refresh Output".into();
                        }
                        ui.add_space(3.0);
                        ui.small("Metadata removed on Save  ·  Fixed processing seed for repeatability");
                    });
                    ui.add_space(14.0);

                    ui.horizontal(|ui| {
                        if ui.add_enabled(!self.busy,
                            egui::Button::new("Open…").min_size(egui::vec2(128.0, 36.0))
                        ).clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("Images", &["png", "jpg", "jpeg", "bmp"])
                                .pick_file()
                            {
                                self.open_path(path, ctx);
                            }
                        }

                        if ui.add_enabled(!self.busy && self.document.is_some(),
                            egui::Button::new("Process").min_size(egui::vec2(128.0, 36.0))
                        ).clicked() {
                            self.process(ctx);
                        }

                        let can_accept = !self.busy && self.document.as_ref()
                            .is_some_and(|d| d.candidate_is_current(self.options));
                        if ui.add_enabled(can_accept,
                            egui::Button::new("Accept → Input").min_size(egui::vec2(128.0, 36.0))
                        ).clicked() {
                            self.accept(ctx);
                        }

                        let can_save = !self.busy && self.document.as_ref().is_some_and(|d| {
                            d.candidate_is_current(self.options)
                                || (d.candidate.is_none() && d.accepted_passes > 0)
                        });
                        if ui.add_enabled(can_save,
                            egui::Button::new("Save…").min_size(egui::vec2(128.0, 36.0))
                        ).clicked() {
                            self.save(ctx);
                        }
                    });
                    ui.add_space(8.0);
                    if let Some(doc) = &self.document {
                        let name = doc.source.file_name()
                            .map(|n| n.to_string_lossy()).unwrap_or_default();
                        ui.small(format!("{name}  ·  {}  ·  {} accepted pass(es)",
                            format_name(doc.format), doc.accepted_passes));
                    }
                    ui.label(&self.status);
                    ui.add_space(12.0);
                });
            });
        });
    }
}

fn draw_preview_pane(ui: &mut egui::Ui, title: &str, texture: Option<&egui::TextureHandle>,
                     width: f32, hovering: bool, stale: bool) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_min_size(egui::vec2(width, 326.0));
        ui.set_max_width(width);
        ui.vertical_centered(|ui| {
            ui.strong(if stale { format!("{title} · settings changed") } else { title.into() });
            ui.add_space(5.0);
            if hovering {
                ui.add_space(106.0);
                ui.label("Drop to open image");
            } else if let Some(texture) = texture {
                let natural = texture.size_vec2();
                let scale = ((width - 16.0) / natural.x).min(282.0 / natural.y).min(1.0);
                ui.add(egui::Image::new((texture.id(), natural * scale)));
            } else {
                ui.add_space(106.0);
                ui.label(if title == "Input" { "Drag and drop an image" } else { "Process to preview" });
            }
        });
    });
}

fn effect_row(ui: &mut egui::Ui, title: &str, effect: &mut Effect, busy: bool) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        changed |= ui.add_enabled(!busy, egui::Checkbox::new(&mut effect.enabled, title)).changed();
        let slider = egui::Slider::new(&mut effect.strength, 0.0..=1.0)
            .step_by(0.01)
            .custom_formatter(|value, _| format!("{:.0}%", value * 100.0))
            .custom_parser(|text| text.trim().trim_end_matches('%').trim().parse::<f64>()
                .ok().map(|v| v / 100.0))
            .show_value(true);
        changed |= ui.add_enabled(!busy && effect.enabled, slider).changed();
    });
    changed
}

fn read_document(path: PathBuf) -> Result<Document, String> {
    let reader = ImageReader::open(&path)
        .map_err(|e| e.to_string())?
        .with_guessed_format().map_err(|e| e.to_string())?;
    let format = reader.format().ok_or("Unrecognised image format")?;
    if !matches!(format, ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::Bmp) {
        return Err("Only PNG, JPG/JPEG and BMP are supported".into());
    }
    let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    let (w, h) = decoder.dimensions();
    if w as u64 * h as u64 > MAX_PIXELS {
        return Err("This image is too large (maximum 25 megapixels)".into());
    }
    // Read and apply EXIF orientation before dropping the original metadata.
    let orientation = decoder.orientation().map_err(|e| e.to_string())?;
    let mut decoded = DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    decoded.apply_orientation(orientation);
    Ok(Document {
        source: path,
        format,
        input: Arc::new(decoded.into_rgba8()),
        candidate: None,
        candidate_options: None,
        accepted_passes: 0,
    })
}

// Operations model a photographic rendering process, not a detector-specific
// guarantee. Re-running from the same input + options is deterministic.
fn process_pixels(source: &RgbaImage, options: ProcessingOptions) -> Result<RgbaImage, String> {
    if source.width() == 0 || source.height() == 0 {
        return Err("Empty image".into());
    }
    let mut output = source.clone();
    if options.framing.amount() > 0.0 {
        output = photographic_framing(&output, options.framing.amount());
    }
    if options.recapture.amount() > 0.0 {
        output = display_recapture(&output, options.recapture.amount());
    }
    if options.tone.amount() > 0.0 {
        output = tone_colour(&output, options.tone.amount());
    }
    if options.texture.amount() > 0.0 {
        output = photographic_texture(&output, options.texture.amount());
    }
    // Transparent RGB is undefined: consistently clear invisible colour data.
    for p in output.pixels_mut() {
        if p[3] == 0 { p.0 = [0, 0, 0, 0]; }
    }
    Ok(output)
}

fn tone_colour(source: &RgbaImage, amount: f32) -> RgbaImage {
    let s = amount.clamp(0.0, 1.0);
    if s <= 0.0 { return source.clone(); }
    let mut result = source.clone();
    for pixel in result.pixels_mut() {
        if pixel[3] == 0 { continue; }
        let r = pixel[0] as f32 / 255.0;
        let g = pixel[1] as f32 / 255.0;
        let b = pixel[2] as f32 / 255.0;
        let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        let saturation = 1.0 - 0.10 * s;
        for (c, channel_gain) in [1.0 - 0.09*s, 1.0 + 0.05*s, 1.0 + 0.14*s].iter().enumerate() {
            let v = [r, g, b][c];
            let mixed = luma + (v - luma) * saturation;
            let balanced = (mixed * channel_gain).clamp(0.0, 1.0);
            let curved = balanced.powf(1.0 - 0.16*s);
            let compressed = curved / (1.0 + 0.20*s*(curved - 0.62).max(0.0));
            let lifted = compressed * (1.0 - 0.010*s) + 0.010*s;
            pixel[c] = byte(lifted * 255.0);
        }
    }
    result
}

// Fixed coordinate noise enables reproducible tests. At higher intensities
// the model includes larger luminance/chroma noise, low amplitude correlated
// grain and restrained reconstruction sharpening.
fn photographic_texture(source: &RgbaImage, amount: f32) -> RgbaImage {
    let s = amount.clamp(0.0, 1.0);
    if s <= 0.0 { return source.clone(); }
    let blur = imageops::blur(source, 0.85);
    let guard = opaque_guard_map(source, 3);
    let mut result = source.clone();
    let (w, _) = source.dimensions();
    for (x, y, out) in result.enumerate_pixels_mut() {
        if !safe_at(&guard, x, y, w) || out[3] != 255 { continue; }
        let base = source.get_pixel(x, y);
        let smooth = blur.get_pixel(x, y);
        let lum = (0.2126 * base[0] as f32 + 0.7152 * base[1] as f32
            + 0.0722 * base[2] as f32) / 255.0;
        let sensitivity = 0.55 + 0.45 * (1.0 - (lum - 0.48).abs() * 1.8).max(0.0);
        let luma_noise = triangular_noise(x, y, 71) * (4.0*s + 15.0*s*s) * sensitivity;
        let block_noise = bilinear_noise(x as f32 / 3.0, y as f32 / 3.0, 113)
            * 3.0 * s;
        let row_noise = triangular_noise(0, y / 4, 157) * 0.7 * s;
        for c in 0..3 {
            let chroma_noise = triangular_noise(x, y, 193 + c as u32*17)
                * (1.0*s + 7.0*s*s);
            let edge = (base[c] as f32 - smooth[c] as f32) * (0.12*s + 0.30*s*s);
            out[c] = byte(base[c] as f32 + edge + luma_noise + block_noise
                + row_noise + chroma_noise);
        }
    }
    result
}

// A screen-to-camera analogue rather than simple blur: fractional display
// colour sampling, an optical low-pass stage, Bayer sampling/reconstruction,
// and low-amplitude pixel-grid interference. All are smoothly scaled by s.
fn display_recapture(source: &RgbaImage, amount: f32) -> RgbaImage {
    let s = amount.clamp(0.0, 1.0);
    let (w, h) = source.dimensions();
    if s <= 0.0 || w < 12 || h < 12 { return source.clone(); }

    let scale = 1.0 - 0.27 * s;
    let rw = ((w as f32 * scale).round() as u32).max(2);
    let rh = ((h as f32 * scale).round() as u32).max(2);
    let sampled = imageops::resize(source, rw, rh, imageops::FilterType::CatmullRom);
    let enlarged = imageops::resize(&sampled, w, h, imageops::FilterType::Triangle);
    let optical = imageops::blur(&enlarged, 0.23 + 0.48*s);

    let mut sensor = vec![0_u8; w as usize * h as usize];
    let shift = 0.42 * s;
    for y in 0..h {
        for x in 0..w {
            let channel = bayer_channel(x, y);
            let base = optical.get_pixel(x, y);
            let offset = match channel {
                0 => -shift, 1 => 0.0, _ => shift,
            };
            let shifted = sample_rgba_premultiplied(
                &optical, x as f32 + offset, y as f32 + shift*0.2
            );
            let raw = base[channel] as f32 * (1.0 - 0.65*s) + shifted[channel]*0.65*s;
            sensor[y as usize * w as usize + x as usize] = byte(raw);
        }
    }

    let guard = opaque_guard_map(source, 4);
    let mut result = source.clone();
    for (x, y, out) in result.enumerate_pixels_mut() {
        if out[3] != 255 || !safe_at(&guard, x, y, w) { continue; }
        let measured = bayer_reconstruct(&sensor, w, h, x, y);
        let base = source.get_pixel(x, y);
        let optical_px = optical.get_pixel(x, y);
        // A gentle interference pattern between virtual screen and sensor grids.
        let fx = x as f32;
        let fy = y as f32;
        let moire = (fx*0.233 + fy*0.037).sin()
            * (fx*0.247 - fy*0.028).sin() * 4.0*s*s;
        for c in 0..3 {
            let captured = measured[c] * 0.78 + optical_px[c] as f32 * 0.22;
            let mix = 0.35*s + 0.58*s*s;
            let fringe = if c == 0 { moire } else if c == 2 { -moire*0.7 } else { moire*0.2 };
            out[c] = byte(base[c] as f32 * (1.0 - mix) + captured * mix + fringe);
        }
    }
    result
}

// Amount controls both surrounding matte width and keystone distortion. The
// canvas dimensions stay unchanged; any transparent source is composited onto
// an opaque matte if framing is enabled.
fn photographic_framing(source: &RgbaImage, amount: f32) -> RgbaImage {
    let s = amount.clamp(0.0, 1.0);
    let (w, h) = source.dimensions();
    if s <= 0.0 || w < 32 || h < 32 { return source.clone(); }

    let edge = 0.004 + 0.035*s;
    let tl = (edge*0.85, edge*0.45);
    let tr = (1.0 - edge*0.50, edge*0.72);
    let bl = (edge*1.35 + 0.008*s, 1.0 - edge*0.38);
    let br = (1.0 - edge*0.65 - 0.006*s, 1.0 - edge*0.75);
    let mut result = RgbaImage::new(w, h);

    for (x, y, out) in result.enumerate_pixels_mut() {
        let px = (x as f32 + 0.5) / w as f32;
        let py = (y as f32 + 0.5) / h as f32;
        let mut u = px;
        let mut v = py;
        for _ in 0..3 {
            let lx = lerp(tl.0, bl.0, v);
            let rx = lerp(tr.0, br.0, v);
            u = (px - lx) / (rx - lx).max(0.001);
            let ty = lerp(tl.1, tr.1, u);
            let by = lerp(bl.1, br.1, u);
            v = (py - ty) / (by - ty).max(0.001);
        }
        let shade = (px - 0.5)*10.0 - (py - 0.5)*6.0;
        let noise = triangular_noise(x, y, 401)*2.5*s;
        let matte = [
            (222.0 + shade + noise).clamp(0.0, 255.0),
            (232.0 + shade + noise).clamp(0.0, 255.0),
            (242.0 + shade + noise).clamp(0.0, 255.0),
        ];
        if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
            out.0 = [byte(matte[0]), byte(matte[1]), byte(matte[2]), 255];
            continue;
        }
        let p = sample_rgba_premultiplied(source,
            u * (w - 1) as f32, v * (h - 1) as f32);
        let alpha = p[3] / 255.0;
        for c in 0..3 {
            out[c] = byte(p[c]*alpha + matte[c]*(1.0 - alpha));
        }
        out[3] = 255;
    }
    result
}

fn byte(v: f32) -> u8 { v.round().clamp(0.0, 255.0) as u8 }
fn lerp(a: f32, b: f32, t: f32) -> f32 { a + (b - a) * t }

fn fully_opaque(source: &RgbaImage) -> bool {
    source.pixels().all(|p| p[3] == 255)
}

// Premultiplication and an alpha-neighbourhood guard prevent colour bleed into
// transparent pixels during photographic texture and recapture operations.
fn opaque_guard_map(source: &RgbaImage, radius: u32) -> Option<Vec<bool>> {
    if fully_opaque(source) { return None; }
    let (w, h) = source.dimensions();
    let width = w as usize;
    let height = h as usize;
    let stride = width + 1;
    let mut summed = vec![0_u64; stride * (height + 1)];
    for y in 0..height {
        let mut running = 0_u64;
        for x in 0..width {
            if source.get_pixel(x as u32, y as u32)[3] != 255 { running += 1; }
            summed[(y + 1)*stride + (x + 1)] = summed[y*stride + (x + 1)] + running;
        }
    }
    let r = radius as usize;
    let mut guard = vec![false; width*height];
    for y in 0..height {
        for x in 0..width {
            let x0 = x.saturating_sub(r);
            let y0 = y.saturating_sub(r);
            let x1 = (x + r + 1).min(width);
            let y1 = (y + r + 1).min(height);
            let count = (summed[y1*stride + x1] - summed[y0*stride + x1])
                - (summed[y1*stride + x0] - summed[y0*stride + x0]);
            guard[y*width + x] = count == 0;
        }
    }
    Some(guard)
}

fn safe_at(mask: &Option<Vec<bool>>, x: u32, y: u32, w: u32) -> bool {
    mask.as_ref().is_none_or(|v| v[y as usize*w as usize + x as usize])
}

fn bayer_channel(x: u32, y: u32) -> usize {
    match (x & 1, y & 1) {
        (0, 0) => 0, (1, 1) => 2, _ => 1,
    }
}

fn sensor_at(sensor: &[u8], w: u32, h: u32, x: i32, y: i32) -> f32 {
    let ix = x.clamp(0, w as i32 - 1) as usize;
    let iy = y.clamp(0, h as i32 - 1) as usize;
    sensor[iy*w as usize + ix] as f32
}

fn bayer_reconstruct(sensor: &[u8], w: u32, h: u32, x: u32, y: u32) -> [f32; 3] {
    let x = x as i32;
    let y = y as i32;
    let m = |dx, dy| sensor_at(sensor, w, h, x + dx, y + dy);
    let horizontal = (m(-1, 0) + m(1, 0))*0.5;
    let vertical = (m(0, -1) + m(0, 1))*0.5;
    let diag = (m(-1, -1) + m(1, -1) + m(-1, 1) + m(1, 1))*0.25;
    match (x & 1, y & 1) {
        (0, 0) => [m(0, 0), (horizontal + vertical)*0.5, diag],
        (1, 1) => [diag, (horizontal + vertical)*0.5, m(0, 0)],
        (1, 0) => [horizontal, m(0, 0), vertical],
        _ => [vertical, m(0, 0), horizontal],
    }
}

fn hash_noise(x: u32, y: u32, stream: u32) -> f32 {
    let mut n = PROCESSING_SEED
        ^ x.wrapping_mul(0x9E37_79B9)
        ^ y.wrapping_mul(0x85EB_CA6B)
        ^ stream.wrapping_mul(0xC2B2_AE35);
    n ^= n >> 16;
    n = n.wrapping_mul(0x7FEB_352D);
    n ^= n >> 15;
    n = n.wrapping_mul(0x846C_A68B);
    n ^= n >> 16;
    (n as f64 / u32::MAX as f64) as f32
}

fn triangular_noise(x: u32, y: u32, stream: u32) -> f32 {
    hash_noise(x, y, stream) + hash_noise(x, y, stream.wrapping_add(257)) - 1.0
}

fn bilinear_noise(x: f32, y: f32, stream: u32) -> f32 {
    let x0 = x.floor().max(0.0) as u32;
    let y0 = y.floor().max(0.0) as u32;
    let tx = x - x.floor();
    let ty = y - y.floor();
    let a = lerp(triangular_noise(x0, y0, stream),
        triangular_noise(x0+1, y0, stream), tx);
    let b = lerp(triangular_noise(x0, y0+1, stream),
        triangular_noise(x0+1, y0+1, stream), tx);
    lerp(a, b, ty)
}

fn sample_rgba_premultiplied(source: &RgbaImage, x: f32, y: f32) -> [f32; 4] {
    let w = source.width();
    let h = source.height();
    let x = x.clamp(0.0, (w - 1) as f32);
    let y = y.clamp(0.0, (h - 1) as f32);
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(w - 1);
    let y1 = (y0 + 1).min(h - 1);
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let taps = [
        (x0, y0, (1.0-tx)*(1.0-ty)),
        (x1, y0, tx*(1.0-ty)),
        (x0, y1, (1.0-tx)*ty),
        (x1, y1, tx*ty),
    ];
    let mut p = [0.0_f32; 4];
    for (px, py, weight) in taps {
        let q = source.get_pixel(px, py);
        let alpha = q[3] as f32 / 255.0;
        for c in 0..3 { p[c] += q[c] as f32 * alpha * weight; }
        p[3] += q[3] as f32 * weight;
    }
    let alpha = p[3] / 255.0;
    if alpha > 1e-5 {
        for c in 0..3 { p[c] /= alpha; }
    }
    p
}

fn make_preview(ctx: &egui::Context, image: &RgbaImage, name: &str) -> egui::TextureHandle {
    let thumb = imageops::thumbnail(image, 1000, 850);
    let pixels = egui::ColorImage::from_rgba_unmultiplied(
        [thumb.width() as usize, thumb.height() as usize], thumb.as_raw());
    ctx.load_texture(name, pixels, egui::TextureOptions::LINEAR)
}

fn preferred_extension(path: &Path, format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Png => "png",
        ImageFormat::Bmp => "bmp",
        ImageFormat::Jpeg => {
            if path.extension().is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case("jpeg")) {
                "jpeg"
            } else { "jpg" }
        }
        _ => unreachable!("format verified on open"),
    }
}

fn format_name(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Png => "PNG",
        ImageFormat::Bmp => "BMP",
        ImageFormat::Jpeg => "JPEG",
        _ => "Unsupported",
    }
}

fn rgba_as_rgb(image: &RgbaImage) -> Vec<u8> {
    let mut rgb = Vec::with_capacity(image.width() as usize * image.height() as usize * 3);
    for p in image.pixels() {
        rgb.extend_from_slice(&p.0[..3]);
    }
    rgb
}

fn write_image(path: &Path, format: ImageFormat, image: &RgbaImage)
    -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    encode_image(&mut writer, format, image)?;
    writer.flush()?;
    Ok(())
}

fn encode_image<W: Write>(writer: &mut W, format: ImageFormat, image: &RgbaImage)
    -> image::ImageResult<()> {
    let (w, h) = image.dimensions();
    match format {
        ImageFormat::Png => PngEncoder::new(writer)
            .write_image(image.as_raw(), w, h, ExtendedColorType::Rgba8),
        ImageFormat::Bmp => BmpEncoder::new(writer)
            .write_image(image.as_raw(), w, h, ExtendedColorType::Rgba8),
        ImageFormat::Jpeg => {
            let rgb = rgba_as_rgb(image);
            JpegEncoder::new_with_quality(writer, OUTPUT_JPEG_QUALITY)
                .write_image(&rgb, w, h, ExtendedColorType::Rgb8)
        }
        _ => unreachable!("format verified on open"),
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1010.0, 780.0])
            .with_min_inner_size([850.0, 680.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native("CleanFrame v4", options, Box::new(|cc| {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        Ok(Box::new(CleanFrame::new()))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GenericImageView, Rgba};

    fn fixture() -> RgbaImage {
        let mut img = RgbaImage::new(100, 84);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = Rgba([
                ((x*37 + y*19)%256) as u8,
                ((x*13 + y*47)%256) as u8,
                ((x*29 + y*7)%256) as u8,
                255,
            ]);
        }
        img
    }
    fn off() -> ProcessingOptions {
        let zero = Effect { enabled: false, strength: 0.0 };
        ProcessingOptions { tone: zero, texture: zero, recapture: zero, framing: zero }
    }
    #[test]
    fn options_all_off_do_not_change_rgb() {
        let src = fixture();
        assert_eq!(src, process_pixels(&src, off()).unwrap());
    }
    #[test]
    fn zero_strength_is_no_op() {
        let src = fixture();
        let mut opts = off();
        opts.tone.enabled = true;
        opts.recapture.enabled = true;
        opts.framing.enabled = true;
        opts.texture.enabled = true;
        assert_eq!(src, process_pixels(&src, opts).unwrap());
    }
    #[test]
    fn each_effect_changes_image_and_preserves_size() {
        let src = fixture();
        for index in 0..4 {
            let mut opts = off();
            let effect = Effect { enabled: true, strength: 0.65 };
            match index {
                0 => opts.tone = effect,
                1 => opts.texture = effect,
                2 => opts.recapture = effect,
                _ => opts.framing = effect,
            }
            let out = process_pixels(&src, opts).unwrap();
            assert_eq!(src.dimensions(), out.dimensions());
            assert_ne!(src, out, "effect {index} should change image");
        }
    }
    #[test]
    fn processing_is_deterministic_and_cumulative() {
        let src = fixture();
        let options = ProcessingOptions::default();
        let a = process_pixels(&src, options).unwrap();
        let b = process_pixels(&src, options).unwrap();
        assert_eq!(a, b);
        let twice = process_pixels(&a, options).unwrap();
        assert_ne!(a, twice);
    }
    #[test]
    fn transparency_preserved_without_framing() {
        let mut src = fixture();
        src.put_pixel(4, 4, Rgba([120, 80, 40, 128]));
        src.put_pixel(4, 5, Rgba([10, 70, 40, 0]));
        let before: Vec<u8> = src.pixels().map(|p| p[3]).collect();
        let out = process_pixels(&src, ProcessingOptions::default()).unwrap();
        let after: Vec<u8> = out.pixels().map(|p| p[3]).collect();
        assert_eq!(before, after);
    }
    #[test]
    fn framing_intentionally_composites_alpha() {
        let mut src = fixture();
        src.put_pixel(4, 5, Rgba([10, 70, 40, 0]));
        let mut opts = off();
        opts.framing = Effect { enabled: true, strength: 0.50 };
        let out = process_pixels(&src, opts).unwrap();
        assert!(out.pixels().all(|p| p[3] == 255));
    }
    #[test]
    fn save_formats_have_original_dimensions() {
        let src = fixture();
        for format in [ImageFormat::Png, ImageFormat::Jpeg, ImageFormat::Bmp] {
            let mut encoded = Vec::new();
            encode_image(&mut encoded, format, &src).unwrap();
            assert_eq!(image::guess_format(&encoded).unwrap(), format);
            let decoded = image::load_from_memory_with_format(&encoded, format).unwrap();
            assert_eq!(decoded.dimensions(), src.dimensions());
        }
    }
    #[test]
    fn all_checkbox_combinations_keep_dimensions() {
        let src = fixture();
        let effect = Effect { enabled: true, strength: 0.4 };
        let disabled = Effect { enabled: false, strength: 0.4 };
        for flags in 0..16 {
            let options = ProcessingOptions {
                tone: if flags & 1 != 0 { effect } else { disabled },
                texture: if flags & 2 != 0 { effect } else { disabled },
                recapture: if flags & 4 != 0 { effect } else { disabled },
                framing: if flags & 8 != 0 { effect } else { disabled },
            };
            let output = process_pixels(&src, options).unwrap();
            assert_eq!(output.dimensions(), src.dimensions(), "flags {flags}");
        }
    }
    #[test]
    fn jpeg_encoder_does_not_copy_exif() {
        let src = fixture();
        let mut jpeg = Vec::new();
        encode_image(&mut jpeg, ImageFormat::Jpeg, &src).unwrap();
        let payload = b"Exif\0\0II\x2a\x00\x08\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";
        let length = (payload.len() + 2) as u16;
        let mut with_exif = vec![0xFF, 0xD8, 0xFF, 0xE1];
        with_exif.extend_from_slice(&length.to_be_bytes());
        with_exif.extend_from_slice(payload);
        with_exif.extend_from_slice(&jpeg[2..]);
        let decoded = image::load_from_memory_with_format(&with_exif, ImageFormat::Jpeg)
            .unwrap().into_rgba8();
        let mut cleaned = Vec::new();
        encode_image(&mut cleaned, ImageFormat::Jpeg, &decoded).unwrap();
        assert!(!cleaned.windows(6).any(|w| w == b"Exif\0\0"));
    }
    #[test]
    fn candidate_acceptance_semantics() {
        let opts = ProcessingOptions::default();
        let image = fixture();
        let result = process_pixels(&image, opts).unwrap();
        let mut doc = Document {
            source: PathBuf::from("example.png"),
            format: ImageFormat::Png,
            input: Arc::new(image.clone()),
            candidate: Some(Arc::new(result.clone())),
            candidate_options: Some(opts),
            accepted_passes: 0,
        };
        assert!(doc.candidate_is_current(opts));
        let mut changed = opts;
        changed.tone.strength = 0.9;
        assert!(!doc.candidate_is_current(changed));
        assert_eq!(&*doc.input, &image);
        doc.input = doc.candidate.take().unwrap();
        doc.candidate_options = None;
        doc.accepted_passes += 1;
        assert_eq!(&*doc.input, &result);
        assert!(doc.candidate.is_none());
    }
}
