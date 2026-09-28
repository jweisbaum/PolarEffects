//! Polar grids and the math over them.
//!
//! The polar grid type and its interpolation, the Expedition and Adrena
//! readers and writers, binning track samples into polar segments, blending
//! weighted sources, and the comparison surfaces (spec.md 6, 11, 12).
//!
//! The grid itself is plain data in `pe_core` ([`Polar`]), because an
//! imported polar is part of the document; everything it *does* lives here.
//!
//! The blend is derived, never stored (invariant 2): nothing here writes
//! project data, and export always recomputes from sources plus overlays.
//! Export bytes are pinned by golden files, because they must be identical on
//! every platform (invariant 5).

pub mod format;
pub mod grid;
pub mod orc;
pub mod source;

mod build;
mod expedition;
mod table;

pub use format::{PolarError, Reason, detect, read, read_as, write};
pub use grid::{cell_count, fold_twa, interpolate, resample};
pub use orc::vpp_to_polar;
pub use pe_core::polar::{PolarFileFormat, PolarGrid as Polar};
pub use source::{blend_input, source_polar};

/// The fastest a boat may go in an imported polar, knots (spec.md 6).
/// Anything faster is a format error, not a polar. The wind speed axis has
/// its own, higher bound: [`MAX_TWS_KN`].
pub const MAX_SPEED_KN: f64 = 60.0;

/// The fastest a wind speed axis value may be in an imported polar, knots
/// (spec.md 6). Real Adrena/ORC-style grids carry a TWS axis out to 70 kn
/// (a gale, not a boat speed), which [`MAX_SPEED_KN`] alone would refuse.
pub const MAX_TWS_KN: f64 = 70.0;
