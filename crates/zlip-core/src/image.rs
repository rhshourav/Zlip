//! Clipboard image handling: Windows DIB -> RGBA -> PNG, PNG header probing and
//! thumbnails. Pure data conversion, so it is tested on Linux.

use std::io::Cursor;

/// Refuse absurd dimensions before allocating anything.
const MAX_SIDE: u32 = 16_384;
const MAX_PIXELS: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgba8Image {
    pub width: u32,
    pub height: u32,
    /// `width * height * 4` bytes, row-major, top-down.
    pub pixels: Vec<u8>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ImageError {
    #[error("unsupported or malformed bitmap: {0}")]
    Dib(&'static str),
    #[error("image is too large")]
    TooLarge,
    #[error("PNG error: {0}")]
    Png(String),
}

fn check_dims(w: u32, h: u32) -> Result<(), ImageError> {
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE || u64::from(w) * u64::from(h) > MAX_PIXELS
    {
        return Err(ImageError::TooLarge);
    }
    Ok(())
}

fn u16_at(b: &[u8], o: usize) -> Option<u16> {
    b.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]]))
}
fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// Convert a `CF_DIB` / `CF_DIBV5` blob (a BITMAPINFOHEADER and pixels, no file
/// header) to RGBA. Supports 24-bit and 32-bit, `BI_RGB` and `BI_BITFIELDS`.
pub fn dib_to_rgba(dib: &[u8]) -> Result<Rgba8Image, ImageError> {
    let bad = ImageError::Dib;
    let header = u32_at(dib, 0).ok_or(bad("truncated header"))? as usize;
    if !matches!(header, 40 | 108 | 124) {
        return Err(bad("unknown header size"));
    }
    let width = u32_at(dib, 4).ok_or(bad("truncated header"))? as i32;
    let raw_h = u32_at(dib, 8).ok_or(bad("truncated header"))? as i32;
    let bits = u16_at(dib, 14).ok_or(bad("truncated header"))?;
    let compression = u32_at(dib, 16).ok_or(bad("truncated header"))?;
    if width <= 0 || raw_h == 0 {
        return Err(bad("non-positive size"));
    }
    let top_down = raw_h < 0;
    let (w, h) = (width as u32, raw_h.unsigned_abs());
    check_dims(w, h)?;

    let (masks, mut data_off) = match (compression, bits) {
        (0, 24 | 32) => ([0x00FF_0000, 0x0000_FF00, 0x0000_00FF, 0xFF00_0000], header),
        (3, 32) => {
            // BI_BITFIELDS: masks live in the header (V4/V5) or right after a 40-byte one.
            let m = |o: usize| u32_at(dib, o).ok_or(bad("truncated masks"));
            let alpha = if header >= 108 { m(52)? } else { 0 };
            (
                [m(40)?, m(44)?, m(48)?, alpha],
                if header == 40 { header + 12 } else { header },
            )
        }
        _ => return Err(bad("only 24/32-bit uncompressed bitmaps are supported")),
    };
    // A colour table (rare for 24/32-bit) sits before the pixels.
    if let Some(used) = u32_at(dib, 32).filter(|&u| u > 0 && header == 40) {
        data_off += used as usize * 4;
    }

    let bytes_pp = usize::from(bits / 8);
    let stride = (w as usize * bytes_pp).div_ceil(4) * 4;
    let needed = data_off + stride * h as usize;
    if dib.len() < needed {
        return Err(bad("pixel data is shorter than the header claims"));
    }

    let shifts = masks.map(|m| if m == 0 { 0 } else { m.trailing_zeros() });
    let maxes = masks.map(|m| if m == 0 { 0 } else { m >> m.trailing_zeros() });
    // A zero mask means "channel absent": treat it as fully on.
    let scale = |v: u32, max: u32| {
        (v * 255 + max / 2)
            .checked_div(max)
            .map_or(255, |q| q as u8)
    };

    let mut out = vec![0u8; w as usize * h as usize * 4];
    let mut any_alpha = false;
    for y in 0..h as usize {
        let src_row = if top_down { y } else { h as usize - 1 - y };
        let row = &dib[data_off + src_row * stride..][..stride];
        for x in 0..w as usize {
            let px = &row[x * bytes_pp..];
            let v = if bytes_pp == 4 {
                u32::from_le_bytes([px[0], px[1], px[2], px[3]])
            } else {
                u32::from_le_bytes([px[0], px[1], px[2], 0])
            };
            let ch = |i: usize| scale((v & masks[i]) >> shifts[i], maxes[i]);
            let a = if masks[3] == 0 { 255 } else { ch(3) };
            any_alpha |= a != 0;
            let o = (y * w as usize + x) * 4;
            out[o..o + 4].copy_from_slice(&[ch(0), ch(1), ch(2), a]);
        }
    }
    // 32-bit BI_RGB bitmaps usually carry an unused alpha byte of zero.
    if !any_alpha || compression == 0 {
        for px in out.as_chunks_mut::<4>().0 {
            px[3] = 255;
        }
    }
    Ok(Rgba8Image {
        width: w,
        height: h,
        pixels: out,
    })
}

