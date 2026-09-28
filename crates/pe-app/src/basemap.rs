//! The bundled world basemap (spec.md 9.1). Copied from VectorEffects'
//! `ve-render::basemap`, with the same asset.
//!
//! Natural Earth land and coastlines at two levels of detail, converted at
//! build time by VectorEffects' `tools/basemap-builder` and committed as
//! `assets/basemap.bin`. The app ships its own basemap and never contacts a
//! tile server (invariant 4).
//!
//! The bytes are handed to the frontend unparsed: the renderer wants typed
//! arrays, so decoding here only to re-encode for the webview would be
//! wasted work. What this module provides is validation, which turns a
//! corrupt or truncated asset into a clear error at startup rather than a
//! blank map.

use crate::error::{AppError, Result};

/// The compiled-in basemap.
pub const EMBEDDED: &[u8] = include_bytes!("../../../assets/basemap.bin");

/// File magic. VectorEffects' name for the format, kept so the asset is the
/// same file byte for byte.
const MAGIC: &[u8; 4] = b"VEBM";
/// Format version this build understands.
pub const FORMAT_VERSION: u32 = 1;

/// Summary of one level of detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LodInfo {
    /// Natural Earth scale marker: 110 or 50.
    pub marker: u32,
    /// Number of triangle vertices.
    pub tri_vertices: u32,
    /// Number of triangle indices.
    pub tri_indices: u32,
    /// Number of coastline vertices.
    pub line_vertices: u32,
    /// Number of coastline rings.
    pub line_strips: u32,
}

/// What the asset contains, without decoding its geometry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BasemapInfo {
    /// Format version.
    pub version: u32,
    /// One entry per level of detail, coarsest first.
    pub lods: Vec<LodInfo>,
}

fn bad(message: String) -> AppError {
    AppError::Internal(format!("the bundled basemap is damaged: {message}"))
}

fn read_u32(bytes: &[u8], at: usize) -> Result<u32> {
    bytes
        .get(at..at + 4)
        .and_then(|s| <[u8; 4]>::try_from(s).ok())
        .map(u32::from_le_bytes)
        .ok_or_else(|| bad(format!("truncated at byte {at}")))
}

/// Validates the asset and reports what it holds. Every declared section is
/// bounds-checked against the actual length.
pub fn inspect(bytes: &[u8]) -> Result<BasemapInfo> {
    if bytes.get(..4) != Some(MAGIC.as_slice()) {
        return Err(bad("wrong magic".to_owned()));
    }
    let version = read_u32(bytes, 4)?;
    if version != FORMAT_VERSION {
        return Err(bad(format!(
            "format version {version}, expected {FORMAT_VERSION}"
        )));
    }
    let lod_count = read_u32(bytes, 8)?;
    if lod_count == 0 || lod_count > 8 {
        return Err(bad(format!("implausible level count {lod_count}")));
    }
    let mut pos = 12usize;
    let mut lods = Vec::with_capacity(lod_count as usize);
    for _ in 0..lod_count {
        let info = LodInfo {
            marker: read_u32(bytes, pos)?,
            tri_vertices: read_u32(bytes, pos + 4)?,
            tri_indices: read_u32(bytes, pos + 8)?,
            line_vertices: read_u32(bytes, pos + 12)?,
            line_strips: read_u32(bytes, pos + 16)?,
        };
        pos += 20;
        let payload = (info.tri_vertices as usize) * 8
            + (info.tri_indices as usize) * 4
            + (info.line_vertices as usize) * 8
            + (info.line_strips as usize) * 8;
        pos = pos
            .checked_add(payload)
            .ok_or_else(|| bad("section sizes overflow".to_owned()))?;
        if pos > bytes.len() {
            return Err(bad(format!(
                "level {} claims {payload} bytes past the end of a {}-byte file",
                info.marker,
                bytes.len()
            )));
        }
        lods.push(info);
    }
    if pos != bytes.len() {
        return Err(bad(format!("{} trailing bytes", bytes.len() - pos)));
    }
    Ok(BasemapInfo { version, lods })
}

/// The bundled basemap, as raw bytes.
#[tauri::command]
pub fn basemap() -> tauri::ipc::Response {
    tauri::ipc::Response::new(EMBEDDED.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shipped asset must be well formed, or the map is blank.
    #[test]
    fn the_embedded_asset_is_valid() {
        let info = inspect(EMBEDDED).expect("embedded basemap must parse");
        assert_eq!(info.version, FORMAT_VERSION);
        let markers: Vec<u32> = info.lods.iter().map(|l| l.marker).collect();
        assert_eq!(markers, vec![110, 50], "levels must be coarsest first");
        for lod in &info.lods {
            assert!(lod.tri_indices > 0 && lod.tri_indices % 3 == 0, "{lod:?}");
            assert!(lod.tri_vertices > 0 && lod.line_strips > 0, "{lod:?}");
        }
        assert!(info.lods[1].tri_indices > info.lods[0].tri_indices * 4);
    }

    #[test]
    fn a_truncated_asset_is_rejected() {
        assert!(inspect(&EMBEDDED[..EMBEDDED.len() / 2]).is_err());
    }

    #[test]
    fn wrong_magic_is_rejected() {
        assert!(inspect(b"NOPE1234").is_err());
        assert!(inspect(&[]).is_err());
    }

    #[test]
    fn a_future_version_is_rejected() {
        let mut bytes = EMBEDDED[..16].to_vec();
        bytes[4..8].copy_from_slice(&(FORMAT_VERSION + 1).to_le_bytes());
        assert!(inspect(&bytes).is_err());
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let mut bytes = EMBEDDED.to_vec();
        bytes.push(0);
        assert!(inspect(&bytes).is_err());
    }
}
