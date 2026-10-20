use crate::engine::Favicon;
use base64::Engine;
use image::{ImageBuffer, Rgba};
use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::{Options, Tree};
use std::sync::LazyLock;

fn encode_png(img: &ImageBuffer<Rgba<u8>, Vec<u8>>) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut bytes);
    img.write_to(&mut cursor, image::ImageFormat::Png)
        .expect("PNG encoding 16x16 icon failed");
    bytes
}

static NEOWEB_PNG: LazyLock<Vec<u8>> = LazyLock::new(|| {
    let mut img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(16, 16);
    let center_x = 7.5f32;
    let center_y = 7.5f32;
    let radius = 7.0f32;

    for y in 0..16 {
        for x in 0..16 {
            let dx = x as f32 - center_x;
            let dy = y as f32 - center_y;
            let dist = (dx * dx + dy * dy).sqrt();

            if dist <= radius {
                // Background dark disk
                let mut pixel = Rgba([30, 30, 46, 255]); // #1e1e2e
                if dist > 6.0 {
                    // Outer ring accent
                    pixel = Rgba([137, 180, 250, 255]); // #89b4fa
                }

                // Draw bold letter "N"
                // Left bar: x = 4..=5, y = 4..=11
                // Right bar: x = 10..=11, y = 4..=11
                // Diagonal: connecting left top to right bottom
                let is_left_bar = (x == 4 || x == 5) && (4..=11).contains(&y);
                let is_right_bar = (x == 10 || x == 11) && (4..=11).contains(&y);
                let is_diagonal = if (4..=11).contains(&y) {
                    let progress = (y - 4) as f32 / 7.0;
                    let target_x = 5.0 + progress * 5.0;
                    (x as f32 - target_x).abs() < 1.0
                } else {
                    false
                };

                if is_left_bar || is_right_bar || is_diagonal {
                    pixel = Rgba([137, 180, 250, 255]); // Blue N
                }
                img.put_pixel(x, y, pixel);
            } else {
                img.put_pixel(x, y, Rgba([0, 0, 0, 0]));
            }
        }
    }
    encode_png(&img)
});

static BLANK_PNG: LazyLock<Vec<u8>> = LazyLock::new(|| {
    let mut img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(16, 16);

    for y in 0..16 {
        for x in 0..16 {
            // Document sheet: x in 3..=12, y in 2..=13
            if (3..=12).contains(&x) && (2..=13).contains(&y) {
                // Folded corner at top right: x in 9..=12, y in 2..=5
                if x >= 9 && y <= 5 && (x - 9) + (y - 2) >= 4 {
                    // Corner cut
                    img.put_pixel(x, y, Rgba([0, 0, 0, 0]));
                } else if x == 3 || x == 12 || y == 2 || y == 13 {
                    // Document border
                    img.put_pixel(x, y, Rgba([147, 153, 178, 255]));
                } else if (y == 7 || y == 9 || y == 11) && (5..=10).contains(&x) {
                    // Content lines
                    img.put_pixel(x, y, Rgba([147, 153, 178, 255]));
                } else {
                    // White document body
                    img.put_pixel(x, y, Rgba([205, 214, 244, 255]));
                }
            } else {
                img.put_pixel(x, y, Rgba([0, 0, 0, 0]));
            }
        }
    }
    encode_png(&img)
});

static HELP_PNG: LazyLock<Vec<u8>> = LazyLock::new(|| {
    let mut img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(16, 16);
    let center_x = 7.5f32;
    let center_y = 7.5f32;
    let radius = 7.0f32;

    for y in 0..16 {
        for x in 0..16 {
            let dx = x as f32 - center_x;
            let dy = y as f32 - center_y;
            let dist = (dx * dx + dy * dy).sqrt();

            if dist <= radius {
                let mut pixel = Rgba([180, 190, 254, 255]); // Lavender circle
                if dist > 6.0 {
                    pixel = Rgba([203, 166, 247, 255]); // Mauve border
                }

                // Question mark in dark color
                // Top arc: y=4, x in 6..=9; y=5, x=9; y=6, x in 7..=8; y=7, x in 7..=8
                // Dot: y in 10..=11, x in 7..=8
                let is_q_arc = (y == 4 && (6..=9).contains(&x))
                    || (y == 5 && (x == 6 || x == 9))
                    || (y == 6 && x == 8)
                    || (y == 7 && x == 7);
                let is_q_dot = (y == 9 || y == 10) && (x == 7);

                if is_q_arc || is_q_dot {
                    pixel = Rgba([30, 30, 46, 255]); // Dark mark
                }

                img.put_pixel(x, y, pixel);
            } else {
                img.put_pixel(x, y, Rgba([0, 0, 0, 0]));
            }
        }
    }
    encode_png(&img)
});

