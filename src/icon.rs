use crate::state::Status;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::Write;

// Colors (RGBA)
const COLOR_ACTIVE: [u8; 4] = [0x32, 0xD7, 0x4B, 0xFF]; // #32D74B
const COLOR_PENDING: [u8; 4] = [0xFF, 0x9F, 0x0A, 0xFF]; // #FF9F0A
const COLOR_IDLE: [u8; 4] = [0x8E, 0x8E, 0x93, 0xFF]; // #8E8E93

// Layout params (@2x retina)
const DOT_DIAMETER: u32 = 10;
const DOT_SPACING: u32 = 4;
const PADDING: u32 = 3;
const MAX_COLS: u32 = 3;

fn status_color(s: Status) -> [u8; 4] {
    match s {
        Status::Active => COLOR_ACTIVE,
        Status::Pending => COLOR_PENDING,
        Status::Idle => COLOR_IDLE,
    }
}

/// Calculate grid dimensions for N dots.
fn grid_dims(n: u32) -> (u32, u32) {
    if n == 0 {
        return (0, 0);
    }
    let cols = n.min(MAX_COLS);
    let rows = (n + MAX_COLS - 1) / MAX_COLS;
    (cols, rows)
}

/// Calculate image dimensions in pixels for N dots.
pub fn image_dims(n: u32) -> (u32, u32) {
    if n == 0 {
        return (0, 0);
    }
    let (cols, rows) = grid_dims(n);
    let w = 2 * PADDING + cols * DOT_DIAMETER + (cols - 1) * DOT_SPACING;
    let h = 2 * PADDING + rows * DOT_DIAMETER + (rows - 1) * DOT_SPACING;
    (w, h)
}

/// Generate a PNG dot grid for the given statuses.
/// Returns raw PNG bytes. Empty if no statuses.
pub fn make_dot_grid_png(statuses: &[Status]) -> Vec<u8> {
    let n = statuses.len() as u32;
    if n == 0 {
        return Vec::new();
    }

    let (width, height) = image_dims(n);
    let (cols, _rows) = grid_dims(n);

    // Build RGBA pixel buffer
    let mut pixels = vec![0u8; (width * height * 4) as usize];

    for (i, &status) in statuses.iter().enumerate() {
        let col = i as u32 % cols;
        let row = i as u32 / cols;
        let cx = PADDING + col * (DOT_DIAMETER + DOT_SPACING) + DOT_DIAMETER / 2;
        let cy = PADDING + row * (DOT_DIAMETER + DOT_SPACING) + DOT_DIAMETER / 2;
        let color = status_color(status);
        let r = DOT_DIAMETER as f32 / 2.0;

        // Draw filled circle
        let x_start = cx.saturating_sub(DOT_DIAMETER / 2);
        let x_end = (cx + DOT_DIAMETER / 2).min(width);
        let y_start = cy.saturating_sub(DOT_DIAMETER / 2);
        let y_end = (cy + DOT_DIAMETER / 2).min(height);

        for py in y_start..y_end {
            for px in x_start..x_end {
                let dx = px as f32 - cx as f32 + 0.5;
                let dy = py as f32 - cy as f32 + 0.5;
                if dx * dx + dy * dy <= r * r {
                    let offset = ((py * width + px) * 4) as usize;
                    pixels[offset] = color[0];
                    pixels[offset + 1] = color[1];
                    pixels[offset + 2] = color[2];
                    pixels[offset + 3] = color[3];
                }
            }
        }
    }

    encode_png(width, height, &pixels)
}

