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
    imageops::{self},
    DynamicImage, ExtendedColorType, ImageDecoder, ImageEncoder, ImageFormat, ImageReader,
    RgbaImage,
};

// Fixed, conservative settings. The three checkboxes are the only processing options.
const FINE_DETAIL_REDUCTION: f32 = 0.23;
const MEDIUM_DETAIL_REDUCTION: f32 = 0.08;
const SUBPIXEL_X: f32 = 0.35;
const SUBPIXEL_Y: f32 = -0.28;
const RECONSTRUCTION_JPEG_QUALITY: u8 = 94;
const RECONSTRUCTION_BLEND: f32 = 0.85;
const OUTPUT_JPEG_QUALITY: u8 = 96;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ProcessingOptions {
    frequency: bool,
    subpixel: bool,
    compression: bool,
}

impl Default for ProcessingOptions {
    fn default() -> Self {
        Self {
            frequency: true,
            subpixel: true,
            // Compression can affect text and graphics more visibly.
            compression: false,
        }
    }
}

struct Document {
    source: PathBuf,
    format: ImageFormat,
    original: Arc<RgbaImage>,
    cleaned: Option<Arc<RgbaImage>>,
}

enum JobResult {
    Opened(Result<Document, String>),
    Processed(Result<RgbaImage, String>),
    Saved(Result<PathBuf, String>),
}

struct CleanFrame {
    document: Option<Document>,
    preview: Option<egui::TextureHandle>,
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
            preview: None,
            sender,
            receiver,
            options: ProcessingOptions::default(),
            busy: false,
            status: "Drop an image here, or click Open".into(),
        }
    }

    fn poll_jobs(&mut self, ctx: &egui::Context) {
        while let Ok(result) = self.receiver.try_recv() {
            self.busy = false;
            match result {
                JobResult::Opened(Ok(doc)) => {
                    self.preview = Some(make_preview(ctx, &doc.original));
                    self.document = Some(doc);
                    self.status = "Ready to process".into();
                }
                JobResult::Opened(Err(err)) => {
                    self.document = None;
                    self.preview = None;
                    self.status = format!("Cannot open image: {err}");
                }
                JobResult::Processed(Ok(image)) => {
                    self.preview = Some(make_preview(ctx, &image));
                    if let Some(doc) = &mut self.document {
                        doc.cleaned = Some(Arc::new(image));
                    }
                    self.status = "Processed — ready to save".into();
                }
                JobResult::Processed(Err(err)) => {
                    if let Some(doc) = &mut self.document {
                        doc.cleaned = None;
                        self.preview = Some(make_preview(ctx, &doc.original));
                    }
                    self.status = format!("Processing failed: {err}");
                }
                JobResult::Saved(Ok(path)) => {
                    let name = path.file_name().map(|s| s.to_string_lossy()).unwrap_or_default();
                    self.status = format!("Saved: {name}");
                }
                JobResult::Saved(Err(err)) => self.status = format!("Save failed: {err}"),
            }
        }
    }

    fn open_path(&mut self, path: PathBuf, ctx: &egui::Context) {
        if self.busy {
            return;
        }
        self.document = None;
        self.preview = None;
        self.status = "Opening image…".into();
        self.busy = true;
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let result = read_document(path);
            let _ = sender.send(JobResult::Opened(result));
            ctx.request_repaint();
        });
    }

    fn invalidate_processed(&mut self, ctx: &egui::Context) {
        if let Some(doc) = &mut self.document {
            doc.cleaned = None;
            self.preview = Some(make_preview(ctx, &doc.original));
            self.status = "Options changed — click Process".into();
        }
    }

    fn process(&mut self, ctx: &egui::Context) {
        if self.busy {
            return;
        }
        let Some(doc) = &mut self.document else { return; };
        let source = Arc::clone(&doc.original);
        let options = self.options;
        doc.cleaned = None;

        self.busy = true;
        self.status = "Processing image…".into();
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let image = process_pixels(&source, options);
            let _ = sender.send(JobResult::Processed(image));
            ctx.request_repaint();
        });
    }

    fn save(&mut self, ctx: &egui::Context) {
        if self.busy {
            return;
        }
        let Some(doc) = &self.document else { return; };
        let Some(image) = &doc.cleaned else { return; };
        let format = doc.format;
        let source = doc.source.clone();
        let image = Arc::clone(image);
        let extension = preferred_extension(&source, format);
        let stem = source.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
        let suggested = format!("{stem}_cleaned.{extension}");

        let dialog = rfd::FileDialog::new()
            .add_filter(format_name(format), &[extension])
            .set_file_name(suggested.as_str());
        let dialog = match source.parent() {
            Some(parent) => dialog.set_directory(parent),
            None => dialog,
        };
        let Some(mut output) = dialog.save_file() else { return; };

        // The bytes and extension must both match the input format.
        if !output.extension().is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case(extension)) {
            output.set_extension(extension);
        }

        // Do not let the output dialog overwrite the original input file.
        if let (Ok(a), Ok(b)) = (fs::canonicalize(&source), fs::canonicalize(&output)) {
            if a == b {
                self.status = "Choose another filename; original is protected".into();
                return;
            }
        }

        self.busy = true;
        self.status = "Saving image…".into();
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let result = write_image(&output, format, &image)
                .map(|()| output)
                .map_err(|e| e.to_string());
            let _ = sender.send(JobResult::Saved(result));
            ctx.request_repaint();
        });
    }
}

