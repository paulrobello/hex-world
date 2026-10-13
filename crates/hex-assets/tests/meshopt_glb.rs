use hex_assets::{GlbError, rewrite_glb};
use std::borrow::Cow;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn positions(glb: &[u8]) -> Vec<[f32; 3]> {
    let (doc, buffers, _) = gltf::import_slice(glb).expect("valid plain glTF");
    let mesh = doc.meshes().next().unwrap();
    let prim = mesh.primitives().next().unwrap();
    let reader = prim.reader(|b| Some(&buffers[b.index()]));
    reader.read_positions().unwrap().collect()
}

#[test]
fn plain_glb_passes_through_borrowed() {
    let plain = fixture("hex_test.plain.glb");
    let out = rewrite_glb(&plain).unwrap();
    assert!(matches!(out, Cow::Borrowed(_)));
    assert_eq!(&*out, &plain[..]);
}

#[test]
fn meshopt_glb_decodes_to_reference_geometry() {
    let packed = fixture("hex_test.meshopt.glb");
    assert!(String::from_utf8_lossy(&packed).contains("EXT_meshopt_compression"));
    assert!(!String::from_utf8_lossy(&packed).contains("KHR_mesh_quantization"));
    let out = rewrite_glb(&packed).unwrap();
    assert!(matches!(out, Cow::Owned(_)));
    assert!(!String::from_utf8_lossy(&out).contains("EXT_meshopt_compression"));
    assert_eq!(positions(&out), positions(&fixture("hex_test.plain.glb")));
}

#[test]
fn non_glb_is_rejected() {
    assert!(matches!(
        rewrite_glb(b"not a glb at all"),
        Err(GlbError::NotGlb)
    ));
}

#[test]
fn corrupt_meshopt_payload_errors_not_panics() {
    let mut packed = fixture("hex_test.meshopt.glb");
    let n = packed.len();
    for b in &mut packed[n - 256..] {
        *b = 0xA5;
    }
    assert!(rewrite_glb(&packed).is_err());
}

fn glb(json: &str, bin: &[u8]) -> Vec<u8> {
    let mut j = json.as_bytes().to_vec();
    while !j.len().is_multiple_of(4) {
        j.push(b' ');
    }
    let mut b = bin.to_vec();
    while !b.len().is_multiple_of(4) {
        b.push(0);
    }
    let total = (12 + 8 + j.len() + 8 + b.len()) as u32;
    let mut out = b"glTF".to_vec();
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&total.to_le_bytes());
    out.extend_from_slice(&(j.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&j);
    out.extend_from_slice(&(b.len() as u32).to_le_bytes());
    out.extend_from_slice(b"BIN\0");
    out.extend_from_slice(&b);
    out
}

#[test]
fn non_object_json_root_errors_not_panics() {
    for root in [
        r#"["EXT_meshopt_compression"]"#,
        r#""EXT_meshopt_compression""#,
    ] {
        assert!(
            matches!(rewrite_glb(&glb(root, &[])), Err(GlbError::BadJson(_))),
            "{root}"
        );
    }
}

#[test]
fn non_object_buffer_view_errors_not_panics() {
    let json = r#"{"extensionsUsed":["EXT_meshopt_compression"],"bufferViews":[7]}"#;
    assert!(matches!(
        rewrite_glb(&glb(json, &[])),
        Err(GlbError::BadJson(_))
    ));
}

#[test]
fn oversized_decode_hits_cap() {
    // 2^27 vertices x 4 bytes = 512 MiB, over the 256 MiB decode cap.
    let json = r#"{"extensionsUsed":["EXT_meshopt_compression"],"buffers":[{"byteLength":16}],
        "bufferViews":[{"buffer":0,"byteLength":536870912,"byteStride":4,
        "extensions":{"EXT_meshopt_compression":{"buffer":0,"byteOffset":0,"byteLength":16,
        "byteStride":4,"count":134217728,"mode":"ATTRIBUTES"}}}]}"#;
    match rewrite_glb(&glb(json, &[0xa0; 16])) {
        Err(GlbError::Decode(m)) => assert!(m.contains("cap"), "{m}"),
        other => panic!("expected cap error, got {other:?}"),
    }
}
