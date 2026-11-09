//! Pure PNG normalization boundary. No filesystem, IPC, URL or native target access.
use sha2::{Digest, Sha256};
use std::io::{Cursor, Write};

pub const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_PIXELS: usize = 4 * 1024 * 1024;
pub const MAX_AXIS: u32 = 4096;
pub const MAX_OUTPUT_BYTES: usize = 20 * 1024 * 1024;
const MAX_CHUNKS: usize = 4096;
const SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageError {
    InputLimit,
    Format,
    Animation,
    Dimensions,
    Decode,
    Encode,
    Allocation,
}

#[derive(Debug, PartialEq, Eq)]
pub struct NormalizedImage {
    pub width: u32,
    pub height: u32,
    /// Newly encoded RGBA8 PNG bytes, never the original container/metadata.
    pub bytes: Vec<u8>,
    pub sha256: String,
    /// Portable declaration path, not a machine-specific filesystem location.
    pub path: String,
}

fn scan(bytes: &[u8]) -> Result<(u32, u32), ImageError> {
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(ImageError::InputLimit);
    }
    if !bytes.starts_with(SIGNATURE) {
        return Err(ImageError::Format);
    }
    let mut offset = 8usize;
    let mut dimensions = None;
    let mut count = 0usize;
    while offset < bytes.len() {
        count += 1;
        if count > MAX_CHUNKS || bytes.len() - offset < 12 {
            return Err(ImageError::Format);
        }
        let size = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
        let end = offset
            .checked_add(12)
            .and_then(|n| n.checked_add(size))
            .ok_or(ImageError::Format)?;
        if end > bytes.len() {
            return Err(ImageError::Format);
        }
        let kind = &bytes[offset + 4..offset + 8];
        if !kind.iter().all(u8::is_ascii_alphabetic) {
            return Err(ImageError::Format);
        }
        let crc = u32::from_be_bytes(bytes[end - 4..end].try_into().unwrap());
        if crc32fast::hash(&bytes[offset + 4..end - 4]) != crc {
            return Err(ImageError::Format);
        }
        if matches!(kind, b"acTL" | b"fcTL" | b"fdAT") {
            return Err(ImageError::Animation);
        }
        if count == 1 && kind != b"IHDR" {
            return Err(ImageError::Format);
        }
        if kind == b"IHDR" {
            if dimensions.is_some() || size != 13 {
                return Err(ImageError::Format);
            }
            let width = u32::from_be_bytes(bytes[offset + 8..offset + 12].try_into().unwrap());
            let height = u32::from_be_bytes(bytes[offset + 12..offset + 16].try_into().unwrap());
            let pixels = u64::from(width) * u64::from(height);
            if width == 0
                || height == 0
                || width > MAX_AXIS
                || height > MAX_AXIS
                || pixels > MAX_PIXELS as u64
            {
                return Err(ImageError::Dimensions);
            }
            dimensions = Some((width, height));
        }
        if kind == b"IEND" {
            if size != 0 || end != bytes.len() {
                return Err(ImageError::Format);
            }
            return dimensions.ok_or(ImageError::Format);
        }
        offset = end;
    }
    Err(ImageError::Format)
}

fn buffer(size: usize) -> Result<Vec<u8>, ImageError> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(size)
        .map_err(|_| ImageError::Allocation)?;
    bytes.resize(size, 0);
    Ok(bytes)
}