impl eframe::App for CleanFrame {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_jobs(ctx);

        // Accept one OS file drop anywhere in the window.
        let dropped = ctx.input(|i| i.raw.dropped_files.iter().find_map(|f| f.path.clone()));
        if let Some(path) = dropped {
            if !self.busy {
                self.open_path(path, ctx);
            } else {
                self.status = "Finish the current operation before opening another image".into();
            }
        }
        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(10.0);
            ui.vertical_centered(|ui| {
                ui.heading("CleanFrame");
                ui.label("PNG  ·  JPG  ·  BMP");
                ui.add_space(12.0);

                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.set_min_size(egui::vec2(495.0, 264.0));
                    ui.vertical_centered(|ui| {
                        ui.add_space(7.0);
                        if hovering {
                            ui.add_space(90.0);
                            ui.heading("Release to open image");
                        } else if let Some(texture) = &self.preview {
                            let natural = texture.size_vec2();
                            let scale = (470.0 / natural.x).min(221.0 / natural.y).min(1.0);
                            ui.add(egui::Image::new((texture.id(), natural * scale)));
                        } else {
                            ui.add_space(90.0);
                            ui.label("Drag and drop an image here");
                        }
                    });
                });

                ui.add_space(12.0);
                let mut changed = false;
                ui.vertical(|ui| {
                    ui.label("Processing steps");
                    ui.add_space(3.0);
                    changed |= ui.add_enabled(
                        !self.busy,
                        egui::Checkbox::new(&mut self.options.frequency, "Frequency-aware detail processing"),
                    ).changed();
                    changed |= ui.add_enabled(
                        !self.busy,
                        egui::Checkbox::new(&mut self.options.subpixel, "Subpixel resampling"),
                    ).changed();
                    changed |= ui.add_enabled(
                        !self.busy,
                        egui::Checkbox::new(&mut self.options.compression, "Controlled compression and reconstruction"),
                    ).changed();
                });
                if changed {
                    self.invalidate_processed(ctx);
                }

                ui.add_space(15.0);
                ui.horizontal(|ui| {
                    if ui.add_enabled(
                        !self.busy,
                        egui::Button::new("Open…").min_size(egui::vec2(150.0, 38.0)),
                    ).clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("Images", &["png", "jpg", "jpeg", "bmp"])
                            .pick_file()
                        {
                            self.open_path(path, ctx);
                        }
                    }

                    let can_process = !self.busy && self.document.is_some();
                    if ui.add_enabled(
                        can_process,
                        egui::Button::new("Process").min_size(egui::vec2(150.0, 38.0)),
                    ).clicked() {
                        self.process(ctx);
                    }

                    let can_save = !self.busy && self.document.as_ref().is_some_and(|doc| doc.cleaned.is_some());
                    if ui.add_enabled(
                        can_save,
                        egui::Button::new("Save…").min_size(egui::vec2(150.0, 38.0)),
                    ).clicked() {
                        self.save(ctx);
                    }
                });

                ui.add_space(12.0);
                if let Some(doc) = &self.document {
                    let name = doc.source.file_name().map(|s| s.to_string_lossy()).unwrap_or_default();
                    ui.small(format!("{name}  ·  {}", format_name(doc.format)));
                }
                ui.label(&self.status);
            });
        });
    }
}

