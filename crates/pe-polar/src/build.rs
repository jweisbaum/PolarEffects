//! Collects the points a reader finds into a [`PolarGrid`].
//!
//! Values are keyed by their canonical form (`pe_core::canonical`), so what
//! the reader builds is exactly what a saved project reads back, and axes come
//! out sorted whatever order the file used. Readers preserve port and
//! starboard angles; only the explicit symmetric display mode combines them.

use std::collections::{BTreeMap, BTreeSet};

use pe_core::canonical;
use pe_core::polar::PolarGrid;

use crate::format::{MAX_AXIS_VALUES, Reason};

/// The ordering key of a canonical, non-negative value: for such floats the
/// bit pattern sorts like the number.
fn key(value: f64) -> u64 {
    value.to_bits()
}

#[derive(Debug, Default)]
pub(crate) struct Builder {
    twa: BTreeSet<u64>,
    tws: BTreeSet<u64>,
    /// (twa, tws) → (sum, count).
    cells: BTreeMap<(u64, u64), (f64, u32)>,
}

impl Builder {
    /// Adds an angle (preserving its side) to the TWA axis; returns its key.
    pub(crate) fn twa(&mut self, twa: f64) -> Result<u64, Reason> {
        let k = key(canonical::degrees(twa));
        self.twa.insert(k);
        if self.twa.len() > MAX_AXIS_VALUES {
            return Err(Reason::TooManyValues);
        }
        Ok(k)
    }

    /// Adds a wind speed to the TWS axis; returns its key.
    pub(crate) fn tws(&mut self, tws: f64) -> Result<u64, Reason> {
        let k = key(canonical::knots(tws));
        self.tws.insert(k);
        if self.tws.len() > MAX_AXIS_VALUES {
            return Err(Reason::TooManyValues);
        }
        Ok(k)
    }

    /// Records a boat speed at keys returned by [`Self::twa`] and [`Self::tws`].
    pub(crate) fn cell(&mut self, twa: u64, tws: u64, bsp: f64) {
        let entry = self.cells.entry((twa, tws)).or_insert((0.0, 0));
        entry.0 += bsp;
        entry.1 += 1;
    }

    /// Whether any boat speed was recorded.
    pub(crate) fn has_cells(&self) -> bool {
        !self.cells.is_empty()
    }

    pub(crate) fn finish(self) -> PolarGrid {
        let twa: Vec<u64> = self.twa.into_iter().collect();
        let tws: Vec<u64> = self.tws.into_iter().collect();
        let bsp = twa
            .iter()
            .map(|a| {
                tws.iter()
                    .map(|s| {
                        self.cells
                            .get(&(*a, *s))
                            .map(|(sum, n)| canonical::knots(sum / f64::from(*n)))
                    })
                    .collect()
            })
            .collect();
        PolarGrid {
            twa: twa.into_iter().map(f64::from_bits).collect(),
            tws: tws.into_iter().map(f64::from_bits).collect(),
            bsp,
        }
    }
}
