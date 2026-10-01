//! A pure-Rust decoder for the blosc1 container, limited to LZ4. Copied from
//! VectorEffects' `ve-zarr`.
//!
//! The WeatherBench2, ARCO-ERA5 and Copernicus Marine arrays are all written
//! with `blosc(cname="lz4", clevel=5, shuffle=1)`. Decoding that needs three
//! things: the container
//! layout, raw LZ4 block decompression, and the byte unshuffle filter. All
//! three are short, so this crate does them itself rather than linking
//! c-blosc, which drags in a C++ toolchain for a Snappy path that this store
//! never uses.
//!
//! # Container layout
//!
//! ```text
//! 0   version         1 octet
//! 1   version of lz    1 octet
//! 2   flags           1 octet
//! 3   typesize        1 octet
//! 4   nbytes          4 octets, little endian, uncompressed size
//! 8   blocksize       4 octets, little endian
//! 12  cbytes          4 octets, little endian, size of the whole container
//! 16  bstarts         4 octets each, one per block, little endian
//! ```
//!
//! Each block holds one or more *streams*. A block is split into `typesize`
//! streams unless the `DONT_SPLIT` flag is set or the block is the short
//! final one. Every stream is prefixed by its own compressed length; when
//! that length equals the stream's uncompressed size the stream was stored
//! verbatim, which is how blosc represents incompressible data.

use crate::error::{EnvError, Result};

/// Byte shuffle filter was applied.
const FLAG_SHUFFLE: u8 = 0x01;
/// The payload is stored uncompressed.
const FLAG_MEMCPYED: u8 = 0x02;
/// Bit shuffle filter was applied. Not supported here.
const FLAG_BITSHUFFLE: u8 = 0x04;
/// Blocks were not split into per-byte streams.
const FLAG_DONT_SPLIT: u8 = 0x10;

/// The compressor identifier for LZ4, in the top three bits of the flags.
const COMPRESSOR_LZ4: u8 = 1;

/// The fixed container header size.
pub const HEADER_LEN: usize = 16;

fn u32le(bytes: &[u8], at: usize) -> Result<u32> {
    bytes
        .get(at..at + 4)
        .and_then(|b| b.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or_else(|| EnvError::Blosc(format!("truncated at offset {at}")))
}

/// Reverses blosc's byte shuffle.
///
/// The shuffled form stores every element's first byte, then every element's
/// second byte, and so on. Bytes past the last whole element are stored
/// verbatim, which matters for the short final block.
fn unshuffle(src: &[u8], typesize: usize) -> Vec<u8> {
    let n = src.len();
    if typesize <= 1 {
        return src.to_vec();
    }
    let mut out = vec![0u8; n];
    let nelem = n / typesize;
    for j in 0..typesize {
        let plane = j * nelem;
        for i in 0..nelem {
            out[i * typesize + j] = src[plane + i];
        }
    }
    let tail = nelem * typesize;
    out[tail..].copy_from_slice(&src[tail..]);
    out
}

/// The largest decoded size accepted when the caller cannot say what to
/// expect. A global ERA5 field is 4.2 MB and a CMEMS geoChunk 2.2 MB.
pub const MAX_UNKNOWN_SIZE: usize = 256 << 20;

/// What the caller knows about the decoded size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expected {
    /// Exactly this many bytes (a chunk of a known shape and type).
    Exactly(usize),
    /// At most this many bytes.
    AtMost(usize),
}

/// A blosc1 container's fixed header, checked.
///
/// The sizes come from the archive and are not trusted: [`Header::parse`]
/// checks them against what the caller expects before anything is sized
/// from them, so a 30-byte container declaring 4 GB is an error, not an
/// abort.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    flags: u8,
    /// Bytes per element, as the shuffle saw them.
    pub typesize: usize,
    /// Decoded size of the whole container.
    pub nbytes: usize,
    /// Decoded size of every block but a short final one.
    pub blocksize: usize,
    /// Size of the whole container, header included.
    pub cbytes: usize,
}

