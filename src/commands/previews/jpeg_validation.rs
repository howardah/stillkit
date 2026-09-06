// The JPEG decoder tolerates an early EOI by filling missing blocks. For the
// optional thumbnail path, verify every baseline Huffman block is present before
// decoding pixels. Unsupported scans (including progressive JPEGs) use HEIC instead.
#[derive(Default, Clone)]
struct Huffman {
    lengths: [u8; 16],
    values: Vec<u8>,
}

impl Huffman {
    fn symbol(&self, bits: &mut Bits<'_>) -> Option<u8> {
        let (mut code, mut first, mut offset) = (0u32, 0u32, 0usize);
        for &count in &self.lengths {
            code = (code << 1) | bits.read(1)?;
            if code >= first && code - first < u32::from(count) {
                return self.values.get(offset + (code - first) as usize).copied();
            }
            offset += usize::from(count);
            first = (first + u32::from(count)) << 1;
        }
        None
    }
}

struct Bits<'a> {
    data: &'a [u8],
    position: usize,
    byte: u8,
    remaining: u8,
}

impl Bits<'_> {
    fn read(&mut self, count: u8) -> Option<u32> {
        let mut value = 0;
        for _ in 0..count {
            if self.remaining == 0 {
                self.byte = *self.data.get(self.position)?;
                self.position += 1;
                if self.byte == 0xff {
                    if *self.data.get(self.position)? != 0 {
                        return None;
                    }
                    self.position += 1;
                }
                self.remaining = 8;
            }
            self.remaining -= 1;
            value = (value << 1) | u32::from((self.byte >> self.remaining) & 1);
        }
        Some(value)
    }

    fn marker(&mut self, expected: u8) -> Option<()> {
        // Entropy-coded segments are padded with one bits to the next byte.
        let padding = (1u16 << self.remaining) - 1;
        if u16::from(self.byte) & padding != padding {
            return None;
        }
        self.remaining = 0;
        if self.data.get(self.position..self.position + 2)? != [0xff, expected] {
            return None;
        }
        self.position += 2;
        Some(())
    }
}

pub(super) fn complete_baseline(data: &[u8]) -> bool {
    validate(data).is_some()
}

fn validate(data: &[u8]) -> Option<()> {
    if data.get(..2)? != [0xff, 0xd8] {
        return None;
    }
    let mut position = 2;
    let mut tables = vec![Huffman::default(); 8];
    let mut frame = None;
    let mut restart_interval = 0;
    loop {
        if *data.get(position)? != 0xff {
            return None;
        }
        let marker = *data.get(position + 1)?;
        let length = usize::from(u16::from_be_bytes(
            data.get(position + 2..position + 4)?.try_into().ok()?,
        ));
        if length < 2 {
            return None;
        }
        let segment = data.get(position + 4..position + 2 + length)?;
        position += 2 + length;
        match marker {
            0xc0 if frame.is_none() => frame = Some(Frame::parse(segment)?),
            0xc4 => read_tables(segment, &mut tables)?,
            0xdd if segment.len() == 2 => {
                restart_interval = u16::from_be_bytes(segment.try_into().ok()?)
            }
            0xda => return frame?.scan(segment, data.get(position..)?, &tables, restart_interval),
            0xdb | 0xe0..=0xef | 0xfe => {}
            _ => return None,
        }
    }
}

fn read_tables(mut data: &[u8], tables: &mut [Huffman]) -> Option<()> {
    while !data.is_empty() {
        let kind = data[0];
        if kind >> 4 > 1 || kind & 15 > 3 {
            return None;
        }
        let lengths: [u8; 16] = data.get(1..17)?.try_into().ok()?;
        let count: usize = lengths.iter().map(|&n| usize::from(n)).sum();
        if count == 0 || count > 256 {
            return None;
        }
        let mut available = 1i32;
        for &length in &lengths {
            available = available * 2 - i32::from(length);
            if available < 0 {
                return None;
            }
        }
        tables[usize::from((kind >> 4) * 4 + (kind & 15))] = Huffman {
            lengths,
            values: data.get(17..17 + count)?.to_vec(),
        };
        data = data.get(17 + count..)?;
    }
    Some(())
}

struct Frame {
    width: u32,
    height: u32,
    components: Vec<(u8, u8, u8)>,
}

impl Frame {
    fn parse(data: &[u8]) -> Option<Self> {
        if *data.first()? != 8 {
            return None;
        }
        let height = u32::from(u16::from_be_bytes(data.get(1..3)?.try_into().ok()?));
        let width = u32::from(u16::from_be_bytes(data.get(3..5)?.try_into().ok()?));
        let count = usize::from(*data.get(5)?);
        if width == 0 || height == 0 || !(1..=3).contains(&count) || data.len() != 6 + 3 * count {
            return None;
        }
        let mut components = Vec::new();
        for component in data[6..].as_chunks::<3>().0 {
            let (h, v) = (component[1] >> 4, component[1] & 15);
            if !(1..=4).contains(&h)
                || !(1..=4).contains(&v)
                || components.iter().any(|&(id, _, _)| id == component[0])
            {
                return None;
            }
            components.push((component[0], h, v));
        }
        Some(Self {
            width,
            height,
            components,
        })
    }