/// Encode RGBA pixels into a PNG file.
fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let mut png = Vec::new();

    // PNG signature
    png.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);

    // IHDR chunk
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.push(8); // bit depth
    ihdr.push(6); // color type: RGBA
    ihdr.push(0); // compression
    ihdr.push(0); // filter
    ihdr.push(0); // interlace
    write_chunk(&mut png, b"IHDR", &ihdr);

    // IDAT chunk: build raw scanlines with filter byte, then zlib compress
    let row_bytes = (width * 4) as usize;
    let mut raw = Vec::with_capacity((height as usize) * (1 + row_bytes));
    for y in 0..height as usize {
        raw.push(0); // filter: None
        let start = y * row_bytes;
        raw.extend_from_slice(&rgba[start..start + row_bytes]);
    }

    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(&raw).unwrap();
    let compressed = encoder.finish().unwrap();
    write_chunk(&mut png, b"IDAT", &compressed);

    // IEND chunk
    write_chunk(&mut png, b"IEND", &[]);

    png
}

fn write_chunk(out: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
    let len = data.len() as u32;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(chunk_type);
    out.extend_from_slice(data);
    // CRC32 over chunk_type + data
    let crc = crc32(chunk_type, data);
    out.extend_from_slice(&crc.to_be_bytes());
}

fn crc32(chunk_type: &[u8], data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFFFFFF;
    for &b in chunk_type.iter().chain(data.iter()) {
        crc ^= b as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
        }
    }
    crc ^ 0xFFFFFFFF
}

/// Compute a lookup key for pregenerated icon table.
/// Encodes statuses as a base-3 number (0=Active, 1=Pending, 2=Idle).
pub fn status_key(statuses: &[Status]) -> u16 {
    let mut key: u16 = 0;
    for &s in statuses {
        key = key * 3 + s.index() as u16;
    }
    key
}

