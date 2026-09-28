//! Polar grids and the math over them.
//!
//! The polar grid type and its interpolation, the Expedition and Adrena
//! readers and writers, binning track samples into polar segments, blending
//! weighted sources, and the comparison surfaces (spec.md 6, 11, 12).
//!
//! The blend is derived, never stored (invariant 2): nothing here writes
//! project data, and export always recomputes from sources plus overlays.
//! Export bytes are pinned by golden files, because they must be identical on
//! every platform (invariant 5).
//!
//! Filled in from milestone M4 (plan.md).