/// Build a `CF_DIB` blob (40-byte header, 32-bit `BI_RGB`, bottom-up BGRA) so
/// images can be pasted into apps that do not read the `PNG` clipboard format.
/// Alpha is carried in the fourth byte but many consumers ignore it.
#[must_use]
pub fn rgba_to_dib(img: &Rgba8Image) -> Vec<u8> {
    let (w, h) = (img.width as usize, img.height as usize);
    let mut d = Vec::with_capacity(40 + w * h * 4);
    d.extend_from_slice(&40u32.to_le_bytes());
    d.extend_from_slice(&(img.width as i32).to_le_bytes());
    d.extend_from_slice(&(img.height as i32).to_le_bytes()); // positive = bottom-up
    d.extend_from_slice(&1u16.to_le_bytes());
    d.extend_from_slice(&32u16.to_le_bytes());
    d.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
    d.extend_from_slice(&((w * h * 4) as u32).to_le_bytes());
    d.extend_from_slice(&[0u8; 16]); // resolution and palette counts
    for y in (0..h).rev() {
        for px in img.pixels[y * w * 4..(y + 1) * w * 4].as_chunks::<4>().0 {
            d.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
        }
    }
    d
}

pub fn encode_png(img: &Rgba8Image) -> Result<Vec<u8>, ImageError> {
    let mut buf = Vec::new();
    let mut enc = png::Encoder::new(&mut buf, img.width, img.height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc
        .write_header()
        .map_err(|e| ImageError::Png(e.to_string()))?;
    w.write_image_data(&img.pixels)
        .map_err(|e| ImageError::Png(e.to_string()))?;
    w.finish().map_err(|e| ImageError::Png(e.to_string()))?;
    Ok(buf)
}

/// Read width and height from the IHDR chunk without decoding pixels.
#[must_use]
pub fn png_dimensions(png: &[u8]) -> Option<(u32, u32)> {
    const SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if png.get(..8)? != SIG || png.get(12..16)? != b"IHDR" {
        return None;
    }
    let be = |o: usize| {
        png.get(o..o + 4)
            .map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
    };
    let (w, h) = (be(16)?, be(20)?);
    check_dims(w, h).ok().map(|()| (w, h))
}

/// Decode any 8/16-bit PNG to RGBA8.
pub fn decode_png(bytes: &[u8]) -> Result<Rgba8Image, ImageError> {
    let (w, h) = png_dimensions(bytes).ok_or(ImageError::Png("not a PNG".into()))?;
    let mut dec = png::Decoder::new(Cursor::new(bytes));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec
        .read_info()
        .map_err(|e| ImageError::Png(e.to_string()))?;
    let mut buf = vec![0u8; reader.output_buffer_size().ok_or(ImageError::TooLarge)?];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| ImageError::Png(e.to_string()))?;
    let src = &buf[..info.buffer_size()];
    let mut pixels = Vec::with_capacity(w as usize * h as usize * 4);
    match info.color_type {
        png::ColorType::Rgba => pixels.extend_from_slice(src),
        png::ColorType::Rgb => src
            .as_chunks::<3>()
            .0
            .iter()
            .for_each(|p| pixels.extend_from_slice(&[p[0], p[1], p[2], 255])),
        png::ColorType::GrayscaleAlpha => {
            src.as_chunks::<2>()
                .0
                .iter()
                .for_each(|p| pixels.extend_from_slice(&[p[0], p[0], p[0], p[1]]));
        }
        png::ColorType::Grayscale => src
            .iter()
            .for_each(|&g| pixels.extend_from_slice(&[g, g, g, 255])),
        png::ColorType::Indexed => return Err(ImageError::Png("palette was not expanded".into())),
    }
    Ok(Rgba8Image {
        width: info.width,
        height: info.height,
        pixels,
    })
}