impl Header {
    /// Reads and checks the first [`HEADER_LEN`] bytes of a container.
    ///
    /// # Errors
    /// [`EnvError::Blosc`] if `src` is shorter than the header, declares a
    /// size other than `expected`, a block larger than the chunk, or a
    /// compressor or filter this decoder does not implement.
    pub fn parse(src: &[u8], expected: Expected) -> Result<Self> {
        if src.len() < HEADER_LEN {
            return Err(EnvError::Blosc(format!(
                "container is {} bytes, shorter than the {HEADER_LEN}-byte header",
                src.len()
            )));
        }
        let flags = src[2];
        let header = Self {
            flags,
            typesize: usize::from(src[3]),
            nbytes: u32le(src, 4)? as usize,
            blocksize: u32le(src, 8)? as usize,
            cbytes: u32le(src, 12)? as usize,
        };
        let nbytes = header.nbytes;
        match expected {
            Expected::Exactly(n) if nbytes != n => {
                return Err(EnvError::Blosc(format!(
                    "header declares {nbytes} decoded bytes but the chunk holds {n}"
                )));
            }
            Expected::AtMost(n) if nbytes > n => {
                return Err(EnvError::Blosc(format!(
                    "header declares {nbytes} decoded bytes, more than the {n} allowed"
                )));
            }
            _ => {}
        }
        if header.blocksize > nbytes && nbytes > 0 {
            return Err(EnvError::Blosc(format!(
                "header declares a {}-byte block in a {nbytes}-byte chunk",
                header.blocksize
            )));
        }
        // blosc never grows data by more than its header, offsets and stream
        // lengths; a container declaring far more is not one to size
        // ranges from, and nothing larger than a body may be is ever read.
        let most = nbytes
            .saturating_mul(2)
            .saturating_add(4096)
            .min(crate::http::MAX_BODY_BYTES as usize);
        if header.cbytes > most {
            return Err(EnvError::Blosc(format!(
                "header declares {} bytes for {nbytes} decoded, more than the {most} a container may take",
                header.cbytes
            )));
        }
        if flags & FLAG_BITSHUFFLE != 0 {
            return Err(EnvError::Blosc("bit shuffle is not supported".into()));
        }
        let compressor = flags >> 5;
        if compressor != COMPRESSOR_LZ4 {
            return Err(EnvError::Blosc(format!(
                "compressor id {compressor} is not supported; this decoder handles lz4 only"
            )));
        }
        if !header.memcpyed() {
            if header.blocksize == 0 && nbytes > 0 {
                return Err(EnvError::Blosc("header declares a zero blocksize".into()));
            }
            if header.typesize == 0 {
                return Err(EnvError::Blosc("header declares a zero typesize".into()));
            }
        }
        Ok(header)
    }

    /// Whether the payload is stored uncompressed, straight after the header.
    pub fn memcpyed(&self) -> bool {
        self.flags & FLAG_MEMCPYED != 0
    }

    /// How many blocks the container holds.
    pub fn nblocks(&self) -> usize {
        if self.blocksize == 0 {
            // Only a memcpyed or empty container gets here.
            usize::from(self.nbytes > 0)
        } else {
            self.nbytes.div_ceil(self.blocksize)
        }
    }

    /// The decoded size of a memcpyed container's "blocks": the declared
    /// blocksize when there is one, else the whole payload.
    fn span(&self) -> usize {
        if self.blocksize == 0 {
            self.nbytes
        } else {
            self.blocksize
        }
    }

    /// How many bytes from the start hold the header and the block offsets:
    /// what has to be read before any block can be found.
    pub fn index_len(&self) -> usize {
        if self.memcpyed() {
            HEADER_LEN
        } else {
            HEADER_LEN + 4 * self.nblocks()
        }
    }

    /// The decoded bytes of block `b`, and where they start in the chunk.
    pub fn block_span(&self, b: usize) -> std::ops::Range<usize> {
        let start = (b * self.span()).min(self.nbytes);
        start..(start + self.span()).min(self.nbytes)
    }