fn read_document(path: PathBuf) -> Result<Document, String> {
    let reader = ImageReader::open(&path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let format = reader.format().ok_or("Unrecognised image format")?;
    if !matches!(format, ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::Bmp) {
        return Err("Only PNG, JPG/JPEG and BMP images are supported".into());
    }

    // Normalise orientation before dropping EXIF and any other metadata.
    let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    let orientation = decoder.orientation().map_err(|e| e.to_string())?;
    let mut decoded = DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    decoded.apply_orientation(orientation);

    Ok(Document {
        source: path,
        format,
        original: Arc::new(decoded.into_rgba8()),
        cleaned: None,
    })
}

fn process_pixels(source: &RgbaImage, options: ProcessingOptions) -> Result<RgbaImage, String> {
    // Every step works on fresh pixels; the original is never modified.
    let mut output = source.clone();
    if options.frequency {
        output = frequency_aware_detail(&output);
    }
    if options.subpixel {
        output = subpixel_resampling(&output);
    }
    if options.compression {
        output = compression_reconstruction(&output)?;
    }

    // Remove hidden RGB values in completely transparent pixels.
    for pixel in output.pixels_mut() {
        if pixel[3] == 0 {
            pixel.0 = [0, 0, 0, 0];
        }
    }
    Ok(output)
}

fn fully_opaque(image: &RgbaImage) -> bool {
    image.pixels().all(|p| p[3] == 255)
}

// For alpha images, build a summed-area table of non-opaque pixels. This
// creates a constant-time guard map for each output pixel, avoiding expensive
// per-pixel neighbourhood scans on large images. None means fully opaque.
fn opaque_guard_map(image: &RgbaImage, radius: u32) -> Option<Vec<bool>> {
    if fully_opaque(image) {
        return None;
    }
    let (w, h) = image.dimensions();
    let width = w as usize;
    let height = h as usize;
    let stride = width + 1;
    let mut summed = vec![0_u64; (width + 1) * (height + 1)];
    for y in 0..height {
        let mut row_sum = 0_u64;
        for x in 0..width {
            if image.get_pixel(x as u32, y as u32)[3] < 255 {
                row_sum += 1;
            }
            summed[(y + 1) * stride + (x + 1)] = summed[y * stride + (x + 1)] + row_sum;
        }
    }
    let mut safe = vec![false; width * height];
    let radius = radius as usize;
    for y in 0..height {
        for x in 0..width {
            let x0 = x.saturating_sub(radius);
            let y0 = y.saturating_sub(radius);
            let x1 = (x + radius + 1).min(width);
            let y1 = (y + radius + 1).min(height);
            let count = (summed[y1 * stride + x1] - summed[y0 * stride + x1])
                - (summed[y1 * stride + x0] - summed[y0 * stride + x0]);
            safe[y * width + x] = count == 0;
        }
    }
    Some(safe)
}

fn safe_at(mask: &Option<Vec<bool>>, x: u32, y: u32, width: u32) -> bool {
    mask.as_ref().is_none_or(|v| v[y as usize * width as usize + x as usize])
}

// Two Gaussian scales split an image into broad tones, medium detail and microdetail.
// Reduce the higher-frequency components slightly; protect prominent edges more.
fn frequency_aware_detail(source: &RgbaImage) -> RgbaImage {
    let (width, height) = source.dimensions();
    if width < 5 || height < 5 {
        return source.clone();
    }
    let soft = imageops::blur(source, 0.8);
    let wide = imageops::blur(source, 2.1);
    let alpha_guard = opaque_guard_map(source, 7);
    let mut result = source.clone();

    for (x, y, pixel) in result.enumerate_pixels_mut() {
        let original = source.get_pixel(x, y);
        if original[3] != 255 || !safe_at(&alpha_guard, x, y, width) {
            continue;
        }
        let s = soft.get_pixel(x, y);
        let w = wide.get_pixel(x, y);
        let edge_strength = (0..3)
            .map(|c| (original[c] as f32 - w[c] as f32).abs())
            .fold(0.0_f32, f32::max);
        let edge_protection = (1.0 - edge_strength / 36.0).clamp(0.12, 1.0);
        let high_keep = 1.0 - FINE_DETAIL_REDUCTION * edge_protection;
        let medium_keep = 1.0 - MEDIUM_DETAIL_REDUCTION * edge_protection;
        for c in 0..3 {
            let base = original[c] as f32;
            let a = s[c] as f32;
            let b = w[c] as f32;
            pixel[c] = (b + (a - b) * medium_keep + (base - a) * high_keep)
                .round()
                .clamp(0.0, 255.0) as u8;
        }
    }
    result
}

// Fractional forward/backward translations change interpolation correlations
// without permanently translating the image or changing its dimensions.
fn subpixel_resampling(source: &RgbaImage) -> RgbaImage {
    if source.width() < 6 || source.height() < 6 {
        return source.clone();
    }
    let first = fractional_warp(source, SUBPIXEL_X, SUBPIXEL_Y);
    fractional_warp(&first, -SUBPIXEL_X, -SUBPIXEL_Y)
}

fn fractional_warp(source: &RgbaImage, dx: f32, dy: f32) -> RgbaImage {
    let (width, height) = source.dimensions();
    let mut result = source.clone();
    for (x, y, output) in result.enumerate_pixels_mut() {
        if x < 2 || y < 2 || x >= width - 2 || y >= height - 2 || output[3] != 255 {
            continue;
        }
        let sx = x as f32 + dx;
        let sy = y as f32 + dy;
        let x0 = sx.floor() as u32;
        let y0 = sy.floor() as u32;
        let x1 = x0 + 1;
        let y1 = y0 + 1;
        let neighbours = [
            source.get_pixel(x0, y0),
            source.get_pixel(x1, y0),
            source.get_pixel(x0, y1),
            source.get_pixel(x1, y1),
        ];
        if neighbours.iter().any(|p| p[3] != 255) {
            continue;
        }
        let wx = sx - x0 as f32;
        let wy = sy - y0 as f32;
        for c in 0..3 {
            let top = neighbours[0][c] as f32 * (1.0 - wx) + neighbours[1][c] as f32 * wx;
            let bottom = neighbours[2][c] as f32 * (1.0 - wx) + neighbours[3][c] as f32 * wx;
            output[c] = (top * (1.0 - wy) + bottom * wy).round().clamp(0.0, 255.0) as u8;
        }
    }
    result
}

// A JPEG encode/decode cycle is optional even for PNG/BMP sources. The final
// save step still uses the original *file format*. Avoid affecting alpha edges.
fn compression_reconstruction(source: &RgbaImage) -> Result<RgbaImage, String> {
    let (width, height) = source.dimensions();
    let rgb = rgba_as_rgb(source);
    let mut compressed = Vec::new();
    JpegEncoder::new_with_quality(&mut compressed, RECONSTRUCTION_JPEG_QUALITY)
        .write_image(&rgb, width, height, ExtendedColorType::Rgb8)
        .map_err(|e| format!("JPEG reconstruction encode: {e}"))?;
    let decoded = image::load_from_memory_with_format(&compressed, ImageFormat::Jpeg)
        .map_err(|e| format!("JPEG reconstruction decode: {e}"))?
        .into_rgb8();

    let alpha_guard = opaque_guard_map(source, 9);
    let mut result = source.clone();
    for (x, y, pixel) in result.enumerate_pixels_mut() {
        if pixel[3] != 255 || !safe_at(&alpha_guard, x, y, width) {
            continue;
        }
        let reconstructed = decoded.get_pixel(x, y);
        for c in 0..3 {
            pixel[c] = (source.get_pixel(x, y)[c] as f32 * (1.0 - RECONSTRUCTION_BLEND)
                + reconstructed[c] as f32 * RECONSTRUCTION_BLEND)
                .round()
                .clamp(0.0, 255.0) as u8;
        }
    }
    Ok(result)
}

fn rgba_as_rgb(image: &RgbaImage) -> Vec<u8> {
    let mut rgb = Vec::with_capacity(image.width() as usize * image.height() as usize * 3);
    for pixel in image.pixels() {
        rgb.extend_from_slice(&pixel.0[..3]);
    }
    rgb
}

fn make_preview(ctx: &egui::Context, image: &RgbaImage) -> egui::TextureHandle {
    let thumbnail = imageops::thumbnail(image, 940, 442);
    let pixels = egui::ColorImage::from_rgba_unmultiplied(
        [thumbnail.width() as usize, thumbnail.height() as usize],
        thumbnail.as_raw(),
    );
    ctx.load_texture("image-preview", pixels, egui::TextureOptions::LINEAR)
}

fn preferred_extension(path: &Path, format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Png => "png",
        ImageFormat::Bmp => "bmp",
        ImageFormat::Jpeg => {
            if path.extension().is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case("jpeg")) {
                "jpeg"
            } else {
                "jpg"
            }
        }
        _ => unreachable!("format verified on open"),
    }
}

