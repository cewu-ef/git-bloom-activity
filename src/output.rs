//! Image and animation encoding; callers own destinations and publication.
//!
//! Every export is [`EXPORT_SCALE`] times the native canvas on each side, so the pixel
//! art stays crisp. The GIF and PNG encoders and their exact-palette rules come from
//! git-koi (MIT, Anthony Yalong).

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use crate::render::{BloomFrame, CANVAS_PIXELS, FRAMES_PER_SECOND};

/// Each native pixel becomes this many export pixels on each side.
pub const EXPORT_SCALE: u32 = 2;
/// Width and height of every export.
pub const EXPORT_PIXELS: u32 = CANVAS_PIXELS * EXPORT_SCALE;

/// Encode a still PNG by repeating each native pixel, without interpolation.
pub fn encode_png(frame: &BloomFrame) -> Result<Vec<u8>, png::EncodingError> {
    encode_scaled_png(
        frame.as_rgba_bytes(),
        CANVAS_PIXELS,
        CANVAS_PIXELS,
        EXPORT_SCALE,
    )
}

/// Lay frames out in a grid on a solid background, each cell captioned by nothing but
/// its position, for render sheets in documentation.
pub fn encode_sheet_png(
    frames: &[BloomFrame],
    columns: u32,
    background: [u8; 4],
    scale: u32,
) -> Result<Vec<u8>, png::EncodingError> {
    let rows = (frames.len() as u32).div_ceil(columns);
    let (width, height) = (columns * CANVAS_PIXELS, rows * CANVAS_PIXELS);
    let mut sheet = vec![background; (width * height) as usize];
    for (index, frame) in frames.iter().enumerate() {
        let (left, top) = (
            index as u32 % columns * CANVAS_PIXELS,
            index as u32 / columns * CANVAS_PIXELS,
        );
        for y in 0..CANVAS_PIXELS {
            for x in 0..CANVAS_PIXELS {
                let pixel = frame.pixel(x, y);
                if pixel[3] != 0 {
                    sheet[((top + y) * width + left + x) as usize] = pixel;
                }
            }
        }
    }
    encode_scaled_png(sheet.as_flattened(), width, height, scale)
}

fn encode_scaled_png(
    rgba_bytes: &[u8],
    width: u32,
    height: u32,
    scale: u32,
) -> Result<Vec<u8>, png::EncodingError> {
    let native_pixels = rgba_bytes.as_chunks::<4>().0;
    let (export_width, export_height) = (width * scale, height * scale);
    let mut export_pixels = vec![[0; 4]; (export_width * export_height) as usize];
    for (index, pixel) in export_pixels.iter_mut().enumerate() {
        let native_x = index as u32 % export_width / scale;
        let native_y = index as u32 / export_width / scale;
        *pixel = native_pixels[(native_y * width + native_x) as usize];
    }
    let mut png_bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut png_bytes, export_width, export_height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(export_pixels.as_flattened())?;
    writer.finish()?;
    Ok(png_bytes)
}

