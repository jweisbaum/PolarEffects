//! GRIB2 message assembly, ported from VectorEffects' `ve-grib::writer`
//! with regional grids added (spec.md 7.8, D14).
//!
//! Each message is built from fixed section bytes with only the varying
//! fields patched in: the reference time, the forecast hour, the grid
//! corners, the parameter and the data. Sections 0–8, templates 3.0
//! (regular lat/lon), 4.0 (analysis or forecast at a level) and 5.0 (simple
//! packing, 16 bits); a bitmap (section 6) only where a value is missing.
//!
//! # Traps this code exists to get right
//!
//! **Signed integers are sign-magnitude, not two's complement.** A latitude
//! of −50° is `0x80000000 | 50_000_000`, not `-50_000_000` as an `i32`. Get
//! this wrong and decoders report a grid near the north pole with a
//! nonsensical extent.
//!
//! **Longitudes are 0–360 here.** Scanning mode 0 runs west to east from
//! `Lo1` and north to south from `La1`. `Lo1` and `Lo2` are written in
//! [0°, 360°): a box across the prime meridian (348.5°E to 0.75°E) has
//! `Lo2 < Lo1`, and a box across the antimeridian (168°E to 192°E) has
//! `Lo2` = 192°. ecCodes and wgrib2 both read either as the eastward run
//! from `Lo1`. Everything else in the application works in [−180, 180);
//! [`GridSpec::points`] is where the two meet.
//!
//! **The time convention is VectorEffects':** the reference time is the
//! first hour written, and each message carries its offset from it in hours
//! as a forecast hour (significance of reference time 1, "start of
//! forecast"). A reader that lists forecast steps sees the export as one
//! run of hourly steps, which is how routing software loads a GRIB.

use std::io::Write;

use crate::error::{GribError, Result};
use crate::packing::{self, Packed};

/// Missing-value marker for a one-octet field.
const MISSING_U8: u8 = 0xff;
/// Missing-value marker for a four-octet field.
const MISSING_U32: u32 = 0xffff_ffff;
/// A full turn, micro-degrees.
const TURN: u32 = 360_000_000;

/// Which field a message carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Parameter {
    /// Eastward wind at 10 m. Discipline 0, UGRD.
    WindU,
    /// Northward wind at 10 m. Discipline 0, VGRD.
    WindV,
    /// Significant height of combined wind waves and swell, metres.
    /// Discipline 10, HTSGW.
    WaveHeight,
    /// Mean wave direction, degrees true, "from". Discipline 10, category
    /// 0, parameter 14 (direction of combined wind waves and swell): ERA5's
    /// `mwd`, as ecCodes names it.
    WaveDirection,
    /// Eastward surface current. Discipline 10, UOGRD.
    CurrentU,
    /// Northward surface current. Discipline 10, VOGRD.
    CurrentV,
}

impl Parameter {
    /// GRIB2 discipline.
    pub fn discipline(self) -> u8 {
        match self {
            Self::WindU | Self::WindV => 0,
            Self::WaveHeight | Self::WaveDirection | Self::CurrentU | Self::CurrentV => 10,
        }
    }

    /// Parameter category within the discipline.
    pub fn category(self) -> u8 {
        match self {
            // Momentum.
            Self::WindU | Self::WindV => 2,
            // Waves.
            Self::WaveHeight | Self::WaveDirection => 0,
            // Currents.
            Self::CurrentU | Self::CurrentV => 1,
        }
    }

    /// Parameter number within the category.
    pub fn number(self) -> u8 {
        match self {
            Self::WindU | Self::CurrentU => 2,
            Self::WindV | Self::CurrentV | Self::WaveHeight => 3,
            Self::WaveDirection => 14,
        }
    }

    /// `(type of fixed surface, scale factor, scaled value)`.
    pub fn surface(self) -> (u8, u8, u32) {
        match self {
            // 103: specified height above ground, 10 m.
            Self::WindU | Self::WindV => (103, 0, 10),
            // 1: ground or water surface.
            Self::WaveHeight | Self::WaveDirection => (1, MISSING_U8, MISSING_U32),
            // 160: depth below sea surface, 0 m.
            Self::CurrentU | Self::CurrentV => (160, 0, 0),
        }
    }