#[derive(Default)]
struct BoundedOutput(Vec<u8>);
impl Write for BoundedOutput {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > MAX_OUTPUT_BYTES.saturating_sub(self.0.len()) {
            return Err(std::io::Error::other("PNG output limit"));
        }
        self.0
            .try_reserve_exact(bytes.len())
            .map_err(std::io::Error::other)?;
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Decode under byte/pixel/scratch bounds, then encode a fresh metadata-free PNG.
/// Callers must separately bound file reads and use a background worker before exposing IPC.
pub fn normalize_png(bytes: &[u8]) -> Result<NormalizedImage, ImageError> {
    let (width, height) = scan(bytes)?;
    let mut decoder = png::Decoder::new_with_limits(
        Cursor::new(bytes),
        png::Limits {
            bytes: 16 * 1024 * 1024,
        },
    );
    decoder.set_ignore_text_chunk(true);
    decoder.set_ignore_iccp_chunk(true);
    decoder.ignore_checksums(false);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|_| ImageError::Decode)?;
    let size = reader
        .output_buffer_size()
        .filter(|n| *n <= MAX_PIXELS * 4)
        .ok_or(ImageError::Decode)?;
    let mut decoded = buffer(size)?;
    let frame = reader
        .next_frame(&mut decoded)
        .map_err(|_| ImageError::Decode)?;
    reader.finish().map_err(|_| ImageError::Decode)?;
    if frame.width != width || frame.height != height || frame.bit_depth != png::BitDepth::Eight {
        return Err(ImageError::Decode);
    }
    let pixels = width as usize * height as usize;
    let channels = match frame.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return Err(ImageError::Decode),
    };
    if frame.buffer_size() != pixels * channels {
        return Err(ImageError::Decode);
    }
    let mut rgba = buffer(pixels * 4)?;
    for (source, target) in decoded[..frame.buffer_size()]
        .chunks_exact(channels)
        .zip(rgba.as_chunks_mut::<4>().0)
    {
        match channels {
            1 => target.copy_from_slice(&[source[0], source[0], source[0], 255]),
            2 => target.copy_from_slice(&[source[0], source[0], source[0], source[1]]),
            3 => target.copy_from_slice(&[source[0], source[1], source[2], 255]),
            _ => target.copy_from_slice(source),
        }
    }
    let mut output = BoundedOutput::default();
    {
        let mut encoder = png::Encoder::new(&mut output, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|_| ImageError::Encode)?;
        writer
            .write_image_data(&rgba)
            .map_err(|_| ImageError::Encode)?;
        writer.finish().map_err(|_| ImageError::Encode)?;
    }
    let sha256 = format!("{:x}", Sha256::digest(&output.0));
    Ok(NormalizedImage {
        width,
        height,
        path: format!("assets/images/{sha256}.png"),
        sha256,
        bytes: output.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn png_bytes(color: png::ColorType, depth: png::BitDepth, pixels: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
            encoder.set_color(color);
            encoder.set_depth(depth);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(pixels).unwrap();
            writer.finish().unwrap();
        }
        bytes
    }
    fn chunk(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut result = (body.len() as u32).to_be_bytes().to_vec();
        result.extend(kind);
        result.extend(body);
        let crc = crc32fast::hash(&result[4..]);
        result.extend(crc.to_be_bytes());
        result
    }
    fn insert(bytes: &mut Vec<u8>, kind: &[u8; 4], body: &[u8]) {
        bytes.splice(33..33, chunk(kind, body));
    }
    fn pixels(bytes: &[u8]) -> Vec<u8> {
        let mut reader = png::Decoder::new(Cursor::new(bytes)).read_info().unwrap();
        let mut output = vec![0; reader.output_buffer_size().unwrap()];
        reader.next_frame(&mut output).unwrap();
        output
    }
    #[test]
    fn converts_color_types_and_strips_metadata() {
        for (color, input, expected) in [
            (png::ColorType::Grayscale, vec![42], vec![42, 42, 42, 255]),
            (
                png::ColorType::GrayscaleAlpha,
                vec![42, 17],
                vec![42, 42, 42, 17],
            ),
            (png::ColorType::Rgb, vec![1, 2, 3], vec![1, 2, 3, 255]),
            (png::ColorType::Rgba, vec![1, 2, 3, 4], vec![1, 2, 3, 4]),
        ] {
            let mut source = png_bytes(color, png::BitDepth::Eight, &input);
            insert(&mut source, b"tEXt", b"Source\0private metadata");
            let image = normalize_png(&source).unwrap();
            assert_eq!(pixels(&image.bytes), expected);
            assert!(!image.bytes.windows(4).any(|w| w == b"tEXt"));
            assert_eq!(image.path, format!("assets/images/{}.png", image.sha256));
            assert_eq!(image.sha256, format!("{:x}", Sha256::digest(&image.bytes)));
            assert_eq!(normalize_png(&image.bytes).unwrap(), image);
        }
    }
    #[test]
    fn expands_palette_transparency_and_discards_ignored_profiles() {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
            encoder.set_color(png::ColorType::Indexed);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_palette(vec![20, 40, 60]);
            encoder.set_trns(vec![17]);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&[0]).unwrap();
            writer.finish().unwrap();
        }
        // These are deliberately invalid compressed metadata: they must not be inflated.
        insert(&mut bytes, b"iCCP", b"PrivateProfile\0\0not-deflate");
        insert(&mut bytes, b"zTXt", b"PrivateText\0\0not-deflate");
        let normalized = normalize_png(&bytes).unwrap();
        assert_eq!(pixels(&normalized.bytes), vec![20, 40, 60, 17]);
        assert!(!normalized.bytes.windows(7).any(|w| w == b"Private"));
    }
    #[test]
    fn rejects_valid_crc_but_invalid_pixel_stream_and_duplicate_headers() {
        let bytes = png_bytes(png::ColorType::Rgb, png::BitDepth::Eight, &[1, 2, 3]);
        let mut invalid = bytes[..33].to_vec();
        invalid.extend(chunk(b"IDAT", b"not-deflate"));
        invalid.extend(chunk(b"IEND", &[]));
        assert_eq!(normalize_png(&invalid), Err(ImageError::Decode));
        let mut duplicate = bytes.clone();
        insert(&mut duplicate, b"IHDR", &bytes[16..29]);
        assert_eq!(normalize_png(&duplicate), Err(ImageError::Format));
        let mut unknown = bytes;
        insert(&mut unknown, b"EVIL", b"unrecognized critical chunk");
        assert_eq!(normalize_png(&unknown), Err(ImageError::Decode));
    }
    #[test]
    fn expands_low_depth_and_reduces_16_bit() {
        let source = png_bytes(png::ColorType::Grayscale, png::BitDepth::One, &[128]);
        assert_eq!(
            pixels(&normalize_png(&source).unwrap().bytes),
            vec![255, 255, 255, 255]
        );
        let source = png_bytes(
            png::ColorType::Rgb,
            png::BitDepth::Sixteen,
            &[1, 2, 3, 4, 5, 6],
        );
        assert_eq!(
            pixels(&normalize_png(&source).unwrap().bytes),
            vec![1, 3, 5, 255]
        );
    }
    #[test]
    fn rejects_truncation_corruption_trailing_and_wrong_formats() {
        let source = png_bytes(png::ColorType::Rgba, png::BitDepth::Eight, &[1, 2, 3, 4]);
        for length in 0..source.len() {
            assert!(normalize_png(&source[..length]).is_err());
        }
        for bytes in [
            b"<svg onload='script'/>".as_slice(),
            b"GIF89a",
            b"\xff\xd8\xff",
        ] {
            assert_eq!(normalize_png(bytes), Err(ImageError::Format));
        }
        let mut corrupt = source.clone();
        corrupt[29] ^= 1;
        assert_eq!(normalize_png(&corrupt), Err(ImageError::Format));
        let mut trailing = source;
        trailing.push(0);
        assert_eq!(normalize_png(&trailing), Err(ImageError::Format));
        assert_eq!(
            normalize_png(&vec![0; MAX_INPUT_BYTES + 1]),
            Err(ImageError::InputLimit)
        );
    }
    #[test]
    fn rejects_animation_and_huge_dimensions_before_decode() {
        let source = png_bytes(png::ColorType::Rgba, png::BitDepth::Eight, &[1, 2, 3, 4]);
        for kind in [b"acTL", b"fcTL", b"fdAT"] {
            let mut bytes = source.clone();
            insert(&mut bytes, kind, &[0; 8]);
            assert_eq!(normalize_png(&bytes), Err(ImageError::Animation));
        }
        for (width, height) in [(0, 1), (4097, 1), (4096, 4096)] {
            let mut bytes = source.clone();
            bytes[16..20].copy_from_slice(&u32::to_be_bytes(width));
            bytes[20..24].copy_from_slice(&u32::to_be_bytes(height));
            let crc = crc32fast::hash(&bytes[12..29]);
            bytes[29..33].copy_from_slice(&crc.to_be_bytes());
            assert_eq!(normalize_png(&bytes), Err(ImageError::Dimensions));
        }
    }
    #[test]
    fn bounded_output_refuses_excess_and_chunk_floods() {
        let mut output = BoundedOutput(vec![0; MAX_OUTPUT_BYTES]);
        assert!(output.write_all(&[1]).is_err());
        let mut source = png_bytes(png::ColorType::Rgba, png::BitDepth::Eight, &[0, 0, 0, 0]);
        let chunks: Vec<u8> = (0..MAX_CHUNKS)
            .flat_map(|_| chunk(b"tEXt", b"K\0V"))
            .collect();
        source.splice(33..33, chunks);
        assert_eq!(normalize_png(&source), Err(ImageError::Format));
    }
}
