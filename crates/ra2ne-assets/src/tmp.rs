//! TS/RA2 TMP terrain tiles: metadata, diamond pixels, optional extra graphics
//! and Z planes. Damaged variants are retained in the source but not rendered.
use crate::sprite::IndexedImage;
use std::{ops::Range, sync::Arc};
const MAX_PIXELS: usize = 4_194_304;
#[derive(Clone, Debug)]
pub struct TmpTile {
    pub x: i32,
    pub y: i32,
    pub height: i8,
    pub terrain_type: u8,
    pub ramp_type: u8,
    pub radar_left: [u8; 3],
    pub radar_right: [u8; 3],
    pub flags: u32,
    pub extra_x: i32,
    pub extra_y: i32,
    diamond: Range<usize>,
    z: Option<Range<usize>>,
    extra: Option<(u16, u16, Range<usize>)>,
    extra_z: Option<Range<usize>>,
}
#[derive(Clone, Debug)]
pub struct Tmp {
    pub blocks_width: u16,
    pub blocks_height: u16,
    pub cell_width: u16,
    pub cell_height: u16,
    bytes: Arc<[u8]>,
    tiles: Vec<Option<TmpTile>>,
}
impl Tmp {
    pub fn parse(bytes: Arc<[u8]>) -> Result<Self, &'static str> {
        let bx = number(&bytes, 0)?;
        let by = number(&bytes, 4)?;
        let width = number(&bytes, 8)?;
        let height = number(&bytes, 12)?;
        if bx == 0
            || by == 0
            || bx > 256
            || by > 256
            || !(4..=2048).contains(&width)
            || width % 4 != 0
            || height != width / 2
        {
            return Err("invalid TMP dimensions");
        }
        let count = (bx * by) as usize;
        let index_end = 16 + count * 4;
        if index_end > bytes.len() {
            return Err("truncated TMP tile index");
        }
        let diamond_size = (width * height / 2) as usize;
        let mut tiles = Vec::with_capacity(count);
        for index in 0..count {
            let at = number(&bytes, 16 + index * 4)? as usize;
            if at == 0 {
                tiles.push(None);
                continue;
            }
            if at < index_end || at.checked_add(52).is_none_or(|end| end > bytes.len()) {
                return Err("TMP tile header outside file");
            }
            let flags = number(&bytes, at + 36)?;
            // Only the low three bitfields carry format semantics. Shipped
            // files contain uninitialized high bits (often 0xcdcdcd..); keep
            // the raw word for inspection without treating padding as flags.
            let range = |relative: usize, length: usize| -> Result<Range<usize>, &'static str> {
                let start = at.checked_add(relative).ok_or("TMP data offset overflow")?;
                let end = start.checked_add(length).ok_or("TMP data size overflow")?;
                if relative < 52 || end > bytes.len() {
                    return Err("TMP pixels outside file");
                }
                Ok(start..end)
            };
            let diamond = range(52, diamond_size)?;
            let z = if flags & 2 != 0 {
                Some(range(number(&bytes, at + 12)? as usize, diamond_size)?)
            } else {
                None
            };
            let ew = number(&bytes, at + 28)?;
            let eh = number(&bytes, at + 32)?;
            let extra_size = usize::try_from(ew)
                .ok()
                .and_then(|w| usize::try_from(eh).ok().and_then(|h| w.checked_mul(h)))
                .ok_or("TMP extra dimensions overflow")?;
            let extra = if flags & 1 != 0 {
                if ew == 0 || eh == 0 || ew > 2048 || eh > 2048 || extra_size > MAX_PIXELS {
                    return Err("invalid TMP extra dimensions");
                }
                Some((
                    ew as u16,
                    eh as u16,
                    range(number(&bytes, at + 8)? as usize, extra_size)?,
                ))
            } else {
                None
            };
            let extra_z = if flags & 3 == 3 {
                Some(range(number(&bytes, at + 16)? as usize, extra_size)?)
            } else {
                None
            };
            tiles.push(Some(TmpTile {
                x: number(&bytes, at)? as i32,
                y: number(&bytes, at + 4)? as i32,
                height: bytes[at + 40] as i8,
                terrain_type: bytes[at + 41],
                ramp_type: bytes[at + 42],
                radar_left: bytes[at + 43..at + 46].try_into().unwrap(),
                radar_right: bytes[at + 46..at + 49].try_into().unwrap(),
                flags,
                extra_x: number(&bytes, at + 20)? as i32,
                extra_y: number(&bytes, at + 24)? as i32,
                diamond,
                z,
                extra,
                extra_z,
            }));
        }
        Ok(Self {
            blocks_width: bx as u16,
            blocks_height: by as u16,
            cell_width: width as u16,
            cell_height: height as u16,
            bytes,
            tiles,
        })
    }
    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }
    pub fn tile(&self, index: usize) -> Option<&TmpTile> {
        self.tiles.get(index)?.as_ref()
    }
    pub fn diamond(&self, index: usize) -> Result<IndexedImage, &'static str> {
        let tile = self.tile(index).ok_or("missing TMP tile")?;
        unpack_diamond(
            &self.bytes[tile.diamond.clone()],
            self.cell_width,
            self.cell_height,
        )
    }
    pub fn z_plane(&self, index: usize) -> Result<Option<IndexedImage>, &'static str> {
        let tile = self.tile(index).ok_or("missing TMP tile")?;
        tile.z
            .as_ref()
            .map(|r| unpack_diamond(&self.bytes[r.clone()], self.cell_width, self.cell_height))
            .transpose()
    }
    pub fn extra(&self, index: usize) -> Option<IndexedImage> {
        let (width, height, range) = self.tile(index)?.extra.as_ref()?;
        Some(IndexedImage {
            width: *width,
            height: *height,
            pixels: self.bytes[range.clone()].to_vec(),
        })
    }
    pub fn extra_z_plane(&self, index: usize) -> Option<&[u8]> {
        Some(&self.bytes[self.tile(index)?.extra_z.clone()?])
    }
}
fn unpack_diamond(bytes: &[u8], width: u16, height: u16) -> Result<IndexedImage, &'static str> {
    let mut image = IndexedImage {
        width,
        height,
        pixels: vec![0; usize::from(width) * usize::from(height)],
    };
    let half = usize::from(height) / 2;
    let mut source = 0;
    for row in 0..usize::from(height) {
        let length = if row < half {
            (row + 1) * 4
        } else {
            (usize::from(height) - row - 1) * 4
        };
        let x = (usize::from(width) - length) / 2;
        let start = row * usize::from(width) + x;
        image.pixels[start..start + length].copy_from_slice(
            bytes
                .get(source..source + length)
                .ok_or("truncated TMP diamond")?,
        );
        source += length;
    }
    if source != bytes.len() {
        return Err("TMP diamond length mismatch");
    }
    Ok(image)
}
fn number(bytes: &[u8], at: usize) -> Result<u32, &'static str> {
    Ok(u32::from_le_bytes(
        bytes
            .get(at..at + 4)
            .ok_or("truncated TMP integer")?
            .try_into()
            .unwrap(),
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> Vec<u8> {
        let mut bytes = Vec::new();
        for n in [1_u32, 1, 8, 4, 20] {
            bytes.extend(n.to_le_bytes());
        }
        let mut header = [0_u8; 52];
        header[40] = 2;
        header[41] = 3;
        header[42] = 1;
        header[43..49].copy_from_slice(&[1, 2, 3, 4, 5, 6]);
        bytes.extend(header);
        bytes.extend(1_u8..=16);
        bytes
    }
    #[test]
    fn diamond_shape_and_terrain_metadata_decode() {
        let tmp = Tmp::parse(Arc::from(sample())).unwrap();
        let tile = tmp.tile(0).unwrap();
        assert_eq!((tile.height, tile.terrain_type, tile.ramp_type), (2, 3, 1));
        assert_eq!(tile.radar_left, [1, 2, 3]);
        assert_eq!(
            tmp.diamond(0).unwrap().pixels,
            vec![
                0, 0, 1, 2, 3, 4, 0, 0, 5, 6, 7, 8, 9, 10, 11, 12, 0, 0, 13, 14, 15, 16, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0
            ]
        );
        assert!(tmp.extra(0).is_none());
        assert!(tmp.z_plane(0).unwrap().is_none());
    }
    #[test]
    fn missing_tiles_and_bad_offsets_fail_without_overread() {
        let bytes = sample();
        for n in 0..bytes.len() {
            assert!(Tmp::parse(Arc::from(&bytes[..n])).is_err());
        }
        let mut missing = bytes.clone();
        missing[16..20].fill(0);
        assert!(Tmp::parse(Arc::from(missing)).unwrap().tile(0).is_none());
        let mut bad = bytes;
        bad[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(Tmp::parse(Arc::from(bad)).is_err());
    }
    #[test]
    fn inactive_header_padding_is_preserved_without_changing_pixels() {
        let mut bytes = sample();
        bytes[56..60].copy_from_slice(&0xcdcd_cdc8_u32.to_le_bytes());
        let parsed = Tmp::parse(Arc::from(bytes)).unwrap();
        assert_eq!(parsed.tile(0).unwrap().flags, 0xcdcd_cdc8);
        assert_eq!(
            parsed.diamond(0).unwrap(),
            Tmp::parse(Arc::from(sample())).unwrap().diamond(0).unwrap()
        );
    }
}