    /// Where each block's compressed bytes lie in the container.
    ///
    /// `index` is the start of the container, at least [`Self::index_len`]
    /// bytes of it. A block runs from its offset to the next larger one (or
    /// the end of the container): blosc writes them in order, and taking the
    /// next larger offset rather than the next entry keeps a reordered
    /// container bounded too.
    ///
    /// # Errors
    /// [`EnvError::Blosc`] if `index` is too short or an offset points
    /// outside the container.
    pub fn block_extents(&self, index: &[u8]) -> Result<Vec<std::ops::Range<usize>>> {
        let n = self.nblocks();
        if self.memcpyed() {
            return Ok((0..n)
                .map(|b| {
                    let span = self.block_span(b);
                    HEADER_LEN + span.start..HEADER_LEN + span.end
                })
                .collect());
        }
        if index.len() < self.index_len() {
            return Err(EnvError::Blosc(format!(
                "the block offsets need {} bytes, only {} were read",
                self.index_len(),
                index.len()
            )));
        }
        let mut starts = Vec::with_capacity(n);
        for b in 0..n {
            let start = u32le(index, HEADER_LEN + b * 4)? as usize;
            if start < self.index_len() || start >= self.cbytes {
                return Err(EnvError::Blosc(format!(
                    "block {b} starts at {start}, outside the {}-byte container",
                    self.cbytes
                )));
            }
            starts.push(start);
        }
        let mut sorted = starts.clone();
        sorted.sort_unstable();
        Ok(starts
            .iter()
            .map(|&start| {
                let end = sorted
                    .iter()
                    .copied()
                    .find(|&s| s > start)
                    .unwrap_or(self.cbytes);
                start..end
            })
            .collect())
    }

    /// Decodes block `b` from its compressed bytes (its extent, as
    /// [`Self::block_extents`] gives it).
    ///
    /// Blocks decode independently: the byte shuffle and the per-byte
    /// streams are both within a block. That is what lets a reader fetch
    /// only the blocks holding the rows it needs.
    ///
    /// # Errors
    /// [`EnvError::Blosc`] if the bytes are short, a stream runs past them,
    /// or a stream does not decode to its size.
    pub fn decode_block(&self, b: usize, src: &[u8]) -> Result<Vec<u8>> {
        if b >= self.nblocks() {
            return Err(EnvError::Blosc(format!(
                "block {b} of a {}-block container",
                self.nblocks()
            )));
        }
        let span = self.block_span(b);
        let block_len = span.len();
        if self.memcpyed() {
            return src
                .get(..block_len)
                .map(<[u8]>::to_vec)
                .ok_or_else(|| EnvError::Blosc(format!("block {b} is truncated")));
        }
        let typesize = self.typesize;
        let is_final_partial = span.end == self.nbytes && block_len < self.blocksize;
        // c-blosc splits a block into one stream per byte of the element, so
        // that each stream sees one shuffle plane. The short final block and
        // an explicit DONT_SPLIT are the exceptions.
        let nstreams = if self.flags & FLAG_DONT_SPLIT == 0 && !is_final_partial {
            typesize
        } else {
            1
        };
        if !block_len.is_multiple_of(nstreams) {
            return Err(EnvError::Blosc(format!(
                "block {b} of {block_len} bytes does not divide into {nstreams} streams"
            )));
        }
        let stream_len = block_len / nstreams;
        let mut block = Vec::with_capacity(block_len);
        let mut at = 0;
        for s in 0..nstreams {
            let compressed_len = u32le(src, at)? as usize;
            at += 4;
            let payload = src
                .get(at..at.saturating_add(compressed_len))
                .ok_or_else(|| {
                    EnvError::Blosc(format!("block {b} stream {s} runs past the block"))
                })?;
            if compressed_len == stream_len {
                // blosc stores incompressible streams verbatim.
                block.extend_from_slice(payload);
            } else {
                let decoded = lz4_flex::block::decompress(payload, stream_len)
                    .map_err(|e| EnvError::Blosc(format!("block {b} stream {s}: {e}")))?;
                if decoded.len() != stream_len {
                    return Err(EnvError::Blosc(format!(
                        "block {b} stream {s} decoded to {} bytes, expected {stream_len}",
                        decoded.len()
                    )));
                }
                block.extend_from_slice(&decoded);
            }
            at += compressed_len;
        }
        if self.flags & FLAG_SHUFFLE != 0 {
            Ok(unshuffle(&block, typesize))
        } else {
            Ok(block)
        }
    }
}

/// Decompresses one blosc1 container.
///
/// # Errors
/// Returns [`EnvError::Blosc`] if the container is truncated, uses a
/// compressor or filter this decoder does not implement, declares a size
/// other than the one expected, or does not decode to the size its header
/// declares.
pub fn decompress(src: &[u8], expected: Expected) -> Result<Vec<u8>> {
    let header = Header::parse(src, expected)?;
    if header.cbytes != src.len() {
        return Err(EnvError::Blosc(format!(
            "header declares {} bytes but the chunk is {}",
            header.cbytes,
            src.len()
        )));
    }
    let extents = header.block_extents(src)?;
    let mut out = Vec::with_capacity(header.nbytes);
    for (b, extent) in extents.into_iter().enumerate() {
        let bytes = src
            .get(extent)
            .ok_or_else(|| EnvError::Blosc(format!("block {b} runs past the chunk")))?;
        out.extend_from_slice(&header.decode_block(b, bytes)?);
    }
    if out.len() != header.nbytes {
        return Err(EnvError::Blosc(format!(
            "decoded {} bytes, header declares {}",
            out.len(),
            header.nbytes
        )));
    }
    Ok(out)
}