/// Get pregenerated dot grid PNG as base64 string.
/// Falls back to runtime generation if count > 5.
pub fn get_dot_grid_base64(statuses: &[Status]) -> String {
    if statuses.is_empty() {
        return String::new();
    }

    let count = statuses.len();
    let key = status_key(statuses);

    // Try pregenerated table (count 1..=5)
    if count <= 5 {
        if let Some(b64) = include!(concat!(env!("OUT_DIR"), "/icon_table.rs")) {
            return b64.to_string();
        }
    }

    // Fallback: runtime generation
    use base64::Engine;
    let png = make_dot_grid_png(statuses);
    base64::engine::general_purpose::STANDARD.encode(&png)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_png_magic() {
        let png = make_dot_grid_png(&[Status::Active]);
        assert!(png.len() > 8);
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    }

    #[test]
    fn test_dimensions_1_session() {
        let (w, h) = image_dims(1);
        // 1 dot: padding + dot + padding
        assert_eq!(w, 2 * PADDING + DOT_DIAMETER); // 6 + 10 = 16
        assert_eq!(h, 2 * PADDING + DOT_DIAMETER); // 16
    }

    #[test]
    fn test_dimensions_3_sessions() {
        let (w, h) = image_dims(3);
        // 3 dots in 1 row: padding + 3*dot + 2*spacing + padding
        assert_eq!(w, 2 * PADDING + 3 * DOT_DIAMETER + 2 * DOT_SPACING); // 6 + 30 + 8 = 44
        assert_eq!(h, 2 * PADDING + DOT_DIAMETER); // 16
    }

    #[test]
    fn test_dimensions_4_sessions() {
        let (w, h) = image_dims(4);
        // 4 dots: 3 cols, 2 rows
        assert_eq!(w, 2 * PADDING + 3 * DOT_DIAMETER + 2 * DOT_SPACING); // 44
        assert_eq!(h, 2 * PADDING + 2 * DOT_DIAMETER + DOT_SPACING); // 6 + 20 + 4 = 30
    }

    #[test]
    fn test_dimensions_9_sessions() {
        let (w, h) = image_dims(9);
        // 9 dots: 3x3
        assert_eq!(w, 2 * PADDING + 3 * DOT_DIAMETER + 2 * DOT_SPACING); // 44
        assert_eq!(h, 2 * PADDING + 3 * DOT_DIAMETER + 2 * DOT_SPACING); // 44
    }

    #[test]
    fn test_empty_returns_empty() {
        let png = make_dot_grid_png(&[]);
        assert!(png.is_empty());
    }

    #[test]
    fn test_base64_roundtrip() {
        use base64::Engine;
        let png = make_dot_grid_png(&[Status::Active, Status::Pending, Status::Idle]);
        let b64 = base64::engine::general_purpose::STANDARD.encode(&png);
        let decoded = base64::engine::general_purpose::STANDARD.decode(&b64).unwrap();
        assert_eq!(decoded, png);
        // Verify it's a valid PNG
        assert_eq!(&decoded[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    }

    #[test]
    fn test_different_statuses_produce_different_pngs() {
        let png1 = make_dot_grid_png(&[Status::Active]);
        let png2 = make_dot_grid_png(&[Status::Idle]);
        assert_ne!(png1, png2);
    }

    #[test]
    fn test_status_key() {
        // Single statuses
        assert_eq!(status_key(&[Status::Active]), 0);
        assert_eq!(status_key(&[Status::Pending]), 1);
        assert_eq!(status_key(&[Status::Idle]), 2);

        // Two statuses
        assert_eq!(status_key(&[Status::Active, Status::Active]), 0);
        assert_eq!(status_key(&[Status::Active, Status::Pending]), 1);
        assert_eq!(status_key(&[Status::Idle, Status::Idle]), 8); // 2*3+2

        // All keys for count=2 should be unique
        let all_statuses = [Status::Active, Status::Pending, Status::Idle];
        let mut keys = std::collections::HashSet::new();
        for &a in &all_statuses {
            for &b in &all_statuses {
                keys.insert(status_key(&[a, b]));
            }
        }
        assert_eq!(keys.len(), 9); // 3^2
    }

    #[test]
    fn test_pregenerated_table_complete() {
        // Verify all count=1..5 combinations have entries
        let all_statuses = [Status::Active, Status::Pending, Status::Idle];

        for count in 1..=5u32 {
            let total = 3u32.pow(count);
            for combo in 0..total {
                let mut statuses = Vec::new();
                let mut v = combo;
                for _ in 0..count {
                    statuses.push(Status::from_index((v % 3) as u8).unwrap());
                    v /= 3;
                }
                let b64 = get_dot_grid_base64(&statuses);
                assert!(!b64.is_empty(), "Missing entry for count={count}, combo={combo}");
            }
        }
    }

    #[test]
    fn test_pregenerated_matches_runtime() {
        use base64::Engine;

        // Spot check a few combinations
        let cases = vec![
            vec![Status::Active],
            vec![Status::Pending, Status::Idle],
            vec![Status::Active, Status::Active, Status::Idle],
        ];

        for statuses in cases {
            let pregenerated = get_dot_grid_base64(&statuses);
            let runtime_png = make_dot_grid_png(&statuses);
            let runtime_b64 = base64::engine::general_purpose::STANDARD.encode(&runtime_png);
            assert_eq!(pregenerated, runtime_b64, "Mismatch for {:?}", statuses);
        }
    }

    #[test]
    fn test_png_has_correct_chunks() {
        let png = make_dot_grid_png(&[Status::Active]);

        // Check for IHDR, IDAT, IEND chunks
        let has_ihdr = find_chunk(&png, b"IHDR");
        let has_idat = find_chunk(&png, b"IDAT");
        let has_iend = find_chunk(&png, b"IEND");

        assert!(has_ihdr, "Missing IHDR chunk");
        assert!(has_idat, "Missing IDAT chunk");
        assert!(has_iend, "Missing IEND chunk");
    }

    fn find_chunk(png: &[u8], chunk_type: &[u8; 4]) -> bool {
        // Skip signature (8 bytes), then scan chunks
        let mut pos = 8;
        while pos + 8 <= png.len() {
            let ct = &png[pos + 4..pos + 8];
            if ct == chunk_type {
                return true;
            }
            let len = u32::from_be_bytes([png[pos], png[pos + 1], png[pos + 2], png[pos + 3]]) as usize;
            pos += 12 + len; // 4 (len) + 4 (type) + len (data) + 4 (crc)
        }
        false
    }
}