static GLOBE_PNG: LazyLock<Vec<u8>> = LazyLock::new(|| {
    let mut img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(16, 16);
    let center_x = 7.5f32;
    let center_y = 7.5f32;
    let radius = 7.0f32;

    for y in 0..16 {
        for x in 0..16 {
            let dx = x as f32 - center_x;
            let dy = y as f32 - center_y;
            let dist = (dx * dx + dy * dy).sqrt();

            if dist <= radius {
                let mut pixel = Rgba([30, 102, 245, 255]); // Sapphire ocean
                if dist > 6.0 {
                    pixel = Rgba([137, 180, 250, 255]); // Light ring
                }

                // Equator line: y = 7 or 8
                let is_equator = y == 7 || y == 8;
                // Prime meridian: x = 7 or 8
                let is_meridian = x == 7 || x == 8;
                // Longitude curves (curved arcs at x ~ 4 and x ~ 11)
                let is_arc = ((x == 4 || x == 11) && (4..=11).contains(&y))
                    || ((x == 5 || x == 10) && (y == 3 || y == 12));

                if is_equator || is_meridian || is_arc {
                    pixel = Rgba([180, 190, 254, 255]); // Light grid
                }

                img.put_pixel(x, y, pixel);
            } else {
                img.put_pixel(x, y, Rgba([0, 0, 0, 0]));
            }
        }
    }
    encode_png(&img)
});

pub fn neoweb_favicon() -> Favicon {
    Favicon {
        glyph: "🧭".to_string(),
        bytes: Some(NEOWEB_PNG.clone()),
        uri: Some("about:neoweb".to_string()),
        is_custom: true,
    }
}

pub fn blank_favicon() -> Favicon {
    Favicon {
        glyph: "📄".to_string(),
        bytes: Some(BLANK_PNG.clone()),
        uri: Some("about:blank".to_string()),
        is_custom: true,
    }
}

pub fn help_favicon() -> Favicon {
    Favicon {
        glyph: "📖".to_string(),
        bytes: Some(HELP_PNG.clone()),
        uri: Some("about:help".to_string()),
        is_custom: true,
    }
}

pub fn default_globe_favicon() -> Favicon {
    Favicon {
        glyph: "🌐".to_string(),
        bytes: Some(GLOBE_PNG.clone()),
        uri: None,
        is_custom: false,
    }
}

static SPINNER_PNGS: LazyLock<[Vec<u8>; 8]> = LazyLock::new(|| {
    let mut frames: [Vec<u8>; 8] = Default::default();
    let center_x = 7.5f32;
    let center_y = 7.5f32;
    let r_min = 4.0f32;
    let r_max = 6.8f32;

    for (k, frame_bytes) in frames.iter_mut().enumerate() {
        let mut img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(16, 16);
        let head_angle = (k as f32) * (std::f32::consts::PI / 4.0);
        let arc_span = 1.35 * std::f32::consts::PI; // ~240 degree sweep

        for y in 0..16 {
            for x in 0..16 {
                let dx = x as f32 - center_x;
                let dy = y as f32 - center_y;
                let dist = (dx * dx + dy * dy).sqrt();

                if dist >= r_min && dist <= r_max {
                    let angle = dy.atan2(dx);
                    let mut diff = head_angle - angle;
                    while diff < 0.0 {
                        diff += 2.0 * std::f32::consts::PI;
                    }
                    while diff >= 2.0 * std::f32::consts::PI {
                        diff -= 2.0 * std::f32::consts::PI;
                    }

                    if diff <= arc_span {
                        let factor = 1.0 - (diff / arc_span) * 0.75;
                        let alpha = (255.0 * factor) as u8;
                        img.put_pixel(x, y, Rgba([137, 180, 250, alpha]));
                    } else {
                        img.put_pixel(x, y, Rgba([0, 0, 0, 0]));
                    }
                } else {
                    img.put_pixel(x, y, Rgba([0, 0, 0, 0]));
                }
            }
        }
        *frame_bytes = encode_png(&img);
    }
    frames
});

