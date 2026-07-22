//! Bounded ZIP package extraction for office codecs.

use std::collections::BTreeMap;
use std::io::{Cursor, Read};

use zip::ZipArchive;

use crate::OfficeError;

/// Limits applied while extracting an office ZIP container.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ZipLimits {
    /// Maximum ZIP entries, including directories.
    pub max_entries: usize,
    /// Maximum decompressed bytes retained for one file entry.
    pub max_entry_bytes: u64,
    /// Maximum decompressed bytes retained across all file entries.
    pub max_total_bytes: u64,
    /// Maximum declared uncompressed-to-compressed ratio for one file entry.
    pub max_ratio: u64,
}

impl ZipLimits {
    /// Production limits for ordinary office documents.
    #[must_use]
    pub const fn office() -> Self {
        Self {
            max_entries: 4_096,
            max_entry_bytes: 64 * 1024 * 1024,
            max_total_bytes: 256 * 1024 * 1024,
            max_ratio: 200,
        }
    }
}

/// Reads a ZIP archive into normalized entry names and bounded byte buffers.
pub fn read_zip_entries(
    bytes: &[u8],
    limits: &ZipLimits,
) -> Result<BTreeMap<String, Vec<u8>>, OfficeError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(zip_error)?;
    if archive.len() > limits.max_entries {
        return Err(package_too_large(
            "entry count",
            format!(
                "archive declares {} entries; limit is {}",
                archive.len(),
                limits.max_entries
            ),
        ));
    }

    let mut total = 0_u64;
    let mut entries = BTreeMap::new();
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(zip_error)?;
        if file.is_dir() {
            continue;
        }
        let name = file.name().replace('\\', "/");
        let declared = file.size();
        let compressed = file.compressed_size();
        reject_declared_entry_size(&name, declared, limits)?;
        reject_compression_ratio(&name, declared, compressed, limits)?;
        reject_declared_total(&name, total, declared, limits)?;

        let mut data = Vec::new();
        let read_limit = limits.max_entry_bytes.saturating_add(1).min(
            limits
                .max_total_bytes
                .saturating_sub(total)
                .saturating_add(1),
        );
        (&mut file)
            .take(read_limit)
            .read_to_end(&mut data)
            .map_err(|err| {
                OfficeError::Kernel(format!("could not read zip entry {name}: {err}"))
            })?;
        let actual = data.len() as u64;
        if actual > limits.max_entry_bytes {
            return Err(package_too_large(
                "entry bytes",
                format!("entry {name} expands past {} bytes", limits.max_entry_bytes),
            ));
        }
        total = total.checked_add(actual).ok_or_else(|| {
            package_too_large(
                "total bytes",
                format!("entry {name} overflows total byte accounting"),
            )
        })?;
        if total > limits.max_total_bytes {
            return Err(package_too_large(
                "total bytes",
                format!(
                    "archive expands past {} bytes at entry {name}",
                    limits.max_total_bytes
                ),
            ));
        }
        entries.insert(name, data);
    }
    Ok(entries)
}

fn reject_declared_entry_size(
    name: &str,
    declared: u64,
    limits: &ZipLimits,
) -> Result<(), OfficeError> {
    if declared > limits.max_entry_bytes {
        return Err(package_too_large(
            "entry bytes",
            format!(
                "entry {name} declares {declared} bytes; limit is {}",
                limits.max_entry_bytes
            ),
        ));
    }
    Ok(())
}

fn reject_declared_total(
    name: &str,
    total: u64,
    declared: u64,
    limits: &ZipLimits,
) -> Result<(), OfficeError> {
    if total.saturating_add(declared) > limits.max_total_bytes {
        return Err(package_too_large(
            "total bytes",
            format!(
                "archive declares more than {} bytes at entry {name}",
                limits.max_total_bytes
            ),
        ));
    }
    Ok(())
}

fn reject_compression_ratio(
    name: &str,
    declared: u64,
    compressed: u64,
    limits: &ZipLimits,
) -> Result<(), OfficeError> {
    if declared == 0 {
        return Ok(());
    }
    let ratio_limit = compressed.saturating_mul(limits.max_ratio);
    if compressed == 0 || declared > ratio_limit {
        return Err(package_too_large(
            "compression ratio",
            format!(
                "entry {name} declares {declared} bytes from {compressed} compressed bytes; ratio limit is {}",
                limits.max_ratio
            ),
        ));
    }
    Ok(())
}

fn zip_error(error: zip::result::ZipError) -> OfficeError {
    OfficeError::Kernel(format!("invalid ZIP package: {error}"))
}

fn package_too_large(limit: &'static str, detail: String) -> OfficeError {
    OfficeError::PackageTooLarge { limit, detail }
}
