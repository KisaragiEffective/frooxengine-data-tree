#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(all(feature = "alloc", not(feature = "std")))]
use alloc::string::String;
#[cfg(feature = "alloc")]
use alloc::borrow::Cow;
use core::fmt::{Display, Formatter};
use lz4_flex::decompress_size_prepended;
#[cfg(feature = "serde")]
use serde::{Deserialize, Deserializer};
#[cfg(feature = "serde")]
use serde::de::DeserializeOwned;
#[cfg(feature = "std")]
use thiserror::Error;

#[derive(Debug)]
#[cfg_attr(feature = "std", derive(Error))]
pub enum FrooxContainerExtractError {
    #[cfg_attr(feature = "std", error("FrDT magic number is corrupted"))]
    InvalidFirstMagicNumber,
    #[cfg_attr(feature = "std", error("Reserved header is corrupted"))]
    InvalidSecondMagicNumber,
    #[cfg_attr(feature = "std", error("Compression algorithm indicator must be fit in 1-byte value"))]
    TooLargeForCompressionMethod,
    #[cfg_attr(feature = "std", error("Unknown compression method"))]
    UnknownCompressionMethod,
    #[cfg_attr(feature = "std", error("Got corrupted VarInt: {0}"))]
    VarIntDecodeError(variant_compression_2::Error),
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u8)]
pub enum FrooxContainerCompressMethod {
    None,
    LZ4,
    LZMA,
    Brotli,
}

impl Display for FrooxContainerCompressMethod {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            FrooxContainerCompressMethod::None => "<no compress>",
            FrooxContainerCompressMethod::LZ4 => "lz4",
            FrooxContainerCompressMethod::LZMA => "lzma",
            FrooxContainerCompressMethod::Brotli => "brotli",
        };

        f.write_str(s)
    }
}

impl TryFrom<u8> for FrooxContainerCompressMethod {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::None),
            1 => Ok(Self::LZ4),
            2 => Ok(Self::LZMA),
            3 => Ok(Self::Brotli),
            _ => Err(())
        }
    }
}

pub fn split_froox_container_header(m: &[u8]) -> Result<FrooxContainer, FrooxContainerExtractError> {
    let Some(m) = m.strip_prefix(b"FrDT") else {
        return Err(FrooxContainerExtractError::InvalidFirstMagicNumber)
    };

    let Some(m) = m.strip_prefix(b"\0\0\0\0") else {
        return Err(FrooxContainerExtractError::InvalidSecondMagicNumber)
    };

    // It seems that froox container use var-int to express inner compress method.
    let (i, m) = variant_compression_2::decompress(m).map_err(|e| FrooxContainerExtractError::VarIntDecodeError(e))?;

    let Ok(i) = u8::try_from(i) else {
        return Err(FrooxContainerExtractError::TooLargeForCompressionMethod)
    };

    let compress_method = FrooxContainerCompressMethod::try_from(i)
        .map_err(|_| FrooxContainerExtractError::UnknownCompressionMethod)?;

    Ok(FrooxContainer::Current {
        header: FrDT(()),
        compress_method,
        raw_content: m,
    })
}

#[cfg(feature = "legacy")]
pub fn legacy(n: &[u8]) -> FrooxContainer {
    FrooxContainer::Legacy { raw_content: n }
}