const SPINNER_GLYPHS: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];

pub fn spinner_favicon(frame: usize) -> Favicon {
    let idx = frame % 8;
    Favicon {
        glyph: SPINNER_GLYPHS[idx].to_string(),
        bytes: Some(SPINNER_PNGS[idx].clone()),
        uri: None,
        is_custom: false,
    }
}

/// Converts a `servo::Image` raster buffer into a normalized 16x16 PNG `Favicon`.
pub fn favicon_from_servo_image(image: &servo::Image) -> Option<Favicon> {
    let width = image.width;
    let height = image.height;
    if width == 0 || height == 0 {
        return None;
    }
    let data = image.data();
    let rgba_img: image::RgbaImage = match image.format {
        servo::PixelFormat::RGBA8 => {
            image::ImageBuffer::from_raw(width, height, data.to_vec())?
        }
        servo::PixelFormat::BGRA8 => {
            let mut buf = Vec::with_capacity((width * height * 4) as usize);
            for chunk in data.chunks_exact(4) {
                buf.push(chunk[2]);
                buf.push(chunk[1]);
                buf.push(chunk[0]);
                buf.push(chunk[3]);
            }
            image::ImageBuffer::from_raw(width, height, buf)?
        }
        servo::PixelFormat::RGB8 => {
            let mut buf = Vec::with_capacity((width * height * 4) as usize);
            for chunk in data.chunks_exact(3) {
                buf.push(chunk[0]);
                buf.push(chunk[1]);
                buf.push(chunk[2]);
                buf.push(255);
            }
            image::ImageBuffer::from_raw(width, height, buf)?
        }
        _ => return None,
    };

    let mut png_bytes = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut png_bytes);
    let resized = if width != 16 || height != 16 {
        image::imageops::resize(&rgba_img, 16, 16, image::imageops::FilterType::Lanczos3)
    } else {
        rgba_img
    };
    resized
        .write_to(&mut cursor, image::ImageFormat::Png)
        .ok()?;

    Some(Favicon {
        glyph: "🌐".to_string(),
        bytes: Some(png_bytes),
        uri: None,
        is_custom: true,
    })
}

/// Render SVG bytes into a 16x16 PNG image using resvg
pub fn render_svg_to_png(
    svg_bytes: &[u8],
    target_width: u32,
    target_height: u32,
) -> Option<Vec<u8>> {
    let opt = Options::default();
    let tree = Tree::from_data(svg_bytes, &opt).ok()?;
    let mut pixmap = Pixmap::new(target_width, target_height)?;

    let svg_size = tree.size();
    let sx = target_width as f32 / svg_size.width();
    let sy = target_height as f32 / svg_size.height();
    let transform = Transform::from_scale(sx, sy);

    resvg::render(&tree, transform, &mut pixmap.as_mut());
    pixmap.encode_png().ok()
}

/// Processes raw favicon bytes in any format (ICO, PNG, SVG, WebP, JPEG, GIF, BMP)
/// and normalizes them into clean 16x16 PNG bytes.
pub fn process_favicon_bytes(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.is_empty() {
        return None;
    }

    // 1. Check if SVG (starts with <?xml or <svg or contains <svg)
    let sample_len = bytes.len().min(512);
    let sample = &bytes[..sample_len];
    let is_svg = sample.starts_with(b"<?xml")
        || sample.starts_with(b"<svg")
        || std::str::from_utf8(sample)
            .map(|s| s.contains("<svg"))
            .unwrap_or(false);

    if is_svg && let Some(png) = render_svg_to_png(bytes, 16, 16) {
        return Some(png);
    }

    // 2. Try loading as raster image (ICO, PNG, WebP, JPEG, GIF, BMP)
    if let Ok(img) = image::load_from_memory(bytes) {
        let mut png_bytes = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut png_bytes);
        if img
            .resize_exact(16, 16, image::imageops::FilterType::Lanczos3)
            .write_to(&mut cursor, image::ImageFormat::Png)
            .is_ok()
        {
            return Some(png_bytes);
        }
    }

    // 3. Fallback to SVG if not caught by sample check
    if !is_svg && let Some(png) = render_svg_to_png(bytes, 16, 16) {
        return Some(png);
    }

    None
}