    /// Short name, for logs and test output (ecCodes' names).
    pub fn short_name(self) -> &'static str {
        match self {
            Self::WindU => "10u",
            Self::WindV => "10v",
            Self::WaveHeight => "swh",
            Self::WaveDirection => "mwd",
            Self::CurrentU => "ucurr",
            Self::CurrentV => "vcurr",
        }
    }
}

/// A regular latitude/longitude grid, north to south and west to east.
///
/// Corners in micro-degrees: `la1` the northern row, `lo1` the western
/// column in [0, 360°). The grid need not go round; when it does (`ni`
/// columns of `step` make a full turn), it starts wherever `lo1` says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridSpec {
    /// Points along a parallel.
    pub ni: u32,
    /// Points along a meridian.
    pub nj: u32,
    /// Latitude of the first (northern) row, micro-degrees.
    pub la1: i32,
    /// Longitude of the first (western) column, micro-degrees in [0, 360°).
    pub lo1: u32,
    /// Grid spacing in micro-degrees, both ways.
    pub step: u32,
}

impl GridSpec {
    /// A global grid from 90°N and 0°E.
    pub fn global(step: u32) -> Self {
        Self {
            ni: TURN / step,
            nj: 180_000_000 / step + 1,
            la1: 90_000_000,
            lo1: 0,
            step,
        }
    }

    /// Total grid points.
    pub fn point_count(self) -> u64 {
        u64::from(self.ni) * u64::from(self.nj)
    }

    /// Latitude of the last (southern) row, micro-degrees.
    pub fn la2(self) -> i32 {
        let span = i64::from(self.nj.saturating_sub(1)) * i64::from(self.step);
        (i64::from(self.la1) - span) as i32
    }

    /// Longitude of the last (eastern) column, micro-degrees in [0, 360°).
    pub fn lo2(self) -> u32 {
        let span = u64::from(self.ni.saturating_sub(1)) * u64::from(self.step);
        ((u64::from(self.lo1) + span) % u64::from(TURN)) as u32
    }

    /// Latitude of row `j`, degrees.
    pub fn lat(self, j: u32) -> f64 {
        (i64::from(self.la1) - i64::from(j) * i64::from(self.step)) as f64 / 1e6
    }

    /// Longitude of column `i`, degrees in [−180, 180).
    pub fn lon(self, i: u32) -> f64 {
        let micro = (u64::from(self.lo1) + u64::from(i) * u64::from(self.step)) % u64::from(TURN);
        let deg = micro as f64 / 1e6;
        if deg >= 180.0 { deg - 360.0 } else { deg }
    }

    /// Grid positions in GRIB scanning order, `(longitude, latitude)`.
    ///
    /// West to east from `lo1`, north to south from `la1`. Longitudes come
    /// back in [−180, 180) for sampling, but the *order* is the file's.
    pub fn points(self) -> impl Iterator<Item = (f64, f64)> {
        (0..self.nj).flat_map(move |j| {
            let lat = self.lat(j).clamp(-90.0, 90.0);
            (0..self.ni).map(move |i| (self.lon(i), lat))
        })
    }

    /// Checks the grid fits the template: within the poles, `lo1` in
    /// [0, 360°), at most a turn of columns, and a spacing.
    pub fn validate(self) -> Result<()> {
        let ok = self.step > 0
            && self.ni > 0
            && self.nj > 0
            && self.lo1 < TURN
            && u64::from(self.ni) * u64::from(self.step) <= u64::from(TURN)
            && self.la1 <= 90_000_000
            && self.la2() >= -90_000_000
            && self.point_count() <= u64::from(u32::MAX);
        if ok {
            Ok(())
        } else {
            Err(GribError::UnsupportedGrid(format!("{self:?}")))
        }
    }
}

/// Reference time, as GRIB2 stores it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReferenceTime {
    /// Four-digit year.
    pub year: u16,
    /// Month, 1-12.
    pub month: u8,
    /// Day, 1-31.
    pub day: u8,
    /// Hour, 0-23.
    pub hour: u8,
    /// Minute, 0-59.
    pub minute: u8,
    /// Second, 0-59.
    pub second: u8,
}

