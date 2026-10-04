//! Palette and TS/RA2 SHP sprites, decoded on demand with allocation limits.
use std::{ops::Range, sync::Arc};
const MAX_PIXELS: usize = 4_194_304;

#[derive(Clone, Debug)]
pub struct Palette {
    pub colors: [[u8; 3]; 256],
}
impl Palette {
    /// Westwood PAL channels use six bits. Transparent pixels are determined
    /// by sprite index zero, not by whether that palette entry is black.
    pub fn parse(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() != 768 || bytes.iter().any(|&v| v > 63) {
            return Err("PAL must contain 256 six-bit RGB entries");
        }
        let colors = std::array::from_fn(|i| {
            [
                bytes[i * 3] << 2,
                bytes[i * 3 + 1] << 2,
                bytes[i * 3 + 2] << 2,
            ]
        });
        Ok(Self { colors })
    }
    pub fn rgba(&self, image: &IndexedImage, transparent_zero: bool) -> Vec<u8> {
        let mut out = Vec::with_capacity(image.pixels.len() * 4);
        for &index in &image.pixels {
            out.extend(self.colors[usize::from(index)]);
            out.push(if transparent_zero && index == 0 {
                0
            } else {
                255
            });
        }
        out
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexedImage {
    pub width: u16,
    pub height: u16,
    pub pixels: Vec<u8>,
}
#[derive(Clone, Debug)]
struct Frame {
    x: u16,
    y: u16,
    width: u16,
    height: u16,
    compression: u32,
    data: Range<usize>,
}
#[derive(Clone, Debug)]
pub struct Shp {
    pub width: u16,
    pub height: u16,
    bytes: Arc<[u8]>,
    frames: Vec<Frame>,
}
impl Shp {
    pub fn parse(bytes: Arc<[u8]>) -> Result<Self, &'static str> {
        let width = word(&bytes, 2)?;
        let height = word(&bytes, 4)?;
        let count = usize::from(word(&bytes, 6)?);
        if word(&bytes, 0)? != 0
            || width == 0
            || height == 0
            || usize::from(width) * usize::from(height) > MAX_PIXELS
            || count == 0
            || count > 10_000
        {
            return Err("invalid TS/RA2 SHP header");
        }
        let directory_end = 8 + count * 24;
        if directory_end > bytes.len() {
            return Err("truncated SHP frame directory");
        }
        let mut headers = Vec::with_capacity(count);
        let mut offsets = Vec::new();
        for index in 0..count {
            let at = 8 + index * 24;
            let x = word(&bytes, at)?;
            let y = word(&bytes, at + 2)?;
            let w = word(&bytes, at + 4)?;
            let h = word(&bytes, at + 6)?;
            let compression = dword(&bytes, at + 8)?;
            let offset = dword(&bytes, at + 20)? as usize;
            if dword(&bytes, at + 16)? != 0 || compression > 3 {
                return Err("unsupported SHP frame flags");
            }
            if w == 0 && h == 0 && offset == 0 {
                headers.push((x, y, w, h, compression, offset));
                continue;
            }
            if w == 0
                || h == 0
                || u32::from(x) + u32::from(w) > u32::from(width)
                || u32::from(y) + u32::from(h) > u32::from(height)
                || offset < directory_end
                || offset >= bytes.len()
            {
                return Err("SHP frame outside canvas or file");
            }
            offsets.push(offset);
            headers.push((x, y, w, h, compression, offset));
        }
        offsets.sort_unstable();
        offsets.dedup();
        offsets.push(bytes.len());
        let mut frames = Vec::with_capacity(count);
        for (x, y, w, h, compression, offset) in headers {
            let end = if offset == 0 {
                0
            } else {
                offsets[offsets.binary_search(&offset).unwrap() + 1]
            };
            if compression & 2 == 0 && usize::from(w) * usize::from(h) > end - offset {
                return Err("truncated raw SHP frame");
            }
            frames.push(Frame {
                x,
                y,
                width: w,
                height: h,
                compression,
                data: offset..end,
            });
        }
        Ok(Self {
            width,
            height,
            bytes,
            frames,
        })
    }
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }
    pub fn frame(&self, index: usize) -> Result<IndexedImage, &'static str> {
        let frame = self
            .frames
            .get(index)
            .ok_or("SHP frame index out of range")?;
        let mut image = IndexedImage {
            width: self.width,
            height: self.height,
            pixels: vec![0; usize::from(self.width) * usize::from(self.height)],
        };
        if frame.width == 0 {
            return Ok(image);
        }
        let data = &self.bytes[frame.data.clone()];
        let decoded = if frame.compression & 2 != 0 {
            decode_rows(data, frame.width, frame.height)?
        } else {
            data[..usize::from(frame.width) * usize::from(frame.height)].to_vec()
        };
        for row in 0..usize::from(frame.height) {
            let at = (usize::from(frame.y) + row) * usize::from(self.width) + usize::from(frame.x);
            let start = row * usize::from(frame.width);
            image.pixels[at..at + usize::from(frame.width)]
                .copy_from_slice(&decoded[start..start + usize::from(frame.width)]);
        }
        Ok(image)
    }
}
fn decode_rows(mut bytes: &[u8], width: u16, height: u16) -> Result<Vec<u8>, &'static str> {
    let mut out = Vec::with_capacity(usize::from(width) * usize::from(height));
    for _ in 0..height {
        let length = usize::from(word(bytes, 0)?);
        if length < 2 {
            return Err("SHP row length smaller than prefix");
        }
        let mut row = bytes.get(2..length).ok_or("truncated SHP row")?;
        let start = out.len();
        while let Some((&value, tail)) = row.split_first() {
            row = tail;
            if value == 0 {
                let (&count, tail) = row.split_first().ok_or("truncated SHP transparent run")?;
                row = tail;
                if count == 0 || out.len() - start + usize::from(count) > usize::from(width) {
                    return Err("invalid SHP transparent run");
                }
                out.resize(out.len() + usize::from(count), 0);
            } else {
                if out.len() - start >= usize::from(width) {
                    return Err("SHP row exceeds frame width");
                }
                out.push(value);
            }
        }
        if out.len() - start != usize::from(width) {
            return Err("SHP row width mismatch");
        }
        bytes = &bytes[length..];
    }
    // Byte-range alignment padding is allowed after the complete frame.
    Ok(out)
}
fn word(bytes: &[u8], at: usize) -> Result<u16, &'static str> {
    Ok(u16::from_le_bytes(
        bytes
            .get(at..at + 2)
            .ok_or("truncated sprite integer")?
            .try_into()
            .unwrap(),
    ))
}
fn dword(bytes: &[u8], at: usize) -> Result<u32, &'static str> {
    Ok(u32::from_le_bytes(
        bytes
            .get(at..at + 4)
            .ok_or("truncated sprite integer")?
            .try_into()
            .unwrap(),
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(compressed: bool) -> Vec<u8> {
        let mut bytes = Vec::new();
        for value in [0_u16, 4, 3, 1] {
            bytes.extend(value.to_le_bytes());
        }
        for value in [1_u16, 1, 2, 1] {
            bytes.extend(value.to_le_bytes());
        }
        for value in [if compressed { 3_u32 } else { 1 }, 0, 0, 32] {
            bytes.extend(value.to_le_bytes());
        }
        if compressed {
            bytes.extend([5, 0, 7, 0, 1]);
        } else {
            bytes.extend([7, 0]);
        }
        bytes
    }
    #[test]
    fn raw_and_transparent_run_frames_decode_to_identical_canvas() {
        let a = Shp::parse(Arc::from(fixture(false)))
            .unwrap()
            .frame(0)
            .unwrap();
        let b = Shp::parse(Arc::from(fixture(true)))
            .unwrap()
            .frame(0)
            .unwrap();
        assert_eq!(a, b);
        assert_eq!(a.pixels, vec![0, 0, 0, 0, 0, 7, 0, 0, 0, 0, 0, 0]);
        let mut bytes = vec![0; 768];
        bytes[21] = 63;
        let palette = Palette::parse(&bytes).unwrap();
        assert_eq!(&palette.rgba(&a, true)[20..24], &[252, 0, 0, 255]);
        assert_eq!(palette.rgba(&a, true)[3], 0);
    }
    #[test]
    fn malformed_directories_runs_and_palette_values_are_rejected() {
        let bytes = fixture(false);
        for n in 0..bytes.len() {
            assert!(Shp::parse(Arc::from(&bytes[..n])).is_err());
        }
        let mut bytes = fixture(true);
        *bytes.last_mut().unwrap() = 3;
        assert!(Shp::parse(Arc::from(bytes)).unwrap().frame(0).is_err());
        let mut bytes = vec![0; 768];
        bytes[0] = 64;
        assert!(Palette::parse(&bytes).is_err());
        assert!(decode_rows(&[1, 0], 1, 1).is_err());
    }
}
