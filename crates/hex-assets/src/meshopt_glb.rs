use meshoptimizer_rs::codec::{BufferView, Filter, Mode};
use meshoptimizer_rs::{Limits, Workspace};
use serde_json::{Map, Value};
use std::borrow::Cow;

#[derive(Debug)]
pub enum GlbError {
    NotGlb,
    BadJson(String),
    Decode(String),
}

impl std::fmt::Display for GlbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for GlbError {}

const EXT: &str = "EXT_meshopt_compression";
/// Cap on one decoded view and on the whole rewritten BIN (the GLB is untrusted input).
const MAX_DECODED: usize = 256 << 20;

fn u32_at(b: &[u8], o: usize) -> Result<u32, GlbError> {
    b.get(o..o + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or(GlbError::NotGlb)
}

/// `bin[off..off + len]`, with overflow-checked bounds (the JSON is untrusted).
fn slice<'a>(bin: &'a [u8], off: usize, len: usize, what: &str) -> Result<&'a [u8], GlbError> {
    off.checked_add(len)
        .and_then(|end| bin.get(off..end))
        .ok_or_else(|| GlbError::Decode(format!("{what} out of range")))
}

fn usize_field(v: &Map<String, Value>, key: &str) -> Result<Option<usize>, GlbError> {
    match v.get(key) {
        None => Ok(None),
        Some(x) => x
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .map(Some)
            .ok_or_else(|| GlbError::BadJson(format!("{key} is not an unsigned integer"))),
    }
}

fn required(v: &Map<String, Value>, key: &str) -> Result<usize, GlbError> {
    usize_field(v, key)?.ok_or_else(|| GlbError::BadJson(format!("missing {key}")))
}