/// Build a page that previews `bloom.gif` and `bloom.png`, read from beside it, on
/// GitHub's light and dark page colors under the given caption.
pub fn encode_preview_html(caption: &str) -> String {
    let caption = caption
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let panel = |class: &str, title: &str| {
        format!(
            "<section class=\"{class}\"><h2>{title}</h2><div class=\"images\">\
<figure><img src=\"bloom.gif\" width=\"{EXPORT_PIXELS}\" height=\"{EXPORT_PIXELS}\" alt=\"The flower, moving\"><figcaption>bloom.gif</figcaption></figure>\
<figure><img src=\"bloom.png\" width=\"{EXPORT_PIXELS}\" height=\"{EXPORT_PIXELS}\" alt=\"The flower, still\"><figcaption>bloom.png</figcaption></figure>\
</div></section>\n"
        )
    };
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
<title>git-bloom-activity preview</title>\n<style>\n\
body {{ margin: 0; font: 14px/1.5 -apple-system, BlinkMacSystemFont, \"Segoe UI\", Helvetica, Arial, sans-serif; }}\n\
h1, h2 {{ margin: 0; font-size: 14px; font-weight: 600; }}\n\
header, section {{ display: grid; gap: 12px; padding: 16px; }}\n\
.light {{ background: #ffffff; color: #1f2328; }}\n.dark {{ background: #0d1117; color: #f0f6fc; }}\n\
.images {{ display: flex; flex-wrap: wrap; gap: 16px; }}\nfigure {{ margin: 0; }}\n\
img {{ display: block; max-width: 100%; height: auto; image-rendering: pixelated; }}\n\
</style>\n</head>\n<body>\n<header class=\"light\"><h1>{caption}</h1></header>\n{}{}</body>\n</html>\n",
        panel("light", "GitHub light"),
        panel("dark", "GitHub dark"),
    )
}

/// One GIF palette holds 256 entries, and one of them marks transparent pixels.
const GIF_COLOR_LIMIT: usize = 255;
const GIF_TRANSPARENT_INDEX: u8 = 0;

/// Encode a seamless loop as an animated GIF at the static export size, repeating each
/// native pixel in both dimensions. The GIF repeats forever at `FRAMES_PER_SECOND`; each
/// delay, in hundredths of a second, is the step in a rounded running total, so a loop
/// lasts exactly its frame count divided by the frame rate. Each frame stores only the
/// box around its drawn pixels, with an exact palette of its colors, and that box clears
/// before the next frame. Colors are never approximated: a frame with more than 255
/// colors or a partly transparent pixel fails instead. Identical frames produce
/// identical bytes without I/O, timestamps, or input mutation.
pub fn encode_gif(frames: &[BloomFrame]) -> Result<Vec<u8>, GifEncodingError> {
    if frames.is_empty() {
        return Err(GifEncodingError::NoFrames);
    }
    let mut gif_encoder =
        gif::Encoder::new(Vec::new(), EXPORT_PIXELS as u16, EXPORT_PIXELS as u16, &[])?;
    gif_encoder.set_repeat(gif::Repeat::Infinite)?;
    for (frame_index, frame) in frames.iter().enumerate() {
        let native_pixels = frame.as_rgba_bytes().as_chunks::<4>().0;
        let [box_left, box_top, box_width, box_height] = find_drawn_box(native_pixels);
        let mut palette_indices: BTreeMap<[u8; 3], u8> = BTreeMap::new();
        let mut palette_channels = vec![0; 3];
        let mut native_indices = Vec::with_capacity((box_width * box_height) as usize);
        for native_y in box_top..box_top + box_height {
            for native_x in box_left..box_left + box_width {
                let [red, green, blue, alpha] =
                    native_pixels[(native_y * CANVAS_PIXELS + native_x) as usize];
                let palette_index = match alpha {
                    0 => GIF_TRANSPARENT_INDEX,
                    255 => match palette_indices.get(&[red, green, blue]) {
                        Some(&palette_index) => palette_index,
                        None if palette_indices.len() == GIF_COLOR_LIMIT => {
                            return Err(GifEncodingError::TooManyColors { frame_index });
                        }
                        None => {
                            let palette_index = palette_indices.len() as u8 + 1;
                            palette_indices.insert([red, green, blue], palette_index);
                            palette_channels.extend([red, green, blue]);
                            palette_index
                        }
                    },
                    _ => return Err(GifEncodingError::PartialTransparency { frame_index }),
                };
                native_indices.push(palette_index);
            }
        }
        let mut export_indices =
            Vec::with_capacity(native_indices.len() * (EXPORT_SCALE * EXPORT_SCALE) as usize);
        for native_row in native_indices.chunks(box_width as usize) {
            let export_row: Vec<u8> = native_row
                .iter()
                .flat_map(|&palette_index| [palette_index; EXPORT_SCALE as usize])
                .collect();
            for _ in 0..EXPORT_SCALE {
                export_indices.extend_from_slice(&export_row);
            }
        }
        gif_encoder.write_frame(&gif::Frame {
            delay: calculate_elapsed_hundredths(frame_index + 1)
                - calculate_elapsed_hundredths(frame_index),
            dispose: gif::DisposalMethod::Background,
            transparent: Some(GIF_TRANSPARENT_INDEX),
            left: (box_left * EXPORT_SCALE) as u16,
            top: (box_top * EXPORT_SCALE) as u16,
            width: (box_width * EXPORT_SCALE) as u16,
            height: (box_height * EXPORT_SCALE) as u16,
            palette: Some(palette_channels),
            buffer: Cow::Owned(export_indices),
            ..gif::Frame::default()
        })?;
    }
    Ok(gif_encoder.into_inner()?)
}

/// The native box `[left, top, width, height]` around a frame's drawn pixels; a frame
/// with none keeps one transparent pixel, since a GIF frame cannot be empty.
fn find_drawn_box(native_pixels: &[[u8; 4]]) -> [u32; 4] {
    let mut drawn_edges: Option<[u32; 4]> = None;
    for (pixel_index, pixel_channels) in native_pixels.iter().enumerate() {
        if pixel_channels[3] == 0 {
            continue;
        }
        let pixel_x = pixel_index as u32 % CANVAS_PIXELS;
        let pixel_y = pixel_index as u32 / CANVAS_PIXELS;
        let [left, top, right, bottom] =
            drawn_edges.get_or_insert([pixel_x, pixel_y, pixel_x, pixel_y]);
        *left = (*left).min(pixel_x);
        *top = (*top).min(pixel_y);
        *right = (*right).max(pixel_x);
        *bottom = (*bottom).max(pixel_y);
    }
    drawn_edges.map_or([0, 0, 1, 1], |[left, top, right, bottom]| {
        [left, top, right + 1 - left, bottom + 1 - top]
    })
}

/// Hundredths of a second from the start of the loop to the given frame, rounded.
fn calculate_elapsed_hundredths(frame_index: usize) -> u16 {
    let frames_per_second = FRAMES_PER_SECOND as usize;
    u16::try_from((frame_index * 200 + frames_per_second) / (2 * frames_per_second))
        .expect("loops last under ten minutes")
}

/// Why a GIF was not produced.
#[derive(Debug)]
pub enum GifEncodingError {
    /// No frames were given.
    NoFrames,
    /// A frame has more opaque colors than one GIF palette holds.
    TooManyColors {
        /// The frame, counted from zero.
        frame_index: usize,
    },
    /// A frame has a pixel that is neither opaque nor fully transparent.
    PartialTransparency {
        /// The frame, counted from zero.
        frame_index: usize,
    },
    /// The GIF encoder failed; carries its own error.
    Encoder(gif::EncodingError),
}

impl fmt::Display for GifEncodingError {
    fn fmt(&self, output_formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoFrames => write!(output_formatter, "a GIF needs at least one frame"),
            Self::TooManyColors { frame_index } => write!(
                output_formatter,
                "frame {frame_index} has more than {GIF_COLOR_LIMIT} colors, more than one GIF palette holds"
            ),
            Self::PartialTransparency { frame_index } => write!(
                output_formatter,
                "frame {frame_index} has partly transparent pixels, which a GIF cannot show"
            ),
            Self::Encoder(encoding_error) => {
                write!(output_formatter, "GIF encoding failed: {encoding_error}")
            }
        }
    }
}

impl Error for GifEncodingError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Encoder(encoding_error) => Some(encoding_error),
            _ => None,
        }
    }
}

impl From<gif::EncodingError> for GifEncodingError {
    fn from(encoding_error: gif::EncodingError) -> Self {
        Self::Encoder(encoding_error)
    }
}