/// Box-filter downscale so the longest side is at most `max_side` (never upscales).
#[must_use]
pub fn thumbnail(img: &Rgba8Image, max_side: u32) -> Rgba8Image {
    let longest = img.width.max(img.height);
    if longest <= max_side || max_side == 0 {
        return img.clone();
    }
    let nw = (u64::from(img.width) * u64::from(max_side) / u64::from(longest)).max(1) as u32;
    let nh = (u64::from(img.height) * u64::from(max_side) / u64::from(longest)).max(1) as u32;
    let mut out = vec![0u8; nw as usize * nh as usize * 4];
    for ny in 0..nh {
        let y0 = (u64::from(ny) * u64::from(img.height) / u64::from(nh)) as u32;
        let y1 = ((u64::from(ny + 1) * u64::from(img.height)).div_ceil(u64::from(nh)) as u32)
            .clamp(y0 + 1, img.height);
        for nx in 0..nw {
            let x0 = (u64::from(nx) * u64::from(img.width) / u64::from(nw)) as u32;
            let x1 = ((u64::from(nx + 1) * u64::from(img.width)).div_ceil(u64::from(nw)) as u32)
                .clamp(x0 + 1, img.width);
            let (mut acc, mut n, mut a_sum) = ([0u64; 3], 0u64, 0u64);
            for y in y0..y1 {
                for x in x0..x1 {
                    let o = (y as usize * img.width as usize + x as usize) * 4;
                    let a = u64::from(img.pixels[o + 3]);
                    // Alpha-weighted so transparent pixels do not darken edges.
                    for (c, sum) in acc.iter_mut().enumerate() {
                        *sum += u64::from(img.pixels[o + c]) * a;
                    }
                    a_sum += a;
                    n += 1;
                }
            }
            let o = (ny as usize * nw as usize + nx as usize) * 4;
            // Fully transparent block: leave the colour at zero.
            for (c, sum) in acc.iter().enumerate() {
                out[o + c] = sum.checked_div(a_sum).unwrap_or(0) as u8;
            }
            out[o + 3] = (a_sum / n) as u8;
        }
    }
    Rgba8Image {
        width: nw,
        height: nh,
        pixels: out,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dib24(w: i32, h: i32, rows_bgr: &[&[[u8; 3]]]) -> Vec<u8> {
        let mut d = Vec::new();
        d.extend_from_slice(&40u32.to_le_bytes());
        d.extend_from_slice(&w.to_le_bytes());
        d.extend_from_slice(&h.to_le_bytes());
        d.extend_from_slice(&1u16.to_le_bytes());
        d.extend_from_slice(&24u16.to_le_bytes());
        d.extend_from_slice(&[0; 24]);
        for row in rows_bgr {
            let start = d.len();
            for px in *row {
                d.extend_from_slice(px);
            }
            while (d.len() - start) % 4 != 0 {
                d.push(0);
            }
        }
        d
    }

    #[test]
    fn bottom_up_24_bit_dib_is_flipped_and_swizzled() {
        // stored bottom row first: [blue, white] then [red, green]
        let dib = dib24(
            2,
            2,
            &[&[[255, 0, 0], [255, 255, 255]], &[[0, 0, 255], [0, 255, 0]]],
        );
        let img = dib_to_rgba(&dib).unwrap();
        assert_eq!((img.width, img.height), (2, 2));
        assert_eq!(&img.pixels[0..4], &[255, 0, 0, 255], "top-left is red");
        assert_eq!(&img.pixels[4..8], &[0, 255, 0, 255], "top-right is green");
        assert_eq!(&img.pixels[8..12], &[0, 0, 255, 255], "bottom-left is blue");
    }

    #[test]
    fn top_down_dib_is_not_flipped() {
        let dib = dib24(1, -2, &[&[[0, 0, 255]], &[[255, 0, 0]]]);
        let img = dib_to_rgba(&dib).unwrap();
        assert_eq!(
            &img.pixels[0..4],
            &[255, 0, 0, 255],
            "first stored row is the top"
        );
    }

    #[test]
    fn plain_32_bit_dib_ignores_a_zero_alpha_byte() {
        let mut d = Vec::new();
        d.extend_from_slice(&40u32.to_le_bytes());
        d.extend_from_slice(&1i32.to_le_bytes());
        d.extend_from_slice(&1i32.to_le_bytes());
        d.extend_from_slice(&1u16.to_le_bytes());
        d.extend_from_slice(&32u16.to_le_bytes());
        d.extend_from_slice(&[0; 24]);
        d.extend_from_slice(&[10, 20, 30, 0]); // B G R unused
        assert_eq!(dib_to_rgba(&d).unwrap().pixels, vec![30, 20, 10, 255]);
    }

    #[test]
    fn malformed_dibs_are_errors_not_panics() {
        assert!(dib_to_rgba(&[]).is_err());
        assert!(dib_to_rgba(&[0; 39]).is_err());
        let mut short = dib24(4, 4, &[&[[1, 2, 3]; 4]]);
        short.truncate(60);
        assert!(
            dib_to_rgba(&short).is_err(),
            "pixel data shorter than claimed"
        );
        let huge = dib24(100_000, 100_000, &[]);
        assert_eq!(dib_to_rgba(&huge), Err(ImageError::TooLarge));
    }

    #[test]
    fn dib_export_round_trips_for_opaque_images() {
        let pixels: Vec<u8> = (0..3 * 2)
            .flat_map(|i| [i * 10, i * 20, i * 5, 255])
            .collect();
        let img = Rgba8Image {
            width: 3,
            height: 2,
            pixels,
        };
        assert_eq!(dib_to_rgba(&rgba_to_dib(&img)).unwrap(), img);
    }

    #[test]
    fn png_round_trip_and_header_probe() {
        let img = Rgba8Image {
            width: 3,
            height: 2,
            pixels: (0..24).collect(),
        };
        let png = encode_png(&img).unwrap();
        assert_eq!(png_dimensions(&png), Some((3, 2)));
        assert_eq!(decode_png(&png).unwrap(), img);
        assert_eq!(png_dimensions(b"not a png at all, sorry"), None);
        assert!(decode_png(b"nope").is_err());
    }

    #[test]
    fn thumbnails_keep_aspect_and_never_upscale() {
        let big = Rgba8Image {
            width: 400,
            height: 200,
            pixels: vec![200; 400 * 200 * 4],
        };
        let t = thumbnail(&big, 100);
        assert_eq!((t.width, t.height), (100, 50));
        assert!(
            t.pixels.iter().all(|&p| p == 200),
            "a flat image stays flat"
        );
        let small = Rgba8Image {
            width: 10,
            height: 10,
            pixels: vec![0; 400],
        };
        assert_eq!(thumbnail(&small, 100), small);
    }
}
