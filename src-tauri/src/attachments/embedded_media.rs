use quick_xml::encoding::Decoder;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

pub(crate) const MOTION_PHOTO_XMP_PROBE_BYTES: u64 = 128 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MotionPhotoLayout {
    pub video_start: u64,
    pub video_length: u64,
    pub mime_type: &'static str,
}

#[derive(Default)]
struct MotionPhotoMetadata {
    motion_photo_enabled: Option<bool>,
    motion_photo_offset: Option<u64>,
    micro_video_enabled: Option<bool>,
    micro_video_offset: Option<u64>,
    container_video: Option<(u64, &'static str)>,
}

/// 从图片文件开头的 XMP 中定位尾部动态照片视频。
///
/// 当前 Motion Photo 规范使用 Container Item 的 Length；MotionPhotoOffset 和
/// MicroVideoOffset 是为了兼容仍在实际设备中出现的旧格式。这里只确定候选范围，
/// 调用端仍需读取候选起点并验证 ISOBMFF `ftyp` 签名，以排除编辑图片后残留的 XMP。
pub(crate) fn parse_motion_photo_layout(
    header: &[u8],
    total_size: u64,
) -> Option<MotionPhotoLayout> {
    if total_size == 0 {
        return None;
    }

    let xmp = find_xmp_packet(header)?;
    let metadata = parse_xmp_metadata(xmp)?;

    // 规范明确要求 MotionPhoto=0 时始终按普通图片处理，即使仍有尾部视频或残留目录。
    if metadata.motion_photo_enabled == Some(false) {
        return None;
    }

    if let Some((length, mime_type)) = metadata.container_video {
        if let Some(layout) = layout_from_tail_length(length, total_size, mime_type) {
            return Some(layout);
        }
    }

    if metadata.motion_photo_enabled == Some(true) {
        if let Some(layout) =
            layout_from_tail_length(metadata.motion_photo_offset?, total_size, "video/mp4")
        {
            return Some(layout);
        }
    }

    if metadata.micro_video_enabled == Some(true) {
        return layout_from_tail_length(metadata.micro_video_offset?, total_size, "video/mp4");
    }

    None
}

pub(crate) fn has_isobmff_file_type_box(bytes: &[u8]) -> bool {
    bytes.len() >= 12 && &bytes[4..8] == b"ftyp"
}

fn parse_xmp_metadata(xmp: &[u8]) -> Option<MotionPhotoMetadata> {
    let mut reader = Reader::from_reader(xmp);
    reader.config_mut().trim_text(true);
    let mut metadata = MotionPhotoMetadata::default();

    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) | Ok(Event::Empty(element)) => {
                inspect_xmp_element(&element, reader.decoder(), &mut metadata);
            }
            Ok(Event::Eof) => break,
            Err(_) => return None,
            _ => {}
        }
    }

    Some(metadata)
}

fn inspect_xmp_element(
    element: &BytesStart<'_>,
    decoder: Decoder,
    metadata: &mut MotionPhotoMetadata,
) {
    let qualified_name = element.name();
    let element_name = local_name(qualified_name.as_ref());
    if element_name.eq_ignore_ascii_case(b"Item") {
        let semantic = attribute_value(element, decoder, b"Semantic");
        if !semantic
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case("MotionPhoto"))
        {
            return;
        }

        let Some(mime_type) = attribute_value(element, decoder, b"Mime")
            .as_deref()
            .and_then(supported_video_mime)
        else {
            return;
        };
        let Some(length) = attribute_value(element, decoder, b"Length")
            .as_deref()
            .and_then(parse_positive_u64)
        else {
            return;
        };
        metadata.container_video = Some((length, mime_type));
        return;
    }

    for attribute in element.attributes().with_checks(false).flatten() {
        let name = local_name(attribute.key.as_ref());
        let Ok(value) = attribute.decode_and_unescape_value(decoder) else {
            continue;
        };
        match name {
            name if name.eq_ignore_ascii_case(b"MotionPhoto") => {
                metadata.motion_photo_enabled = Some(value.as_ref() == "1");
            }
            name if name.eq_ignore_ascii_case(b"MotionPhotoOffset") => {
                metadata.motion_photo_offset = parse_positive_u64(value.as_ref());
            }
            name if name.eq_ignore_ascii_case(b"MicroVideo") => {
                metadata.micro_video_enabled = Some(value.as_ref() == "1");
            }
            name if name.eq_ignore_ascii_case(b"MicroVideoOffset") => {
                metadata.micro_video_offset = parse_positive_u64(value.as_ref());
            }
            _ => {}
        }
    }
}

fn attribute_value(element: &BytesStart<'_>, decoder: Decoder, wanted: &[u8]) -> Option<String> {
    element
        .attributes()
        .with_checks(false)
        .flatten()
        .find(|attribute| local_name(attribute.key.as_ref()).eq_ignore_ascii_case(wanted))?
        .decode_and_unescape_value(decoder)
        .ok()
        .map(|value| value.into_owned())
}

fn find_xmp_packet(header: &[u8]) -> Option<&[u8]> {
    for (start_marker, end_marker) in [
        (b"<x:xmpmeta".as_slice(), b"</x:xmpmeta>".as_slice()),
        (b"<rdf:RDF".as_slice(), b"</rdf:RDF>".as_slice()),
    ] {
        let Some(start) = find_subslice(header, start_marker) else {
            continue;
        };
        let remaining = &header[start..];
        let end = find_subslice(remaining, end_marker)? + end_marker.len();
        return Some(&remaining[..end]);
    }
    None
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    (!needle.is_empty())
        .then(|| {
            haystack
                .windows(needle.len())
                .position(|window| window == needle)
        })
        .flatten()
}

fn local_name(name: &[u8]) -> &[u8] {
    name.rsplit(|byte| *byte == b':').next().unwrap_or(name)
}

fn parse_positive_u64(value: &str) -> Option<u64> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse::<u64>().ok().filter(|value| *value > 0)
}

fn supported_video_mime(value: &str) -> Option<&'static str> {
    if value.eq_ignore_ascii_case("video/mp4") {
        Some("video/mp4")
    } else if value.eq_ignore_ascii_case("video/quicktime") {
        Some("video/quicktime")
    } else {
        None
    }
}

fn layout_from_tail_length(
    video_length: u64,
    total_size: u64,
    mime_type: &'static str,
) -> Option<MotionPhotoLayout> {
    if video_length == 0 || video_length >= total_size {
        return None;
    }
    Some(MotionPhotoLayout {
        video_start: total_size - video_length,
        video_length,
        mime_type,
    })
}