/// Decodes EXT_meshopt_compression bufferViews into a plain GLB that bevy_gltf can load (TC3).
pub fn rewrite_glb(bytes: &[u8]) -> Result<Cow<'_, [u8]>, GlbError> {
    if bytes.len() < 20 || &bytes[0..4] != b"glTF" || u32_at(bytes, 4)? != 2 {
        return Err(GlbError::NotGlb);
    }
    let json_len = u32_at(bytes, 12)? as usize;
    if &bytes[16..20] != b"JSON" {
        return Err(GlbError::NotGlb);
    }
    let json_bytes = bytes
        .get(20..20usize.saturating_add(json_len))
        .ok_or(GlbError::NotGlb)?;
    if !json_bytes.windows(EXT.len()).any(|w| w == EXT.as_bytes()) {
        return Ok(Cow::Borrowed(bytes));
    }
    let mut json: Value =
        serde_json::from_slice(json_bytes).map_err(|e| GlbError::BadJson(e.to_string()))?;
    let bin_off = 20 + json_len;
    let bin = match bytes.get(bin_off..bin_off.saturating_add(8)) {
        Some(h) if &h[4..8] == b"BIN\0" => {
            let n = u32_at(bytes, bin_off)? as usize;
            slice(bytes, bin_off + 8, n, "BIN chunk").map_err(|_| GlbError::NotGlb)?
        }
        _ => &[][..],
    };

    let root = json
        .as_object_mut()
        .ok_or_else(|| GlbError::BadJson("root is not an object".into()))?;
    let mut workspace = Workspace::new(Limits {
        max_bytes: MAX_DECODED,
        ..Limits::default()
    });
    let mut out_bin: Vec<u8> = Vec::with_capacity(bin.len().saturating_mul(4).min(MAX_DECODED));
    let views = root
        .get_mut("bufferViews")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| GlbError::BadJson("no bufferViews".into()))?;
    for view in views.iter_mut() {
        let view = view
            .as_object_mut()
            .ok_or_else(|| GlbError::BadJson("bufferView is not an object".into()))?;
        let ext = match view.get("extensions").and_then(|e| e.get(EXT)) {
            None => None,
            Some(e) => Some(
                e.as_object()
                    .ok_or_else(|| GlbError::BadJson(format!("{EXT} is not an object")))?,
            ),
        };
        let data: Vec<u8> = if let Some(ext) = ext {
            if usize_field(ext, "buffer")?.unwrap_or(0) != 0 {
                return Err(GlbError::Decode(
                    "compressed data outside the GLB BIN".into(),
                ));
            }
            let mode = Mode::parse(ext.get("mode").and_then(Value::as_str).unwrap_or(""))
                .map_err(|e| GlbError::Decode(format!("mode: {e:?}")))?;
            let filter = Filter::parse(ext.get("filter").and_then(Value::as_str).unwrap_or("NONE"))
                .map_err(|e| GlbError::Decode(format!("filter: {e:?}")))?;
            let layout = BufferView::new(
                mode,
                filter,
                required(ext, "count")?,
                required(ext, "byteStride")?,
            )
            .map_err(|e| GlbError::Decode(format!("layout: {e:?}")))?;
            layout
                .validate_parent(
                    required(view, "byteLength")?,
                    usize_field(view, "byteStride")?,
                )
                .map_err(|e| GlbError::Decode(format!("parent view: {e:?}")))?;
            check_cap(out_bin.len(), layout.byte_length())?;
            let src = slice(
                bin,
                usize_field(ext, "byteOffset")?.unwrap_or(0),
                required(ext, "byteLength")?,
                "compressed view",
            )?;
            let decoded = layout
                .decode(src, &mut workspace)
                .map_err(|e| GlbError::Decode(format!("{e:?}")))?;
            let exts = view.get_mut("extensions").and_then(Value::as_object_mut);
            if exts.is_some_and(|o| {
                o.remove(EXT);
                o.is_empty()
            }) {
                view.remove("extensions");
            }
            decoded
        } else {
            if usize_field(view, "buffer")?.unwrap_or(0) != 0 {
                return Err(GlbError::Decode("plain view outside the GLB BIN".into()));
            }
            let len = required(view, "byteLength")?;
            check_cap(out_bin.len(), len)?;
            slice(
                bin,
                usize_field(view, "byteOffset")?.unwrap_or(0),
                len,
                "view",
            )?
            .to_vec()
        };
        while !out_bin.len().is_multiple_of(4) {
            out_bin.push(0);
        }
        view.insert("buffer".into(), 0.into());
        view.insert("byteOffset".into(), out_bin.len().into());
        view.insert("byteLength".into(), data.len().into());
        out_bin.extend_from_slice(&data);
    }
    // The meshopt fallback buffer (uri-less placeholder) is dropped: every view now lives in BIN.
    root.insert(
        "buffers".into(),
        serde_json::json!([{ "byteLength": out_bin.len() }]),
    );
    for key in ["extensionsUsed", "extensionsRequired"] {
        let empty = root.get_mut(key).and_then(Value::as_array_mut).map(|list| {
            list.retain(|v| v != EXT);
            list.is_empty()
        });
        if empty == Some(true) {
            root.remove(key);
        }
    }
    Ok(Cow::Owned(write_glb(&json, &out_bin)?))
}

fn check_cap(so_far: usize, add: usize) -> Result<(), GlbError> {
    if add > MAX_DECODED || so_far.saturating_add(add) > MAX_DECODED {
        return Err(GlbError::Decode(format!(
            "decoded size exceeds the {} MiB cap",
            MAX_DECODED >> 20
        )));
    }
    Ok(())
}

fn write_glb(json: &Value, bin: &[u8]) -> Result<Vec<u8>, GlbError> {
    let mut j = serde_json::to_vec(json).map_err(|e| GlbError::BadJson(e.to_string()))?;
    while !j.len().is_multiple_of(4) {
        j.push(b' ');
    }
    let mut b = bin.to_vec();
    while !b.len().is_multiple_of(4) {
        b.push(0);
    }
    let total = 12 + 8 + j.len() + 8 + b.len();
    let total32 = u32::try_from(total).map_err(|_| GlbError::Decode("GLB over 4 GiB".into()))?;
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&total32.to_le_bytes());
    out.extend_from_slice(&(j.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&j);
    out.extend_from_slice(&(b.len() as u32).to_le_bytes());
    out.extend_from_slice(b"BIN\0");
    out.extend_from_slice(&b);
    Ok(out)
}
