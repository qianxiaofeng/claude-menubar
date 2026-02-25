use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::env;
use std::fs;
use std::io::Write;
use std::path::Path;

// Duplicate the icon generation logic from src/icon.rs for build-time use.
// We can't import from the crate during build.

const COLOR_ACTIVE: [u8; 4] = [0x32, 0xD7, 0x4B, 0xFF];
const COLOR_PENDING: [u8; 4] = [0xFF, 0x9F, 0x0A, 0xFF];
const COLOR_IDLE: [u8; 4] = [0x8E, 0x8E, 0x93, 0xFF];
const DOT_DIAMETER: u32 = 10;
const DOT_SPACING: u32 = 4;
const PADDING: u32 = 3;
const MAX_COLS: u32 = 3;

#[derive(Clone, Copy)]
enum Status {
    Active = 0,
    Pending = 1,
    Idle = 2,
}

impl Status {
    fn color(self) -> [u8; 4] {
        match self {
            Status::Active => COLOR_ACTIVE,
            Status::Pending => COLOR_PENDING,
            Status::Idle => COLOR_IDLE,
        }
    }

    fn from_index(i: u8) -> Self {
        match i {
            0 => Status::Active,
            1 => Status::Pending,
            _ => Status::Idle,
        }
    }
}

fn grid_dims(n: u32) -> (u32, u32) {
    if n == 0 {
        return (0, 0);
    }
    let cols = n.min(MAX_COLS);
    let rows = (n + MAX_COLS - 1) / MAX_COLS;
    (cols, rows)
}

fn image_dims(n: u32) -> (u32, u32) {
    if n == 0 {
        return (0, 0);
    }
    let (cols, rows) = grid_dims(n);
    let w = 2 * PADDING + cols * DOT_DIAMETER + (cols - 1) * DOT_SPACING;
    let h = 2 * PADDING + rows * DOT_DIAMETER + (rows - 1) * DOT_SPACING;
    (w, h)
}

fn make_dot_grid_png(statuses: &[Status]) -> Vec<u8> {
    let n = statuses.len() as u32;
    if n == 0 {
        return Vec::new();
    }

    let (width, height) = image_dims(n);
    let (cols, _) = grid_dims(n);

    let mut pixels = vec![0u8; (width * height * 4) as usize];

    for (i, status) in statuses.iter().enumerate() {
        let col = i as u32 % cols;
        let row = i as u32 / cols;
        let cx = PADDING + col * (DOT_DIAMETER + DOT_SPACING) + DOT_DIAMETER / 2;
        let cy = PADDING + row * (DOT_DIAMETER + DOT_SPACING) + DOT_DIAMETER / 2;
        let color = status.color();
        let r = DOT_DIAMETER as f32 / 2.0;

        let x_start = cx.saturating_sub(DOT_DIAMETER / 2 + 1);
        let x_end = (cx + DOT_DIAMETER / 2 + 2).min(width);
        let y_start = cy.saturating_sub(DOT_DIAMETER / 2 + 1);
        let y_end = (cy + DOT_DIAMETER / 2 + 2).min(height);

        for py in y_start..y_end {
            for px in x_start..x_end {
                let dx = px as f32 - cx as f32 + 0.5;
                let dy = py as f32 - cy as f32 + 0.5;
                let dist = (dx * dx + dy * dy).sqrt();
                let alpha = (r - dist + 0.5).clamp(0.0, 1.0);
                if alpha > 0.0 {
                    let offset = ((py * width + px) * 4) as usize;
                    let a = (alpha * color[3] as f32) as u8;
                    pixels[offset] = color[0];
                    pixels[offset + 1] = color[1];
                    pixels[offset + 2] = color[2];
                    pixels[offset + 3] = a;
                }
            }
        }
    }

    encode_png(width, height, &pixels)
}

fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let mut png = Vec::new();
    png.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);

    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.push(8);
    ihdr.push(6);
    ihdr.push(0);
    ihdr.push(0);
    ihdr.push(0);
    write_chunk(&mut png, b"IHDR", &ihdr);

    let row_bytes = (width * 4) as usize;
    let mut raw = Vec::with_capacity((height as usize) * (1 + row_bytes));
    for y in 0..height as usize {
        raw.push(0);
        let start = y * row_bytes;
        raw.extend_from_slice(&rgba[start..start + row_bytes]);
    }

    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(&raw).unwrap();
    let compressed = encoder.finish().unwrap();
    write_chunk(&mut png, b"IDAT", &compressed);

    write_chunk(&mut png, b"IEND", &[]);
    png
}

fn write_chunk(out: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
    let len = data.len() as u32;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(chunk_type);
    out.extend_from_slice(data);
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

fn status_key(statuses: &[Status]) -> u16 {
    let mut key: u16 = 0;
    for s in statuses {
        key = key * 3 + *s as u16;
    }
    key
}

fn main() {
    use base64::Engine;

    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("icon_table.rs");

    let mut code = String::new();
    code.push_str("{\n");
    code.push_str("    match (count, key) {\n");

    // Generate all combinations for count=1..=5
    for count in 1u32..=5 {
        let total = 3u32.pow(count);
        for combo in 0..total {
            let mut statuses = Vec::new();
            let mut v = combo;
            for _ in 0..count {
                statuses.push(Status::from_index((v % 3) as u8));
                v /= 3;
            }

            let key = status_key(&statuses);
            let png = make_dot_grid_png(&statuses);
            let b64 = base64::engine::general_purpose::STANDARD.encode(&png);

            code.push_str(&format!(
                "        ({}, {}) => Some(\"{}\"),\n",
                count, key, b64
            ));
        }
    }

    code.push_str("        _ => None,\n");
    code.push_str("    }\n");
    code.push_str("}\n");

    fs::write(dest_path, code).unwrap();

    println!("cargo:rerun-if-changed=build.rs");
}