/// Encodes `data` as a blosc1 container the way the archives are written
/// (byte shuffle, LZ4, one stream per byte of the element except in a
/// short final block), with blocks of `blocksize` bytes.
///
/// PolarExplorer never writes Zarr; this exists so tests can build
/// multi-block chunks whose every byte they know, and is checked against
/// the decoder, never used by the application.
#[doc(hidden)]
pub fn encode_for_tests(data: &[u8], typesize: usize, blocksize: usize) -> Vec<u8> {
    let typesize = typesize.max(1);
    let blocksize = blocksize.max(typesize);
    let nblocks = data.len().div_ceil(blocksize);
    let mut body = Vec::new();
    let mut starts = Vec::new();
    let index_len = HEADER_LEN + 4 * nblocks;
    for b in 0..nblocks {
        starts.push((index_len + body.len()) as u32);
        let block = &data[b * blocksize..((b + 1) * blocksize).min(data.len())];
        let partial = block.len() < blocksize;
        let shuffled = shuffle(block, typesize);
        let nstreams = if partial { 1 } else { typesize };
        let stream_len = block.len() / nstreams;
        for s in 0..nstreams {
            let stream = &shuffled[s * stream_len..(s + 1) * stream_len];
            let packed = lz4_flex::block::compress(stream);
            let (len, bytes) = if packed.len() >= stream_len {
                (stream_len, stream.to_vec())
            } else {
                (packed.len(), packed)
            };
            body.extend_from_slice(&(len as u32).to_le_bytes());
            body.extend_from_slice(&bytes);
        }
    }
    let mut out = vec![2u8, 1, FLAG_SHUFFLE | (COMPRESSOR_LZ4 << 5), typesize as u8];
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&(blocksize as u32).to_le_bytes());
    out.extend_from_slice(&((index_len + body.len()) as u32).to_le_bytes());
    for start in starts {
        out.extend_from_slice(&start.to_le_bytes());
    }
    out.extend_from_slice(&body);
    out
}