impl ReferenceTime {
    /// The UTC calendar time of `t`, epoch seconds.
    ///
    /// # Errors
    /// [`GribError::UnsupportedGrid`] for a year GRIB2 cannot hold.
    pub fn from_epoch(t: i64) -> Result<Self> {
        let days = t.div_euclid(86_400);
        let secs = t.rem_euclid(86_400);
        let (year, month, day) = civil_from_days(days);
        let year = u16::try_from(year)
            .map_err(|_| GribError::UnsupportedGrid(format!("the year {year}")))?;
        Ok(Self {
            year,
            month,
            day,
            hour: (secs / 3600) as u8,
            minute: (secs % 3600 / 60) as u8,
            second: (secs % 60) as u8,
        })
    }

    /// Checks the fields are in range.
    pub fn validate(self) -> Result<()> {
        let ok = (1..=12).contains(&self.month)
            && (1..=31).contains(&self.day)
            && self.hour < 24
            && self.minute < 60
            && self.second < 60;
        if ok {
            Ok(())
        } else {
            Err(GribError::UnsupportedGrid(format!(
                "invalid reference time {self:?}"
            )))
        }
    }
}

/// Year, month and day of a day count since 1970-01-01 (proleptic
/// Gregorian; Howard Hinnant's `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, u8, u8) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// Everything a single message needs beyond its values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageSpec {
    /// Which field.
    pub parameter: Parameter,
    /// The grid.
    pub grid: GridSpec,
    /// Forecast reference time: the first hour of the export.
    pub reference_time: ReferenceTime,
    /// Hours from the reference time.
    pub forecast_hour: u32,
    /// Originating centre. 255 means missing.
    pub centre: u16,
    /// Bits per packed value; [`packing::BITS_PER_VALUE`].
    pub bits: u8,
}

// --- Byte helpers -----------------------------------------------------------

fn put_u8(out: &mut Vec<u8>, value: u8) {
    out.push(value);
}

fn put_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_be_bytes());
}

/// Writes a signed 16-bit value in GRIB2's sign-magnitude form.
fn put_i16_sm(out: &mut Vec<u8>, value: i16) {
    let magnitude = value.unsigned_abs();
    let sign = if value < 0 { 0x8000 } else { 0 };
    put_u16(out, sign | magnitude);
}

/// Writes a signed 32-bit value in GRIB2's sign-magnitude form.
fn put_i32_sm(out: &mut Vec<u8>, value: i32) {
    let magnitude = value.unsigned_abs();
    let sign = if value < 0 { 0x8000_0000 } else { 0 };
    put_u32(out, sign | magnitude);
}

/// Reads GRIB2 sign-magnitude back. Used by the in-test reader.
pub fn from_i32_sm(raw: u32) -> i32 {
    let magnitude = (raw & 0x7fff_ffff) as i32;
    if raw & 0x8000_0000 != 0 {
        -magnitude
    } else {
        magnitude
    }
}

/// Reads a 16-bit sign-magnitude value.
pub fn from_i16_sm(raw: u16) -> i16 {
    let magnitude = (raw & 0x7fff) as i16;
    if raw & 0x8000 != 0 {
        -magnitude
    } else {
        magnitude
    }
}

// --- Sections ---------------------------------------------------------------

fn section1(spec: &MessageSpec) -> Vec<u8> {
    let mut out = Vec::with_capacity(21);
    put_u32(&mut out, 21);
    put_u8(&mut out, 1);
    put_u16(&mut out, spec.centre);
    put_u16(&mut out, 0); // sub-centre
    put_u8(&mut out, 2); // master tables version
    put_u8(&mut out, 0); // local tables version
    put_u8(&mut out, 1); // significance of reference time: start of forecast
    put_u16(&mut out, spec.reference_time.year);
    put_u8(&mut out, spec.reference_time.month);
    put_u8(&mut out, spec.reference_time.day);
    put_u8(&mut out, spec.reference_time.hour);
    put_u8(&mut out, spec.reference_time.minute);
    put_u8(&mut out, spec.reference_time.second);
    put_u8(&mut out, 2); // production status: research
    put_u8(&mut out, 1); // type of data: forecast
    out
}

/// Human-readable provenance stored in every message's local-use section.
pub const PROVENANCE: &str = "Created with PolarExplorer";

/// GRIB2 Section 2: four-byte length, section number, then local-use octets.
fn section2() -> Vec<u8> {
    let mut out = Vec::with_capacity(5 + PROVENANCE.len());
    put_u32(&mut out, (5 + PROVENANCE.len()) as u32);
    put_u8(&mut out, 2);
    out.extend_from_slice(PROVENANCE.as_bytes());
    out
}