#[cfg(feature = "serde")]
#[cfg_attr(feature = "std", derive(Error))]
#[derive(Debug)]
pub enum DeserializeError {
    #[cfg(feature = "lz4")]
    #[cfg_attr(feature = "std", error("lz4 decompressor: {0}"))]
    Lz4Decompression(#[from] Lz4DecompressionError),
    #[cfg(feature = "lzma")]
    #[cfg_attr(feature = "std", error("lzma decompressor: {0}"))]
    LzmaDecompression(#[from] LzmaDecompressionError),
    #[cfg(feature = "std")]
    #[cfg_attr(feature = "std", error("I/O stream error: {0}"))]
    Io(#[from] std::io::Error),
    #[cfg(not(all(feature = "lz4", feature = "lzma", feature = "brotli")))]
    #[cfg_attr(feature = "std", error("decompress: {0} is not installed (perhaps need re-compile?)"))]
    NonInstalledDecompressMethod(FrooxContainerCompressMethod),
    #[cfg(feature = "std")]
    #[cfg_attr(feature = "std", error("bson: {0}"))]
    Bson(#[from] bson::de::Error),
    #[cfg(feature = "std")]
    #[cfg_attr(feature = "std", error("brute force on legacy format input was failed (lzma = {lzma}, lz4 = {lz4}, bson = {bson})"))]
    LegacyBruteforce {
        #[cfg(feature = "lzma")]
        lzma: std::io::Error,
        #[cfg(not(feature = "lzma"))]
        lzma: (),
        #[cfg(feature = "lz4")]
        lz4: Lz4DecompressionError,
        #[cfg(not(feature = "lz4"))]
        lz4: (),
        bson: bson::de::Error,
    },
}

#[cfg(feature = "std")]
#[derive(Debug, Error)]
pub enum Lz4DecompressionError {
    #[error("I/O error: {0}")]
    Io(::std::io::Error),
    #[error("I/O error on exact_read: {0}")]
    ExactRead(::std::io::Error),
    #[error("VarInt was corrupted: {0}")]
    CorruptedDotNetSpecificHeader(variant_compression_2::Error),
    #[error("Invalid chunk flag (maybe it's not created with LZ4.NET?)")]
    InvalidChunkFlag(()),
    #[error("chunk is too short ({actual_length} < {excepted_length})")]
    ChunkIsTooShort { actual_length: usize, excepted_length: usize },
    #[error("size header is broken ({compressed} > {uncompressed})")]
    InvalidSizeHeader { compressed: u64, uncompressed: u64 },
    #[error("lz4_flex: {0}")]
    Lz4Flex(lz4_flex::block::DecompressError)
}

#[cfg(not(feature = "std"))]
pub struct Lz4DecompressionError(());

#[cfg(feature = "std")]
#[derive(Debug, Error)]
pub enum LzmaDecompressionError {
    #[error("I/O error: {0}")]
    Io(::std::io::Error),
    #[error("VarInt was corrupted: {0}")]
    CorruptedDotNetSpecificHeader(variant_compression_2::Error),
}

#[cfg(not(feature = "std"))]
pub struct LzmaDecompressionError(());

#[derive(Debug)]
pub struct FrDT(());

#[derive(Debug)]
pub enum FrooxContainer<'a> {
    #[cfg(feature = "legacy")]
    Legacy {
        raw_content: &'a [u8],
    },
    Current {
        header: FrDT,
        compress_method: FrooxContainerCompressMethod,
        raw_content: &'a [u8],
    },
}

impl<'a> FrooxContainer<'a> {
    #[cfg(all(feature = "serde", feature = "std"))]
    pub fn deserialize<T: DeserializeOwned>(&self) -> Result<T, DeserializeError> {
        use std::io::{Cursor, Read};
        match self {
            #[cfg(feature = "legacy")]
            FrooxContainer::Legacy { raw_content } => {
                let mut raw_content = Cursor::new(*raw_content);
                let mut buf = vec![];

                #[cfg(feature = "lzma")]
                let lzma_error = {
                    match seven_zip::lzma_decompress(&mut raw_content, &mut buf) {
                        Ok(()) => {
                            let x = bson::from_slice(&buf)?;

                            return Ok(x)
                        }
                        Err(lzma) => lzma
                    }
                };

                let lz4_error = match read_lz4net_chunks(raw_content.get_ref()) {
                    Ok(d) => {
                        let x = bson::from_slice(&d)?;

                        return Ok(x)
                    },
                    Err(e) => e
                };

                // are you giving raw BSON here?
                let x = bson::from_slice(raw_content.get_ref()).map_err(|e| {
                    DeserializeError::LegacyBruteforce {
                        #[cfg(feature = "lzma")]
                        lzma: lzma_error,
                        #[cfg(feature = "lz4")]
                        lz4: lz4_error,
                        bson: e,
                    }
                })?;

                Ok(x)
            }
            FrooxContainer::Current { header: _, compress_method, raw_content } => {
                let mut cursor = Cursor::new(*raw_content);
                let after_decompress: Cow<'_, [u8]> = match compress_method {
                    FrooxContainerCompressMethod::None => {
                        Cow::Borrowed(*cursor.get_ref())
                    }
                    #[cfg(feature = "lz4")]
                    FrooxContainerCompressMethod::LZ4 => {
                        let buf = read_lz4net_chunks(cursor.get_ref())?;
                        lz4_flex::decompress(&buf, 0).map_err(Lz4DecompressionError::Lz4Flex)?.into()
                    }
                    #[cfg(feature = "lzma")]
                    FrooxContainerCompressMethod::LZMA => {
                        let mut buf = vec![];
                        seven_zip::lzma_decompress(&mut cursor, &mut buf).map_err(|e| DeserializeError::LzmaDecompression(LzmaDecompressionError::Io(e)))?;
                        buf.into()
                    }
                    #[cfg(feature = "brotli")]
                    FrooxContainerCompressMethod::Brotli => {
                        let mut brotli = vec![];
                        brotli::Decompressor::new(cursor, 16 * 1024).read_to_end(&mut brotli)?;
                        brotli.into()
                    }
                    #[cfg(not(all(feature = "lz4", feature = "lzma", feature = "brotli")))]
                    other => return Err(DeserializeError::NonInstalledDecompressMethod(other))
                };

                let read = bson::from_slice::<T>(after_decompress.as_ref())?;
                Ok(read)
            }
        }
    }
}

#[cfg(all(feature = "lz4", feature = "serde"))]
/// don't use this. This has bug and not working properly!!
fn read_lz4net_chunks(raw: &[u8]) -> Result<Vec<u8>, Lz4DecompressionError> {
    // TODO: this is totally bugged, LZ4NET uses weird codec :(
    let mut buf = Vec::with_capacity(2048);
    let mut rec_acc = raw;
    while !rec_acc.is_empty() {
        let (decoded, rest) = read_lz4net_chunk(rec_acc)?;
        println!("decoded: {}", decoded.len());

        buf.extend_from_slice(&decoded);

        rec_acc = rest;
    }

    Ok(buf)
}

#[cfg(feature = "lz4")]
fn read_lz4net_chunk(raw: &[u8]) -> Result<(Cow<'_, [u8]>, &[u8]), Lz4DecompressionError> {
    println!("raw: {}", raw.len());
    use std::io::Read;
    // LZ4NET-specific handle (thanks, @ThomFox!) - see https://github.com/GuVAnj8Gv3RJ/NeosAccountDownloader/issues/17#issuecomment-1601662004
    let (flags, raw_content) = variant_compression_2::decompress(raw)
        .map_err(Lz4DecompressionError::CorruptedDotNetSpecificHeader)?;
    let (uncompressed_size, raw_content) = variant_compression_2::decompress(raw_content)
        .map_err(Lz4DecompressionError::CorruptedDotNetSpecificHeader)?;
    let compressed = (flags & 1) == 1;
    let (compressed_size, raw_content) = if compressed {
        variant_compression_2::decompress(raw_content)
            .map_err(Lz4DecompressionError::CorruptedDotNetSpecificHeader)?
    } else {
        (uncompressed_size, raw_content)
    };

    if compressed_size > uncompressed_size {
        return Err(Lz4DecompressionError::InvalidSizeHeader {
            compressed: compressed_size,
            uncompressed: uncompressed_size
        })
    }
    let compressed_size = compressed_size as usize;
    let uncompressed_size = uncompressed_size as usize;
    println!("flags: {flags}");
    println!("con: {compressed_size}");
    println!("de-con: {uncompressed_size}");
    let (chunk, rest) = raw_content.split_at(compressed_size);
    if chunk.len() < compressed_size {
        return Err(Lz4DecompressionError::ChunkIsTooShort {
            actual_length: chunk.len(),
            excepted_length: compressed_size,
        })
    }
    debug_assert!(chunk.len() == compressed_size);

    let decoded = if compressed {
        let lz4_block = &chunk[..compressed_size];
        eprintln!("{r:X?}", r = &lz4_block[0..10]);
        let maybe_len = lz4_block[0] >> 4;
        let lz4_block = if maybe_len == 15 {
            let (y, r) = variant_compression_2::decompress(&lz4_block[1..]).expect("o");
            eprintln!("{y} ~= {:?}", &r[0..10]);
            &r[6..]
        } else {
            lz4_block
        };
        let mut buf = decompress_size_prepended(lz4_block).expect("ohoh");

                buf.into()
    } else {
        // Not compressed
        Cow::Borrowed(chunk)
    };

    Ok((decoded, rest))
}
