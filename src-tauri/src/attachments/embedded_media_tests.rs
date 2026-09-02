use super::embedded_media::{
    has_isobmff_file_type_box, parse_motion_photo_layout, MotionPhotoLayout,
};

fn xmp(body: &str) -> Vec<u8> {
    format!(
        r#"binary-prefix
        <x:xmpmeta xmlns:x="adobe:ns:meta/">
          <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
            xmlns:Camera="http://ns.google.com/photos/1.0/camera/"
            xmlns:Container="http://ns.google.com/photos/1.0/container/"
            xmlns:Item="http://ns.google.com/photos/1.0/container/item/">
            {body}
          </rdf:RDF>
        </x:xmpmeta>
        binary-suffix"#
    )
    .into_bytes()
}

#[test]
fn parses_current_container_motion_photo_item() {
    let header = xmp(r#"
        <rdf:Description Camera:MotionPhoto="1">
          <Container:Directory><rdf:Seq><rdf:li>
            <Container:Item Item:Length="2048"
              Item:Mime="video/quicktime" Item:Semantic="MotionPhoto"/>
          </rdf:li></rdf:Seq></Container:Directory>
        </rdf:Description>"#);

    assert_eq!(
        parse_motion_photo_layout(&header, 8192),
        Some(MotionPhotoLayout {
            video_start: 6144,
            video_length: 2048,
            mime_type: "video/quicktime",
        })
    );
}

#[test]
fn parses_intermediate_motion_photo_offset() {
    let header = xmp(r#"<rdf:Description Camera:MotionPhotoOffset="4096"
          Camera:MotionPhoto="1"/>"#);

    assert_eq!(
        parse_motion_photo_layout(&header, 10_000),
        Some(MotionPhotoLayout {
            video_start: 5904,
            video_length: 4096,
            mime_type: "video/mp4",
        })
    );
}

#[test]
fn parses_legacy_micro_video_offset_used_by_xiaomi_samples() {
    let header = xmp(r#"<rdf:Description Camera:MicroVideo="1"
          Camera:MicroVideoOffset="1783722"/>"#);

    assert_eq!(
        parse_motion_photo_layout(&header, 2_640_710),
        Some(MotionPhotoLayout {
            video_start: 856_988,
            video_length: 1_783_722,
            mime_type: "video/mp4",
        })
    );
}

#[test]
fn explicit_motion_photo_disable_overrides_residual_container_metadata() {
    let header = xmp(r#"<rdf:Description Camera:MotionPhoto="0">
          <Container:Item Item:Semantic="MotionPhoto"
            Item:Mime="video/mp4" Item:Length="100"/>
        </rdf:Description>"#);

    assert_eq!(parse_motion_photo_layout(&header, 1000), None);
}

#[test]
fn ignores_disabled_legacy_metadata_and_invalid_lengths() {
    let disabled = xmp(r#"<rdf:Description Camera:MicroVideo="0"
          Camera:MicroVideoOffset="100"/>"#);
    assert_eq!(parse_motion_photo_layout(&disabled, 1000), None);

    let invalid = xmp(r#"<Container:Item Item:Semantic="MotionPhoto"
          Item:Mime="video/mp4" Item:Length="1000"/>"#);
    assert_eq!(parse_motion_photo_layout(&invalid, 1000), None);
}

#[test]
fn ignores_malformed_or_incomplete_xmp() {
    assert_eq!(parse_motion_photo_layout(b"plain jpeg bytes", 1000), None);
    assert_eq!(
        parse_motion_photo_layout(b"<x:xmpmeta><rdf:RDF>", 1000),
        None
    );
}

#[test]
fn validates_isobmff_file_type_box_at_video_start() {
    assert!(has_isobmff_file_type_box(b"\0\0\0\x18ftypmp42"));
    assert!(!has_isobmff_file_type_box(b"\0\0\0\x18moovmp42"));
    assert!(!has_isobmff_file_type_box(b"short"));
}