fn section3(grid: GridSpec) -> Vec<u8> {
    let mut out = Vec::with_capacity(72);
    put_u32(&mut out, 72);
    put_u8(&mut out, 3);
    put_u8(&mut out, 0); // grid definition from template
    put_u32(&mut out, grid.point_count() as u32);
    put_u8(&mut out, 0); // no optional point list
    put_u8(&mut out, 0); // no list interpretation
    put_u16(&mut out, 0); // template 3.0: regular lat/lon

    // Shape of earth 6 implies a radius of 6,371,229 m, the sphere of ERA5
    // and of the application (CLAUDE.md conventions), so the radius fields
    // are left missing rather than restated.
    put_u8(&mut out, 6);
    put_u8(&mut out, MISSING_U8);
    put_u32(&mut out, MISSING_U32);
    put_u8(&mut out, MISSING_U8);
    put_u32(&mut out, MISSING_U32);
    put_u8(&mut out, MISSING_U8);
    put_u32(&mut out, MISSING_U32);

    put_u32(&mut out, grid.ni);
    put_u32(&mut out, grid.nj);
    put_u32(&mut out, 0); // basic angle: 0 means units of 1e-6 degrees
    put_u32(&mut out, MISSING_U32); // subdivisions of the basic angle

    put_i32_sm(&mut out, grid.la1);
    put_i32_sm(&mut out, grid.lo1 as i32);
    // 0x30: i and j increments are given, and u/v are earth-relative rather
    // than grid-relative. Without the latter, consumers rotate the vectors.
    put_u8(&mut out, 0x30);
    put_i32_sm(&mut out, grid.la2());
    put_i32_sm(&mut out, grid.lo2() as i32);
    put_u32(&mut out, grid.step); // Di
    put_u32(&mut out, grid.step); // Dj
    put_u8(&mut out, 0x00); // scanning mode: +i, -j, i consecutive
    out
}

fn section4(spec: &MessageSpec) -> Vec<u8> {
    let (surface_type, surface_scale, surface_value) = spec.parameter.surface();
    let mut out = Vec::with_capacity(34);
    put_u32(&mut out, 34);
    put_u8(&mut out, 4);
    put_u16(&mut out, 0); // no optional coordinate values
    put_u16(&mut out, 0); // template 4.0

    put_u8(&mut out, spec.parameter.category());
    put_u8(&mut out, spec.parameter.number());
    put_u8(&mut out, 2); // generating process: forecast
    put_u8(&mut out, 0); // background process
    put_u8(&mut out, 0); // generating process identifier
    put_u16(&mut out, 0); // hours of observational cutoff
    put_u8(&mut out, 0); // minutes of cutoff
    put_u8(&mut out, 1); // unit of time range: hour
    put_u32(&mut out, spec.forecast_hour);

    put_u8(&mut out, surface_type);
    put_u8(&mut out, surface_scale);
    put_u32(&mut out, surface_value);
    put_u8(&mut out, MISSING_U8); // no second surface
    put_u8(&mut out, MISSING_U8);
    put_u32(&mut out, MISSING_U32);
    out
}

fn section5(packed: &Packed) -> Vec<u8> {
    let mut out = Vec::with_capacity(21);
    put_u32(&mut out, 21);
    put_u8(&mut out, 5);
    put_u32(&mut out, packed.count as u32);
    put_u16(&mut out, 0); // template 5.0: simple packing
    out.extend_from_slice(&packed.reference.to_be_bytes()); // IEEE 32-bit
    put_i16_sm(&mut out, packed.binary_scale);
    put_i16_sm(&mut out, packed.decimal_scale);
    put_u8(&mut out, packed.bits);
    put_u8(&mut out, 0); // original values were floating point
    out
}

fn section6() -> Vec<u8> {
    let mut out = Vec::with_capacity(6);
    put_u32(&mut out, 6);
    put_u8(&mut out, 6);
    put_u8(&mut out, 255); // no bitmap: every point has a value
    out
}

