use image::{DynamicImage, Rgb, RgbImage};

pub struct Thumbnail {
    pub data: Vec<u8>,
    pub dimensions: (u32, u32),
    pub linked: bool,
    pub rotation: Option<u8>,
}

impl Thumbnail {
    pub fn green(size: u32) -> Self {
        Self {
            data: jpeg(&DynamicImage::ImageRgb8(RgbImage::from_pixel(
                size,
                size,
                Rgb([0, 220, 0]),
            ))),
            dimensions: (size, size),
            linked: true,
            rotation: None,
        }
    }
}

pub fn jpeg(image: &DynamicImage) -> Vec<u8> {
    let mut data = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut data, 95)
        .encode_image(image)
        .unwrap();
    data
}

// Build a small HEIF with real primary HEVC pixels and independently controlled
// JPEG thumbnails. idat-relative extents avoid offsets into a changing meta box.
pub fn with_thumbnails(thumbnails: &[Thumbnail], rotation: Option<u8>) -> Vec<u8> {
    let source = include_bytes!("../fixtures/gradient-422-10bit.heic");
    let container = heic::heif::parse(source, &heic::Unstoppable).unwrap();
    let primary_data = container.get_item_data(container.primary_item_id).unwrap();
    let hvcc_start = source
        .windows(4)
        .position(|value| value == b"hvcC")
        .unwrap()
        - 4;
    let hvcc_size =
        u32::from_be_bytes(source[hvcc_start..hvcc_start + 4].try_into().unwrap()) as usize;
    let mut properties = spatial_extents((64, 64));
    properties.extend_from_slice(&source[hvcc_start..hvcc_start + hvcc_size]);
    let mut property_count = 2u8;
    let mut primary_associations = vec![1, 2];
    if let Some(angle) = rotation {
        properties.extend(atom(b"irot", &[angle]));
        property_count += 1;
        primary_associations.push(property_count);
    }
    let count = (thumbnails.len() + 2) as u16;
    let mut infos = vec![0, 0, 0, 0];
    infos.extend(count.to_be_bytes());
    infos.extend(item_info(1, b"hvc1"));
    let mut locations = vec![1, 0, 0, 0, 0x44, 0];
    locations.extend(count.to_be_bytes());
    locations.extend(location(1, 0, primary_data.len()));
    let mut associations = vec![0, 0, 0, 0];
    associations.extend(u32::from(count).to_be_bytes());
    associations.extend(association(1, &primary_associations));
    let mut references = vec![0, 0, 0, 0];
    let mut pixels = primary_data.into_owned();
    for (index, thumbnail) in thumbnails.iter().enumerate() {
        let id = index as u16 + 2;
        infos.extend(item_info(id, b"jpeg"));
        locations.extend(location(id, pixels.len(), thumbnail.data.len()));
        pixels.extend_from_slice(&thumbnail.data);
        properties.extend(spatial_extents(thumbnail.dimensions));
        property_count += 1;
        let mut indices = vec![property_count];
        if let Some(angle) = thumbnail.rotation {
            properties.extend(atom(b"irot", &[angle]));
            property_count += 1;
            indices.push(property_count);
        }
        associations.extend(association(id, &indices));
        if thumbnail.linked {
            let mut reference = id.to_be_bytes().to_vec();
            reference.extend([0, 1, 0, 1]);
            references.extend(atom(b"thmb", &reference));
        }
    }
    let mut tiff = std::io::Cursor::new(Vec::new());
    let artist = crate::common::field(
        exif::Tag::Artist,
        exif::Value::Ascii(vec![b"Primary photo".to_vec()]),
    );
    let mut writer = exif::experimental::Writer::new();
    writer.push_field(&artist);
    writer.write(&mut tiff, false).unwrap();
    let mut exif = vec![0, 0, 0, 0];
    exif.extend(tiff.into_inner());
    infos.extend(item_info(count, b"Exif"));
    locations.extend(location(count, pixels.len(), exif.len()));
    associations.extend(association(count, &[]));
    let mut reference = count.to_be_bytes().to_vec();
    reference.extend([0, 1, 0, 1]);
    references.extend(atom(b"cdsc", &reference));
    pixels.extend(exif);
    let mut iprp = atom(b"ipco", &properties);
    iprp.extend(atom(b"ipma", &associations));
    let mut meta = vec![0, 0, 0, 0];
    meta.extend(atom(b"pitm", &[0, 0, 0, 0, 0, 1]));
    meta.extend(atom(b"iinf", &infos));
    meta.extend(atom(b"iloc", &locations));
    meta.extend(atom(b"iprp", &iprp));
    meta.extend(atom(b"iref", &references));
    meta.extend(atom(b"idat", &pixels));
    let mut data = atom(b"ftyp", b"heic\0\0\0\0mif1heic");
    data.extend(atom(b"meta", &meta));
    data
}

fn atom(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut result = ((data.len() + 8) as u32).to_be_bytes().to_vec();
    result.extend(kind);
    result.extend(data);
    result
}

fn item_info(id: u16, kind: &[u8; 4]) -> Vec<u8> {
    let mut data = vec![2, 0, 0, 0];
    data.extend(id.to_be_bytes());
    data.extend([0, 0]);
    data.extend(kind);
    data.push(0);
    atom(b"infe", &data)
}

fn spatial_extents((w, h): (u32, u32)) -> Vec<u8> {
    let mut data = vec![0, 0, 0, 0];
    data.extend(w.to_be_bytes());
    data.extend(h.to_be_bytes());
    atom(b"ispe", &data)
}

fn location(id: u16, offset: usize, length: usize) -> Vec<u8> {
    let mut data = id.to_be_bytes().to_vec();
    data.extend([0, 1, 0, 0, 0, 1]);
    data.extend((offset as u32).to_be_bytes());
    data.extend((length as u32).to_be_bytes());
    data
}

fn association(id: u16, indices: &[u8]) -> Vec<u8> {
    let mut data = id.to_be_bytes().to_vec();
    data.push(indices.len() as u8);
    data.extend(indices);
    data
}