/// blosc's byte shuffle, the inverse of [`unshuffle`] (for
/// [`encode_for_tests`]).
fn shuffle(src: &[u8], typesize: usize) -> Vec<u8> {
    let n = src.len();
    if typesize <= 1 {
        return src.to_vec();
    }
    let nelem = n / typesize;
    let mut out = vec![0u8; n];
    for i in 0..nelem {
        for j in 0..typesize {
            out[j * nelem + i] = src[i * typesize + j];
        }
    }
    let tail = nelem * typesize;
    out[tail..].copy_from_slice(&src[tail..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A container may not declare itself far larger than what it holds:
    /// ranges are sized from `cbytes`.
    #[test]
    fn an_inflated_container_size_is_refused() {
        let flags = COMPRESSOR_LZ4 << 5;
        let c = header(flags, 4, 4000, 1024, 20_000);
        let err = Header::parse(&c, Expected::Exactly(4000)).expect_err("too large");
        assert!(format!("{err}").contains("more than"), "{err}");
        let big = header(flags, 4, 64 << 20, 1 << 20, (64 << 20) + 4096);
        assert!(Header::parse(&big, Expected::AtMost(MAX_UNKNOWN_SIZE)).is_err());
        assert!(
            Header::parse(&header(flags, 4, 4000, 1024, 4100), Expected::Exactly(4000)).is_ok()
        );
    }

    /// The shuffle is its own documentation: element bytes are interleaved
    /// into planes, and unshuffling must put them back in element order.
    #[test]
    fn unshuffle_reverses_the_plane_layout() {
        // Three 4-byte elements: 00010203 04050607 08090a0b.
        let planes = [0u8, 4, 8, 1, 5, 9, 2, 6, 10, 3, 7, 11];
        assert_eq!(
            unshuffle(&planes, 4),
            vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]
        );
    }

    #[test]
    fn a_typesize_of_one_is_a_copy() {
        let data = [9u8, 8, 7];
        assert_eq!(unshuffle(&data, 1), data.to_vec());
    }

    /// A buffer that is not a whole number of elements keeps its tail bytes in
    /// place. The final block of an ERA5 chunk is where this shows up.
    #[test]
    fn unshuffle_copies_a_partial_trailing_element() {
        // Two whole 4-byte elements plus two loose bytes.
        let mut planes = vec![0u8, 4, 1, 5, 2, 6, 3, 7];
        planes.extend_from_slice(&[0xaa, 0xbb]);
        let out = unshuffle(&planes, 4);
        assert_eq!(&out[..8], &[0, 1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(&out[8..], &[0xaa, 0xbb], "tail bytes are verbatim");
    }

    fn header(flags: u8, typesize: u8, nbytes: u32, blocksize: u32, cbytes: u32) -> Vec<u8> {
        let mut h = vec![2u8, 1, flags, typesize];
        h.extend_from_slice(&nbytes.to_le_bytes());
        h.extend_from_slice(&blocksize.to_le_bytes());
        h.extend_from_slice(&cbytes.to_le_bytes());
        h
    }

    #[test]
    fn a_memcpyed_container_returns_its_payload() {
        let payload = [1u8, 2, 3, 4, 5, 6, 7, 8];
        let flags = FLAG_MEMCPYED | (COMPRESSOR_LZ4 << 5);
        let mut c = header(flags, 4, 8, 8, (HEADER_LEN + 8) as u32);
        c.extend_from_slice(&payload);
        assert_eq!(
            decompress(&c, Expected::Exactly(8)).expect("decodes"),
            payload.to_vec()
        );
    }

    /// One block, one stream, stored verbatim because it did not compress.
    #[test]
    fn an_incompressible_stream_is_taken_verbatim() {
        let payload = [0xdeu8, 0xad, 0xbe, 0xef];
        let flags = FLAG_DONT_SPLIT | (COMPRESSOR_LZ4 << 5);
        let body_at = HEADER_LEN + 4;
        let mut c = header(flags, 4, 4, 4, (body_at + 4 + 4) as u32);
        c.extend_from_slice(&(body_at as u32).to_le_bytes()); // bstarts[0]
        c.extend_from_slice(&4u32.to_le_bytes()); // stream length == stream size
        c.extend_from_slice(&payload);
        assert_eq!(
            decompress(&c, Expected::Exactly(4)).expect("decodes"),
            payload.to_vec()
        );
    }

    /// Review fix: a tiny container declaring 4 GB must be refused before
    /// anything is allocated, whether or not the caller knows the size.
    #[test]
    fn an_inflated_declared_size_is_refused_before_allocating() {
        let flags = COMPRESSOR_LZ4 << 5;
        let mut c = header(flags, 4, u32::MAX, u32::MAX, (HEADER_LEN + 14) as u32);
        c.extend_from_slice(&[0; 14]);
        let err = decompress(&c, Expected::Exactly(4 * 721 * 1440)).expect_err("must refuse");
        assert!(format!("{err}").contains("declares"), "{err}");
        let err = decompress(&c, Expected::AtMost(MAX_UNKNOWN_SIZE)).expect_err("must refuse");
        assert!(format!("{err}").contains("allowed"), "{err}");
        // The memcpyed path takes the same check.
        let mut m = header(
            FLAG_MEMCPYED | flags,
            4,
            u32::MAX,
            8,
            (HEADER_LEN + 8) as u32,
        );
        m.extend_from_slice(&[0; 8]);
        assert!(decompress(&m, Expected::Exactly(8)).is_err());
    }

    /// A block larger than the whole chunk is refused rather than sized.
    #[test]
    fn a_block_larger_than_the_chunk_is_refused() {
        let flags = COMPRESSOR_LZ4 << 5;
        let mut c = header(flags, 4, 8, u32::MAX, (HEADER_LEN + 12) as u32);
        c.extend_from_slice(&[0; 12]);
        let err = decompress(&c, Expected::Exactly(8)).expect_err("must refuse");
        assert!(format!("{err}").contains("block"), "{err}");
    }

    #[test]
    fn a_short_container_is_rejected() {
        assert!(matches!(
            decompress(&[0; 4], Expected::AtMost(MAX_UNKNOWN_SIZE)),
            Err(EnvError::Blosc(_))
        ));
    }

    #[test]
    fn an_unsupported_compressor_is_rejected() {
        // Compressor 4 is zstd, which this decoder does not implement.
        let c = header(4 << 5, 4, 8, 8, HEADER_LEN as u32);
        let err = decompress(&c, Expected::Exactly(8)).expect_err("must reject");
        assert!(format!("{err}").contains("not supported"), "{err}");
    }

    #[test]
    fn bit_shuffle_is_rejected_rather_than_decoded_wrongly() {
        let flags = FLAG_BITSHUFFLE | (COMPRESSOR_LZ4 << 5);
        let c = header(flags, 4, 8, 8, HEADER_LEN as u32);
        let err = decompress(&c, Expected::Exactly(8)).expect_err("must reject");
        assert!(format!("{err}").contains("bit shuffle"), "{err}");
    }

    #[test]
    fn a_length_that_disagrees_with_the_header_is_rejected() {
        let flags = COMPRESSOR_LZ4 << 5;
        let c = header(flags, 4, 8, 8, 999);
        let err = decompress(&c, Expected::Exactly(8)).expect_err("must reject");
        assert!(format!("{err}").contains("declares"), "{err}");
    }

    /// Floats that do not compress to nothing: a ramp with a wobble.
    fn field(n: usize) -> Vec<u8> {
        (0..n)
            .flat_map(|i| ((i as f32) * 0.37 + ((i * 7919) % 13) as f32).to_le_bytes())
            .collect()
    }

    /// Three whole blocks and a short fourth: the whole container decodes
    /// to what went in, and so does each block on its own from its extent
    /// alone, which is what a ranged read relies on.
    #[test]
    fn every_block_decodes_alone_from_its_extent() {
        let data = field(1000); // 4000 bytes
        let c = encode_for_tests(&data, 4, 1024);
        assert_eq!(
            decompress(&c, Expected::Exactly(4000)).expect("decodes"),
            data
        );
        let header = Header::parse(&c, Expected::Exactly(4000)).expect("a header");
        assert_eq!(header.nblocks(), 4);
        assert_eq!(header.index_len(), 16 + 16);
        assert_eq!(header.block_span(3), 3072..4000);
        let extents = header
            .block_extents(&c[..header.index_len()])
            .expect("extents");
        assert_eq!(extents.last().map(|e| e.end), Some(c.len()));
        for (b, extent) in extents.into_iter().enumerate() {
            let alone = c[extent].to_vec();
            let decoded = header.decode_block(b, &alone).expect("decodes alone");
            assert_eq!(decoded, data[header.block_span(b)].to_vec(), "block {b}");
        }
        assert!(header.decode_block(4, &[]).is_err());
    }

    /// The offsets need the whole index; fewer bytes is an error, not a
    /// guess.
    #[test]
    fn block_offsets_need_the_whole_index_and_stay_inside() {
        let c = encode_for_tests(&field(1000), 4, 1024);
        let header = Header::parse(&c, Expected::AtMost(MAX_UNKNOWN_SIZE)).expect("a header");
        assert!(header.block_extents(&c[..20]).is_err());
        let mut bad = c.clone();
        bad[16..20].copy_from_slice(&(c.len() as u32 + 5).to_le_bytes());
        assert!(header.block_extents(&bad).is_err());
        // A block cut short by its extent fails rather than reading on.
        let extents = header.block_extents(&c).expect("extents");
        let short = &c[extents[1].start..extents[1].end - 3];
        assert!(header.decode_block(1, short).is_err());
    }

    /// A memcpyed container's blocks are plain slices after the header.
    #[test]
    fn memcpyed_blocks_are_slices_after_the_header() {
        let payload: Vec<u8> = (0u8..40).collect();
        let flags = FLAG_MEMCPYED | (COMPRESSOR_LZ4 << 5);
        let mut c = header(flags, 4, 40, 16, (HEADER_LEN + 40) as u32);
        c.extend_from_slice(&payload);
        let h = Header::parse(&c, Expected::Exactly(40)).expect("a header");
        assert_eq!(h.index_len(), HEADER_LEN);
        let extents = h.block_extents(&c[..HEADER_LEN]).expect("extents");
        assert_eq!(extents, vec![16..32, 32..48, 48..56]);
        assert_eq!(
            h.decode_block(2, &c[48..56]).expect("a slice"),
            payload[32..40].to_vec()
        );
        assert_eq!(
            decompress(&c, Expected::Exactly(40)).expect("decodes"),
            payload
        );
    }
}