/// Section 6 carrying a bitmap: one bit per grid node, set where a value is
/// written. Waves and currents have none over land, and an hour an archive
/// does not have has none anywhere: such a node must arrive as *missing*,
/// not as calm.
fn section6_bitmap(present: &[bool]) -> Vec<u8> {
    let bytes = present.len().div_ceil(8);
    let mut out = Vec::with_capacity(6 + bytes);
    put_u32(&mut out, (6 + bytes) as u32);
    put_u8(&mut out, 6);
    put_u8(&mut out, 0); // a bitmap follows
    // Most significant bit first, in scanning order, padded with zeros.
    for chunk in present.chunks(8) {
        let mut byte = 0u8;
        for (bit, &here) in chunk.iter().enumerate() {
            if here {
                byte |= 0x80 >> bit;
            }
        }
        out.push(byte);
    }
    out
}

fn section7(packed: &Packed) -> Vec<u8> {
    let mut out = Vec::with_capacity(5 + packed.data.len());
    put_u32(&mut out, (5 + packed.data.len()) as u32);
    put_u8(&mut out, 7);
    out.extend_from_slice(&packed.data);
    out
}

/// Builds one complete GRIB2 message.
///
/// A non-finite value is a node the field says nothing about: it is left
/// out of the data section and marked absent in a bitmap. A field with no
/// holes has no bitmap (section 6 says 255), so the common case is the
/// smallest.
///
/// # Errors
/// An invalid grid or reference time, or a value count that does not match
/// the grid.
pub fn message(spec: &MessageSpec, values: &[f32]) -> Result<Vec<u8>> {
    spec.reference_time.validate()?;
    spec.grid.validate()?;
    if values.len() as u64 != spec.grid.point_count() {
        return Err(GribError::UnsupportedGrid(format!(
            "{} values for a {}x{} grid",
            values.len(),
            spec.grid.ni,
            spec.grid.nj
        )));
    }
    let whole = values.iter().all(|v| v.is_finite());
    let (packed, bitmap) = if whole {
        (packing::pack(values, spec.bits)?, section6())
    } else {
        let present: Vec<bool> = values.iter().map(|v| v.is_finite()).collect();
        let written: Vec<f32> = values.iter().copied().filter(|v| v.is_finite()).collect();
        (
            packing::pack(&written, spec.bits)?,
            section6_bitmap(&present),
        )
    };
    let body = [
        section1(spec),
        section2(),
        section3(spec.grid),
        section4(spec),
        section5(&packed),
        bitmap,
        section7(&packed),
    ]
    .concat();

    let total = 16 + body.len() + 4;
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"GRIB");
    put_u16(&mut out, 0); // reserved
    put_u8(&mut out, spec.parameter.discipline());
    put_u8(&mut out, 2); // edition 2
    out.extend_from_slice(&(total as u64).to_be_bytes());
    out.extend_from_slice(&body);
    out.extend_from_slice(b"7777");
    Ok(out)
}

