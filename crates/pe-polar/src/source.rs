//! A source's own polar, as the views draw it and as the blend will read it.
//!
//! An ORC certificate's grid is its VPP ([`crate::vpp_to_polar`]); a polar
//! file's is the grid it was parsed into. A track has no grid of its own —
//! its polar segment is binned from samples (spec.md 12.1, M13).
//!
//! Two readings, because the overlay means different things to each: the
//! views draw the source as it is, marking excluded nodes rather than hiding
//! them (spec.md 10.3: drawn as crosses), while the blend leaves an excluded
//! node's cell empty for that source (spec.md 10.3, 12.3).

use pe_core::source::{Source, SourceKind};

use crate::Polar;

/// The source's grid as imported, or `None` for a track.
pub fn source_polar(source: &Source) -> Option<Polar> {
    match &source.kind {
        SourceKind::Orc { record } => Some(crate::vpp_to_polar(&record.vpp)),
        SourceKind::PolarFile { polar, .. } => Some(polar.clone()),
        SourceKind::Track { .. } => None,
    }
}

/// The grid the blend reads for this source: its own, with every excluded
/// node's cell empty. `None` for a track. Removing every exclusion gives
/// [`source_polar`] back exactly (invariant 1).
pub fn blend_input(source: &Source) -> Option<Polar> {
    let mut polar = source_polar(source)?;
    let overlay = &source.overlay;
    if overlay.excluded_cells.is_empty() {
        return Some(polar);
    }
    for (i, twa) in polar.twa.iter().enumerate() {
        for (j, tws) in polar.tws.iter().enumerate() {
            if overlay.is_cell_excluded(*twa, *tws)
                && let Some(cell) = polar.bsp.get_mut(i).and_then(|row| row.get_mut(j))
            {
                *cell = None;
            }
        }
    }
    Some(polar)
}

#[cfg(test)]
mod tests {
    use pe_core::polar::{PolarFileFormat, PolarGrid};
    use pe_core::source::CellRef;
    use pe_core::{Colour, SourceId};

    use super::*;

    fn file_source() -> Source {
        Source::new(
            SourceId(1),
            "a.txt",
            Colour::parse("#4e79a7").unwrap(),
            SourceKind::PolarFile {
                format: PolarFileFormat::Expedition,
                file_name: "a.txt".to_owned(),
                polar: PolarGrid {
                    twa: vec![45.0, 90.0],
                    tws: vec![6.0, 12.0],
                    bsp: vec![vec![Some(5.5), Some(7.0)], vec![Some(6.5), Some(8.0)]],
                },
            },
        )
    }

    #[test]
    fn an_excluded_node_is_empty_for_the_blend_but_not_for_the_views() {
        let mut source = file_source();
        source.overlay.excluded_cells = vec![CellRef {
            twa: 90.0,
            tws: 6.0,
        }];
        let blend = blend_input(&source).unwrap();
        assert_eq!(
            blend.bsp,
            vec![vec![Some(5.5), Some(7.0)], vec![None, Some(8.0)]]
        );
        let drawn = source_polar(&source).unwrap();
        assert_eq!(drawn.bsp[1][0], Some(6.5));
    }

    #[test]
    fn without_exclusions_the_blend_reads_the_source_exactly() {
        let source = file_source();
        assert_eq!(blend_input(&source), source_polar(&source));
    }

    #[test]
    fn a_track_has_no_grid() {
        let origin = pe_core::track::TrackOrigin::File {
            name: "race.csv".to_owned(),
            boat_name: None,
        };
        let track = Source::new(
            SourceId(2),
            "Track",
            Colour::parse("#e15759").unwrap(),
            SourceKind::Track {
                track: Box::new(pe_core::track::Track::new(pe_core::TrackId(3), origin)),
            },
        );
        assert_eq!(source_polar(&track), None);
        assert_eq!(blend_input(&track), None);
    }
}