/// Parses a `data:` URI (e.g. `data:image/svg+xml;base64,...` or `data:image/png;base64,...`)
/// and normalizes the image data to 16x16 PNG bytes.
pub fn parse_data_uri(uri: &str) -> Option<Vec<u8>> {
    if !uri.starts_with("data:") {
        return None;
    }
    let rest = &uri["data:".len()..];
    let (metadata, data) = rest.split_once(',')?;
    let is_base64 = metadata.contains(";base64");

    let raw_bytes = if is_base64 {
        let clean = data.trim().replace(['\r', '\n', ' '], "");
        base64::engine::general_purpose::STANDARD
            .decode(clean)
            .ok()?
    } else {
        let decoded = percent_encoding::percent_decode_str(data)
            .decode_utf8()
            .ok()?;
        decoded.as_bytes().to_vec()
    };

    process_favicon_bytes(&raw_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_favicons_have_valid_png_bytes() {
        for fav in [
            neoweb_favicon(),
            blank_favicon(),
            help_favicon(),
            default_globe_favicon(),
        ] {
            assert!(
                fav.bytes.is_some(),
                "Favicon must have image bytes: {:?}",
                fav.glyph
            );
            let bytes = fav.bytes.unwrap();
            assert!(!bytes.is_empty());
            let img =
                image::load_from_memory(&bytes).expect("Built-in favicon should decode cleanly");
            assert_eq!(img.width(), 16);
            assert_eq!(img.height(), 16);
        }
    }

    #[test]
    fn test_svg_rendering_to_16x16_png() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
            <circle cx="50" cy="50" r="40" fill="red" />
        </svg>"#;
        let png_bytes = render_svg_to_png(svg, 16, 16).expect("SVG rendering should produce PNG");
        let img = image::load_from_memory(&png_bytes).expect("Produced bytes must be valid PNG");
        assert_eq!(img.width(), 16);
        assert_eq!(img.height(), 16);
    }

    #[test]
    fn test_process_favicon_bytes_with_svg() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32">
            <rect width="32" height="32" fill="blue" />
        </svg>"#;
        let png = process_favicon_bytes(svg).expect("process_favicon_bytes should handle SVG");
        let img = image::load_from_memory(&png).expect("Valid PNG");
        assert_eq!(img.width(), 16);
        assert_eq!(img.height(), 16);
    }

    #[test]
    fn test_parse_data_uri_base64_svg() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="green"/></svg>"#;
        let b64 = base64::engine::general_purpose::STANDARD.encode(svg);
        let uri = format!("data:image/svg+xml;base64,{}", b64);

        let png = parse_data_uri(&uri).expect("Should parse base64 SVG data URI");
        let img = image::load_from_memory(&png).expect("Valid PNG");
        assert_eq!(img.width(), 16);
        assert_eq!(img.height(), 16);
    }

    #[test]
    fn test_parse_data_uri_percent_encoded_svg() {
        let uri = "data:image/svg+xml;utf8,%3Csvg%20xmlns%3D%22http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%22%20width%3D%2216%22%20height%3D%2216%22%3E%3Crect%20width%3D%2216%22%20height%3D%2216%22%20fill%3D%22yellow%22%2F%3E%3C%2Fsvg%3E";
        let png = parse_data_uri(uri).expect("Should parse percent-encoded SVG data URI");
        let img = image::load_from_memory(&png).expect("Valid PNG");
        assert_eq!(img.width(), 16);
        assert_eq!(img.height(), 16);
    }

    #[test]
    fn test_invalid_bytes_fail_gracefully() {
        assert!(process_favicon_bytes(b"not an image").is_none());
        assert!(process_favicon_bytes(b"").is_none());
        assert!(parse_data_uri("https://example.com/not-data-uri").is_none());
        assert!(parse_data_uri("data:image/png;base64,invalid-base64-###").is_none());
    }

    #[test]
    fn test_builtin_favicons_custom_flag() {
        assert!(neoweb_favicon().is_custom);
        assert!(blank_favicon().is_custom);
        assert!(help_favicon().is_custom);
        assert!(!default_globe_favicon().is_custom);
        assert!(!spinner_favicon(0).is_custom);
    }
}