/// Appends one message to a writer, returning its length.
///
/// Messages are written as they are produced rather than assembled into
/// one buffer: an ocean race is hundreds of megabytes, which must never be
/// held in memory at once.
///
/// # Errors
/// As [`message`], or the write failing.
pub fn write_message<W: Write>(out: &mut W, spec: &MessageSpec, values: &[f32]) -> Result<usize> {
    let bytes = message(spec, values)?;
    out.write_all(&bytes)?;
    Ok(bytes.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid() -> GridSpec {
        GridSpec::global(1_000_000)
    }

    fn spec() -> MessageSpec {
        MessageSpec {
            parameter: Parameter::WindU,
            grid: grid(),
            reference_time: ReferenceTime {
                year: 2026,
                month: 9,
                day: 2,
                hour: 12,
                minute: 0,
                second: 0,
            },
            forecast_hour: 6,
            centre: 255,
            bits: 16,
        }
    }

    fn sections(bytes: &[u8]) -> Vec<(u8, &[u8])> {
        let mut out = Vec::new();
        let mut at = 16;
        while at < bytes.len() - 4 {
            let length = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
            out.push((bytes[at + 4], &bytes[at..at + length]));
            at += length;
        }
        out
    }

    #[test]
    fn sections_are_the_documented_lengths() {
        assert_eq!(section1(&spec()).len(), 21);
        assert_eq!(section2().len(), 5 + PROVENANCE.len());
        assert_eq!(section3(grid()).len(), 72);
        assert_eq!(section4(&spec()).len(), 34);
        assert_eq!(section6().len(), 6);
        let packed = packing::pack(&[1.0, 2.0], packing::BITS_PER_VALUE).expect("packs");
        assert_eq!(section5(&packed).len(), 21);
        assert_eq!(section7(&packed).len(), 5 + packed.data.len());
    }

    /// Every message carries the provenance once, whatever it holds.
    #[test]
    fn every_message_says_it_was_created_with_polarexplorer() {
        let values = vec![0.0; grid().point_count() as usize];
        let bytes = message(&spec(), &values).expect("encode");
        let memos: Vec<_> = sections(&bytes)
            .into_iter()
            .filter(|(n, _)| *n == 2)
            .collect();
        assert_eq!(memos.len(), 1);
        assert_eq!(&memos[0].1[5..], b"Created with PolarExplorer");
    }

    /// A field with a hole gets a bitmap, one without gets none: section 6
    /// is 255 and six octets long, as in VectorEffects.
    #[test]
    fn a_bitmap_only_where_a_value_is_missing() {
        let n = grid().point_count() as usize;
        let whole = message(&spec(), &vec![1.5; n]).expect("whole");
        let s6 = sections(&whole)
            .into_iter()
            .find(|(k, _)| *k == 6)
            .unwrap()
            .1;
        assert_eq!(s6, &[0, 0, 0, 6, 6, 255]);
        let mut holed = vec![1.5f32; n];
        holed[3] = f32::NAN;
        let bytes = message(&spec(), &holed).expect("holed");
        let s6 = sections(&bytes)
            .into_iter()
            .find(|(k, _)| *k == 6)
            .unwrap()
            .1;
        assert_eq!(s6[5], 0, "a bitmap follows");
        assert_eq!(s6.len(), 6 + n.div_ceil(8));
        // Nodes 0–7: all present but node 3.
        assert_eq!(s6[6], 0b1110_1111);
        // Section 5 counts the values written, not the grid's.
        let s5 = sections(&bytes)
            .into_iter()
            .find(|(k, _)| *k == 5)
            .unwrap()
            .1;
        assert_eq!(
            u32::from_be_bytes(s5[5..9].try_into().unwrap()) as usize,
            n - 1
        );
    }

    /// Sign-magnitude, not two's complement.
    #[test]
    fn negative_values_use_sign_magnitude() {
        let mut out = Vec::new();
        put_i32_sm(&mut out, -90_000_000);
        assert_eq!(out, (0x8000_0000u32 | 90_000_000).to_be_bytes());
        assert_eq!(
            from_i32_sm(u32::from_be_bytes([out[0], out[1], out[2], out[3]])),
            -90_000_000
        );
        let mut out = Vec::new();
        put_i16_sm(&mut out, -9);
        assert_eq!(out, (0x8000u16 | 9).to_be_bytes());
        assert_eq!(from_i16_sm(u16::from_be_bytes([out[0], out[1]])), -9);
    }

    /// Section 3 of a regional grid across the prime meridian: the octets
    /// as the WMO template lays them out (La1 at 47, Lo1 at 51, flags at
    /// 55, La2 at 56, Lo2 at 60, Di at 64, Dj at 68, scanning at 72,
    /// 1-based), hand-assembled.
    #[test]
    fn a_regional_grid_writes_its_corners() {
        let g = GridSpec {
            ni: 50,
            nj: 22,
            la1: 53_000_000,
            lo1: 348_500_000,
            step: 250_000,
        };
        let s3 = section3(g);
        let u32_at = |at: usize| u32::from_be_bytes(s3[at..at + 4].try_into().unwrap());
        assert_eq!(u32_at(6), 1100, "points");
        assert_eq!(u32_at(30), 50);
        assert_eq!(u32_at(34), 22);
        assert_eq!(u32_at(46), 53_000_000);
        assert_eq!(u32_at(50), 348_500_000);
        assert_eq!(s3[54], 0x30);
        assert_eq!(u32_at(55), 47_750_000);
        assert_eq!(u32_at(59), 750_000, "Lo2 wraps past 360");
        assert_eq!(u32_at(63), 250_000);
        assert_eq!(u32_at(67), 250_000);
        assert_eq!(s3[71], 0);
        // A southern-hemisphere grid: La2 negative, sign-magnitude.
        let south = GridSpec {
            la1: -30_000_000,
            ..g
        };
        let s3 = section3(south);
        let raw = u32::from_be_bytes(s3[55..59].try_into().unwrap());
        assert_eq!(raw, 0x8000_0000 | 35_250_000);
    }

    #[test]
    fn global_grids_start_at_the_prime_meridian_and_wrap() {
        let row: Vec<(f64, f64)> = grid().points().take(360).collect();
        assert_eq!(row[0], (0.0, 90.0));
        assert_eq!(row[179], (179.0, 90.0));
        assert_eq!(row[180], (-180.0, 90.0));
        assert_eq!(row[359], (-1.0, 90.0));
        assert_eq!(grid().lo2(), 359_000_000);
        assert_eq!(grid().la2(), -90_000_000);
        let points: Vec<(f64, f64)> = grid().points().collect();
        assert_eq!(points.len(), 360 * 181);
        assert_eq!(points[points.len() - 1], (-1.0, -90.0));
    }

    #[test]
    fn a_message_has_the_right_envelope() {
        let values = vec![5.0f32; grid().point_count() as usize];
        let bytes = message(&spec(), &values).expect("builds");
        assert_eq!(&bytes[0..4], b"GRIB");
        assert_eq!(bytes[6], 0, "wind is discipline 0");
        assert_eq!(bytes[7], 2, "edition 2");
        let declared = u64::from_be_bytes(bytes[8..16].try_into().expect("8 bytes"));
        assert_eq!(declared as usize, bytes.len());
        assert_eq!(&bytes[bytes.len() - 4..], b"7777");
    }

    /// WMO tables 4.2-0-2, 4.2-10-0 and 4.2-10-1, and 4.5 for surfaces.
    #[test]
    fn parameter_numbers_follow_the_tables() {
        let table = [
            (Parameter::WindU, (0, 2, 2), 103),
            (Parameter::WindV, (0, 2, 3), 103),
            (Parameter::WaveHeight, (10, 0, 3), 1),
            (Parameter::WaveDirection, (10, 0, 14), 1),
            (Parameter::CurrentU, (10, 1, 2), 160),
            (Parameter::CurrentV, (10, 1, 3), 160),
        ];
        for (p, (d, c, n), surface) in table {
            assert_eq!(
                (p.discipline(), p.category(), p.number()),
                (d, c, n),
                "{p:?}"
            );
            assert_eq!(p.surface().0, surface, "{p:?}");
        }
    }

    #[test]
    fn mismatched_counts_bad_grids_and_bad_times_are_refused() {
        assert!(message(&spec(), &[1.0, 2.0]).is_err());
        let mut bad = spec();
        bad.reference_time.month = 13;
        let values = vec![0.0f32; grid().point_count() as usize];
        assert!(message(&bad, &values).is_err());
        let mut bad = spec();
        bad.grid.lo1 = 360_000_000;
        assert!(message(&bad, &values).is_err());
        let mut bad = spec();
        bad.grid.la1 = 80_000_000; // 181 rows down from 80N pass the pole
        assert!(message(&bad, &values).is_err());
    }

    /// Hand-checked calendar dates, including a leap day and before 1970.
    #[test]
    fn reference_times_from_epoch_seconds() {
        let t = |y, mo, d, h| ReferenceTime {
            year: y,
            month: mo,
            day: d,
            hour: h,
            minute: 0,
            second: 0,
        };
        assert_eq!(ReferenceTime::from_epoch(0).unwrap(), t(1970, 1, 1, 0));
        // 2020-07-27T12:00Z.
        assert_eq!(
            ReferenceTime::from_epoch(1_595_851_200).unwrap(),
            t(2020, 7, 27, 12)
        );
        // 2024-02-29T23:00Z.
        assert_eq!(
            ReferenceTime::from_epoch(1_709_247_600).unwrap(),
            t(2024, 2, 29, 23)
        );
        // 1959-01-01T00:00Z, WeatherBench2's first hour.
        assert_eq!(
            ReferenceTime::from_epoch(-347_155_200).unwrap(),
            t(1959, 1, 1, 0)
        );
        let odd = ReferenceTime::from_epoch(3_725).unwrap();
        assert_eq!((odd.hour, odd.minute, odd.second), (1, 2, 5));
    }
}
