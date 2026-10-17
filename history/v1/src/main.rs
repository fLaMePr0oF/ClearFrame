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
    imageops::{self, FilterType},
    DynamicImage, ExtendedColorType, ImageDecoder, ImageEncoder, ImageFormat, ImageReader,
    RgbaImage,
};
use rand::{rngs::SmallRng, Rng, SeedableRng};

// Processing parameters are intentionally conservative and fixed: no settings panel.
const RESAMPLE_SCALE: f32 = 0.985;
const RESAMPLE_BLEND: f32 = 0.26;
const LUMA_GRAIN: f32 = 1.25;
const CHROMA_GRAIN: f32 = 0.45;
const JPEG_QUALITY: u8 = 96;

struct Document {
    source: PathBuf,
    format: ImageFormat,
    original: Option<RgbaImage>,
    cleaned: Option<Arc<RgbaImage>>,
}

enum JobResult {
    Opened(Result<Document, String>),
    Processed(RgbaImage),
    Saved(Result<PathBuf, String>),
}

struct CleanFrame {
    document: Option<Document>,
    preview: Option<egui::TextureHandle>,
    sender: mpsc::Sender<JobResult>,
    receiver: mpsc::Receiver<JobResult>,
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
            busy: false,
            status: "Drop an image here, or click Open".into(),
        }
    }

    fn poll_jobs(&mut self, ctx: &egui::Context) {
        while let Ok(result) = self.receiver.try_recv() {
            self.busy = false;
            match result {
                JobResult::Opened(Ok(doc)) => {
                    self.preview = doc.original.as_ref().map(|img| make_preview(ctx, img));
                    self.status = "Ready to process".into();
                    self.document = Some(doc);
                }
                JobResult::Opened(Err(err)) => {
                    self.document = None;
                    self.preview = None;
                    self.status = format!("Cannot open image: {err}");
                }
                JobResult::Processed(image) => {
                    self.preview = Some(make_preview(ctx, &image));
                    if let Some(doc) = self.document.as_mut() {
                        doc.cleaned = Some(Arc::new(image));
                    }
                    self.status = "Processed — ready to save".into();
                }
                JobResult::Saved(Ok(path)) => {
                    let name = path.file_name().map(|s| s.to_string_lossy()).unwrap_or_default();
                    self.status = format!("Saved: {name}");
                }
                JobResult::Saved(Err(err)) => {
                    self.status = format!("Save failed: {err}");
                }
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

    fn process(&mut self, ctx: &egui::Context) {
        if self.busy {
            return;
        }
        let Some(doc) = self.document.as_mut() else { return; };
        let Some(source) = doc.original.take() else { return; };

        self.busy = true;
        self.status = "Processing image…".into();
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let image = process_pixels(source);
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
            Some(directory) => dialog.set_directory(directory),
            None => dialog,
        };
        let Some(mut output) = dialog.save_file() else { return; };

        // Keep the original image *format* even if the user types another suffix.
        let correct_extension = output
            .extension()
            .is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case(extension));
        if !correct_extension {
            output.set_extension(extension);
        }

        // Never silently overwrite the source photograph.
        if let (Ok(a), Ok(b)) = (fs::canonicalize(&source), fs::canonicalize(&output)) {
            if a == b {
                self.status = "Choose a different filename; the original is protected".into();
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

        // OS file drops can occur anywhere in the app window.
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
            ui.add_space(15.0);
            ui.vertical_centered(|ui| {
                ui.heading("CleanFrame");
                ui.add_space(5.0);
                ui.label("PNG  ·  JPG  ·  BMP");
                ui.add_space(20.0);

                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.set_min_size(egui::vec2(470.0, 295.0));
                    ui.vertical_centered(|ui| {
                        ui.add_space(9.0);
                        if hovering {
                            ui.add_space(99.0);
                            ui.heading("Release to open image");
                        } else if let Some(texture) = &self.preview {
                            let natural = texture.size_vec2();
                            let scale = (435.0 / natural.x).min(245.0 / natural.y).min(1.0);
                            ui.add(egui::Image::new((texture.id(), natural * scale)));
                        } else {
                            ui.add_space(99.0);
                            ui.label("Drag and drop an image here");
                        }
                    });
                });

                ui.add_space(15.0);
                ui.horizontal(|ui| {
                    let open = ui.add_enabled(
                        !self.busy,
                        egui::Button::new("Open…").min_size(egui::vec2(145.0, 36.0)),
                    );
                    if open.clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("Images", &["png", "jpg", "jpeg", "bmp"])
                            .pick_file()
                        {
                            self.open_path(path, ctx);
                        }
                    }

                    let can_process = !self.busy
                        && self.document.as_ref().is_some_and(|doc| doc.original.is_some());
                    let process = ui.add_enabled(
                        can_process,
                        egui::Button::new("Process").min_size(egui::vec2(145.0, 36.0)),
                    );
                    if process.clicked() {
                        self.process(ctx);
                    }

                    let can_save = !self.busy
                        && self.document.as_ref().is_some_and(|doc| doc.cleaned.is_some());
                    let save = ui.add_enabled(
                        can_save,
                        egui::Button::new("Save…").min_size(egui::vec2(145.0, 36.0)),
                    );
                    if save.clicked() {
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

    // Applying EXIF orientation *before* discarding EXIF avoids rotated JPGs.
    let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    let orientation = decoder.orientation().map_err(|e| e.to_string())?;
    let mut decoded = DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    decoded.apply_orientation(orientation);

    Ok(Document {
        source: path,
        format,
        original: Some(decoded.into_rgba8()),
        cleaned: None,
    })
}

fn process_pixels(source: RgbaImage) -> RgbaImage {
    let (width, height) = source.dimensions();
    let opaque = source.pixels().all(|pixel| pixel[3] == 255);

    // A very light round-trip resample affects fine pixel correlations without
    // changing the final dimensions. Don't resample images with transparency:
    // interpolation of straight-alpha RGB can introduce visible edge halos.
    let resampled = if opaque && width >= 64 && height >= 64 {
        let small_width = ((width as f32 * RESAMPLE_SCALE).round() as u32).max(1);
        let small_height = ((height as f32 * RESAMPLE_SCALE).round() as u32).max(1);
        if small_width != width || small_height != height {
            let smaller = imageops::resize(&source, small_width, small_height, FilterType::Lanczos3);
            Some(imageops::resize(&smaller, width, height, FilterType::Lanczos3))
        } else {
            None
        }
    } else {
        None
    };

    let mut output = source.clone();
    let mut rng = SmallRng::from_entropy();

    for (index, (before, after)) in source.pixels().zip(output.pixels_mut()).enumerate() {
        let alpha = before[3];
        if alpha == 0 {
            // Remove invisible RGB payloads, including anything in transparent pixels.
            after.0 = [0, 0, 0, 0];
            continue;
        }
        if alpha < 255 {
            // Keep antialiased edges, compositing behaviour and alpha unchanged.
            continue;
        }

        // Triangular, very low-amplitude shared luminance grain plus a smaller
        // independent colour component. Values are in 8-bit channel levels.
        let luminance = (rng.gen::<f32>() + rng.gen::<f32>() - 1.0) * LUMA_GRAIN;
        let smooth = resampled.as_ref().map(|image| &image.as_raw()[index * 4..index * 4 + 3]);

        for channel in 0..3 {
            let neighbour = smooth.map_or(before[channel] as f32, |px| px[channel] as f32);
            let base = before[channel] as f32;
            let blend = if smooth.is_some() { RESAMPLE_BLEND } else { 0.0 };
            let chroma = (rng.gen::<f32>() - 0.5) * 2.0 * CHROMA_GRAIN;
            after[channel] = (base * (1.0 - blend) + neighbour * blend + luminance + chroma)
                .round()
                .clamp(0.0, 255.0) as u8;
        }
        // No alpha mutation.
    }

    output
}

fn make_preview(ctx: &egui::Context, image: &RgbaImage) -> egui::TextureHandle {
    let thumbnail = imageops::thumbnail(image, 870, 490);
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
        _ => unreachable!("format checked when opening"),
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
            // JPEG does not support alpha. Inputs decoded from JPEG are opaque.
            let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
            for pixel in image.pixels() {
                rgb.extend_from_slice(&pixel.0[..3]);
            }
            JpegEncoder::new_with_quality(writer, JPEG_QUALITY)
                .write_image(&rgb, width, height, ExtendedColorType::Rgb8)
        }
        _ => unreachable!("format checked when opening"),
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([550.0, 540.0])
            .with_min_inner_size([520.0, 500.0])
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

    #[test]
    fn keeps_dimensions_and_alpha() {
        let mut image = RgbaImage::from_pixel(100, 70, Rgba([100, 145, 160, 255]));
        image.put_pixel(7, 10, Rgba([200, 100, 50, 128]));
        image.put_pixel(8, 10, Rgba([200, 100, 50, 0]));
        let cleaned = process_pixels(image);
        assert_eq!(cleaned.dimensions(), (100, 70));
        assert_eq!(cleaned.get_pixel(7, 10).0, [200, 100, 50, 128]);
        assert_eq!(cleaned.get_pixel(8, 10).0, [0, 0, 0, 0]);
    }

    #[test]
    fn saves_in_original_encoding() {
        let image = RgbaImage::from_pixel(20, 10, Rgba([210, 150, 80, 255]));
        for format in [ImageFormat::Png, ImageFormat::Bmp, ImageFormat::Jpeg] {
            let mut bytes = Vec::new();
            encode_image(&mut bytes, format, &image).unwrap();
            assert_eq!(image::guess_format(&bytes).unwrap(), format);
            assert_eq!(image::load_from_memory_with_format(&bytes, format).unwrap().dimensions(), (20, 10));
        }
    }
}