fn format_name(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Png => "PNG",
        ImageFormat::Jpeg => "JPEG",
        ImageFormat::Bmp => "BMP",
        _ => "Unsupported",
    }
}

fn write_image(path: &Path, format: ImageFormat, image: &RgbaImage) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    encode_image(&mut writer, format, image)?;
    writer.flush()?;
    Ok(())
}

fn encode_image<W: Write>(writer: &mut W, format: ImageFormat, image: &RgbaImage) -> image::ImageResult<()> {
    let (width, height) = image.dimensions();
    match format {
        ImageFormat::Png => PngEncoder::new(writer)
            .write_image(image.as_raw(), width, height, ExtendedColorType::Rgba8),
        ImageFormat::Bmp => BmpEncoder::new(writer)
            .write_image(image.as_raw(), width, height, ExtendedColorType::Rgba8),
        ImageFormat::Jpeg => {
            let rgb = rgba_as_rgb(image);
            JpegEncoder::new_with_quality(writer, OUTPUT_JPEG_QUALITY)
                .write_image(&rgb, width, height, ExtendedColorType::Rgb8)
        }
        _ => unreachable!("format verified on open"),
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([550.0, 630.0])
            .with_min_inner_size([540.0, 595.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "CleanFrame",
        options,
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(CleanFrame::new()))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GenericImageView, Rgba};

    fn test_image() -> RgbaImage {
        let mut image = RgbaImage::new(96, 80);
        for (x, y, p) in image.enumerate_pixels_mut() {
            *p = Rgba([
                ((x * 37 + y * 19) % 256) as u8,
                ((x * 13 + y * 47) % 256) as u8,
                ((x * 29 + y * 7) % 256) as u8,
                255,
            ]);
        }
        image
    }

    #[test]
    fn the_three_steps_are_independent() {
        let input = test_image();
        let none = process_pixels(&input, ProcessingOptions { frequency: false, subpixel: false, compression: false }).unwrap();
        assert_eq!(input, none);
        for choice in [
            ProcessingOptions { frequency: true, subpixel: false, compression: false },
            ProcessingOptions { frequency: false, subpixel: true, compression: false },
            ProcessingOptions { frequency: false, subpixel: false, compression: true },
        ] {
            let output = process_pixels(&input, choice).unwrap();
            assert_eq!(output.dimensions(), input.dimensions());
            assert_ne!(output, input, "selected step should change this textured sample");
        }
    }

    #[test]
    fn dimensions_and_alpha_are_preserved() {
        let mut image = test_image();
        image.put_pixel(7, 10, Rgba([200, 100, 50, 128]));
        image.put_pixel(8, 10, Rgba([200, 100, 50, 0]));
        let original_alphas: Vec<_> = image.pixels().map(|p| p[3]).collect();
        let cleaned = process_pixels(&image, ProcessingOptions { frequency: true, subpixel: true, compression: true }).unwrap();
        assert_eq!(cleaned.dimensions(), image.dimensions());
        assert_eq!(cleaned.pixels().map(|p| p[3]).collect::<Vec<_>>(), original_alphas);
        assert_eq!(cleaned.get_pixel(7, 10).0, [200, 100, 50, 128]);
        assert_eq!(cleaned.get_pixel(8, 10).0, [0, 0, 0, 0]);
    }

    #[test]
    fn fresh_encoding_keeps_original_format() {
        let image = test_image();
        for format in [ImageFormat::Png, ImageFormat::Bmp, ImageFormat::Jpeg] {
            let mut bytes = Vec::new();
            encode_image(&mut bytes, format, &image).unwrap();
            assert_eq!(image::guess_format(&bytes).unwrap(), format);
            assert_eq!(image::load_from_memory_with_format(&bytes, format).unwrap().dimensions(), (96, 80));
        }
    }

    #[test]
    fn none_selected_still_removes_hidden_transparent_rgb() {
        let mut image = RgbaImage::from_pixel(6, 7, Rgba([20, 40, 60, 255]));
        image.put_pixel(2, 4, Rgba([13, 87, 244, 0]));
        let cleaned = process_pixels(&image, ProcessingOptions { frequency: false, subpixel: false, compression: false }).unwrap();
        assert_eq!(cleaned.get_pixel(2, 4).0, [0, 0, 0, 0]);
    }
}