    fn scan(&self, scan: &[u8], data: &[u8], tables: &[Huffman], restart: u16) -> Option<()> {
        let count = usize::from(*scan.first()?);
        if count != self.components.len()
            || scan.len() != 1 + count * 2 + 3
            || scan.get(1 + count * 2..)? != [0, 63, 0]
        {
            return None;
        }
        let mut blocks = Vec::new();
        let mut seen = Vec::new();
        for component in scan[1..1 + count * 2].as_chunks::<2>().0 {
            let &(id, h, v) = self
                .components
                .iter()
                .find(|&&(id, _, _)| id == component[0])?;
            if seen.contains(&id) || component[1] >> 4 > 3 || component[1] & 15 > 3 {
                return None;
            }
            seen.push(id);
            let repetitions = if count == 1 { 1 } else { h * v };
            for _ in 0..repetitions {
                blocks.push((
                    &tables[usize::from(component[1] >> 4)],
                    &tables[4 + usize::from(component[1] & 15)],
                ));
            }
        }
        let (h, v) = if count == 1 {
            (1, 1)
        } else {
            (
                self.components.iter().map(|c| u32::from(c.1)).max()?,
                self.components.iter().map(|c| u32::from(c.2)).max()?,
            )
        };
        let mcus = self.width.div_ceil(8 * h) * self.height.div_ceil(8 * v);
        let mut bits = Bits {
            data,
            position: 0,
            byte: 0,
            remaining: 0,
        };
        for mcu in 0..mcus {
            if restart != 0 && mcu != 0 && mcu % u32::from(restart) == 0 {
                bits.marker(0xd0 + ((mcu / u32::from(restart) - 1) % 8) as u8)?;
            }
            for &(dc, ac) in &blocks {
                block(dc, ac, &mut bits)?;
            }
        }
        bits.marker(0xd9)?;
        (bits.position == data.len()).then_some(())
    }
}

fn block(dc: &Huffman, ac: &Huffman, bits: &mut Bits<'_>) -> Option<()> {
    let category = dc.symbol(bits)?;
    if category > 11 {
        return None;
    }
    bits.read(category)?;
    let mut coefficient = 1;
    while coefficient < 64 {
        let symbol = ac.symbol(bits)?;
        if symbol == 0 {
            break;
        }
        let (run, size) = (symbol >> 4, symbol & 15);
        if size == 0 {
            if run != 15 {
                return None;
            }
            coefficient += 16;
        } else {
            if size > 10 {
                return None;
            }
            coefficient += usize::from(run) + 1;
            bits.read(size)?;
        }
        if coefficient > 64 {
            return None;
        }
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_complete_baseline_images_and_rejects_incomplete_scans() {
        for (width, height) in [(1, 1), (17, 31), (64, 64)] {
            let image = image::RgbImage::from_fn(width, height, |x, y| {
                image::Rgb([(x * 7) as u8, (y * 11) as u8, ((x + y) * 3) as u8])
            });
            let mut jpeg = Vec::new();
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90)
                .encode_image(&image)
                .unwrap();
            assert!(complete_baseline(&jpeg));
            let sos = jpeg.windows(2).position(|v| v == [0xff, 0xda]).unwrap();
            let start = sos + 2 + usize::from(u16::from_be_bytes([jpeg[sos + 2], jpeg[sos + 3]]));
            for end in [start, start + 1, (start + jpeg.len() - 2) / 2] {
                let mut truncated = jpeg[..end].to_vec();
                truncated.extend([0xff, 0xd9]);
                assert!(
                    !complete_baseline(&truncated),
                    "{width}x{height}, end={end}"
                );
            }
            assert!(!complete_baseline(&jpeg[..jpeg.len() - 1]));
            let sof = jpeg.windows(2).position(|v| v == [0xff, 0xc0]).unwrap();
            jpeg[sof + 1] = 0xc2;
            assert!(
                !complete_baseline(&jpeg),
                "progressive requires primary fallback"
            );
        }
    }

    #[test]
    fn entropy_reader_handles_stuffing_padding_and_restart_markers() {
        let mut bits = Bits {
            data: &[0xff, 0, 0xff, 0xd0, 0x7f, 0xff, 0xd9],
            position: 0,
            byte: 0,
            remaining: 0,
        };
        assert_eq!(bits.read(8), Some(255));
        assert_eq!(bits.marker(0xd0), Some(()));
        assert_eq!(bits.read(1), Some(0));
        assert_eq!(bits.marker(0xd9), Some(()));
        assert_eq!(bits.read(1), None);
    }

    #[test]
    fn malformed_headers_are_rejected_without_panicking() {
        for data in [
            b"".as_slice(),
            &[0xff, 0xd8],
            &[0xff, 0xd8, 0xff, 0xc4, 0, 1],
            &[0xff, 0xd8, 0xff, 0xc4, 255, 255],
        ] {
            assert!(!complete_baseline(data));
        }
    }
}
