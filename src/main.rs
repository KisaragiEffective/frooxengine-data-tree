#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "alloc")]
use alloc::string::String;
use alloc::borrow::Cow;
use core::fmt::{Display, Formatter};
#[cfg(feature = "serde")]
use serde::{Deserialize, Deserializer};
#[cfg(feature = "serde")]
use serde::de::DeserializeOwned;
#[cfg(feature = "std")]
use thiserror::Error;

#[derive(Debug)]
#[cfg_attr(feature = "std", derive(Error))]
enum FrooxContainerExtractError {
    #[cfg_attr(feature = "std", error("FrDT magic number is corrupted"))]
    InvalidFirstMagicNumber,
    #[cfg_attr(feature = "std", error("Reserved header is corrupted"))]
    InvalidSecondMagicNumber,
    #[cfg_attr(feature = "std", error("Compression algorithm indicator must be fit in 1-byte value"))]
    TooLargeForCompressionMethod,
    #[cfg_attr(feature = "std", error("Unknown compression method"))]
    UnknownCompressionMethod,
    #[cfg_attr(feature = "std", error("Got corrupted VarInt: {0}"))]
    VarIntDecodeError(String),
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u8)]
enum FrooxContainerCompressMethod {
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

fn split_froox_container_header(m: &[u8]) -> Result<FrooxContainer, FrooxContainerExtractError> {
    let Some(m) = m.strip_prefix(b"FrDT") else {
        return Err(FrooxContainerExtractError::InvalidFirstMagicNumber)
    };

    let Some(m) = m.strip_prefix(b"\0\0\0\0") else {
        return Err(FrooxContainerExtractError::InvalidSecondMagicNumber)
    };

    // It seems that froox container use var-int to express inner compress method.
    let (i, m) = varint_compression::decompress(m).map_err(|e| FrooxContainerExtractError::VarIntDecodeError(e.to_owned()))?;

    let Ok(i) = u8::try_from(i) else {
        return Err(FrooxContainerExtractError::TooLargeForCompressionMethod)
    };

    let compress_method = FrooxContainerCompressMethod::try_from(i)
        .map_err(|_| FrooxContainerExtractError::UnknownCompressionMethod)?;

    Ok(FrooxContainer {
        header: FrDT(()),
        compress_method,
        raw_content: m,
    })
}

#[cfg(feature = "serde")]
#[cfg_attr(feature = "std", derive(Error))]
#[derive(Debug)]
enum DeserializeError {
    #[cfg(feature = "lz4")]
    #[cfg_attr(feature = "std", error("lz4 decompressor: {0}"))]
    Lz4Decompression(#[from] Lz4DecompressionError),
    #[cfg(feature = "lzma")]
    #[cfg_attr(feature = "std", error("lzma decompressor: {0}"))]
    LzmaDecompression(#[from] lzma::Error),
    #[cfg(feature = "std")]
    #[cfg_attr(feature = "std", error("I/O stream error: {0}"))]
    Io(#[from] std::io::Error),
    #[cfg(not(all(feature = "lz4", feature = "lzma", feature = "brotli")))]
    #[cfg_attr(feature = "std", error("decompress: {0} is not installed (perhaps need re-compile?)"))]
    NonInstalledDecompressMethod(FrooxContainerCompressMethod),
    #[cfg_attr(feature = "std", error("bson: {0}"))]
    Bson(#[from] bson::de::Error),
}

#[cfg(feature = "std")]
#[derive(Debug, Error)]
#[error("{0}")]
struct Lz4DecompressionError(::std::io::Error);

#[cfg(not(feature = "std"))]
struct Lz4DecompressionError(());

#[derive(Debug)]
struct FrDT(());

#[derive(Debug)]
struct FrooxContainer<'a> {
    header: FrDT,
    compress_method: FrooxContainerCompressMethod,
    raw_content: &'a [u8],
}

impl<'a> FrooxContainer<'a> {
    #[cfg(all(feature = "serde", feature = "std"))]
    fn deserialize<T: DeserializeOwned>(&self) -> Result<T, DeserializeError> {
        use std::io::{Cursor, Read};
        let cursor = Cursor::new(self.raw_content);
        let after_decompress: Cow<'_, [u8]> = match self.compress_method {
            FrooxContainerCompressMethod::None => {
                self.raw_content.into()
            }
            #[cfg(feature = "lz4")]
            FrooxContainerCompressMethod::LZ4 => {
                // FIXME: deserialize: Io(Custom { kind: Other, error: LZ4Error("ERROR_frameType_unknown") })
                let mut buf = vec![];
                lz4::Decoder::new(cursor).map_err(Lz4DecompressionError)?.read_to_end(&mut buf)?;
                buf.into()
            }
            #[cfg(feature = "lzma")]
            FrooxContainerCompressMethod::LZMA => {
                let mut buf = vec![];
                lzma::read(cursor)?.read_to_end(&mut buf)?;
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

#[cfg(feature = "std")]
fn main() {
    use std::fs::File;
    use std::io::{BufReader, Read};
    let mut m = BufReader::new(
        File::open(
            std::env::args().next().unwrap()
        ).unwrap()
    );

    let mut buf = vec![];

    m.read_to_end(&mut buf).expect("bulk read");

    let m = split_froox_container_header(&buf).expect("raw");

    println!("{m:?}", m = &m);

    let r = m.deserialize::<bson::Bson>().expect("deserialize");

    println!("{r:?}");
}
