use std::io::{Cursor, Write};

use sim_lib_doc_core::{OfficeError, ZipLimits};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::package::{ODS_MIMETYPE, OdfPackage};

#[test]
fn odf_rejects_too_many_zip_entries() {
    let bytes = odf_zip(&[("content.xml", b"a".as_slice(), CompressionMethod::Stored)]);

    let err = read_with_limits(&bytes, limits(1, 128, 512, 100));

    assert_package_limit(err, "entry count");
}

#[test]
fn odf_rejects_oversized_zip_entry_before_expansion() {
    let payload = vec![b'a'; 80];
    let bytes = odf_zip(&[("content.xml", &payload, CompressionMethod::Stored)]);

    let err = read_with_limits(&bytes, limits(8, 64, 512, 100));

    assert_package_limit(err, "entry bytes");
}

#[test]
fn odf_rejects_zip_entries_over_total_limit() {
    let first = vec![b'a'; 8];
    let second = vec![b'b'; 8];
    let max_total = ODS_MIMETYPE.len() as u64 + 12;
    let bytes = odf_zip(&[
        ("content.xml", &first, CompressionMethod::Stored),
        ("styles.xml", &second, CompressionMethod::Stored),
    ]);

    let err = read_with_limits(&bytes, limits(8, 128, max_total, 100));

    assert_package_limit(err, "total bytes");
}

#[test]
fn odf_rejects_implausible_zip_compression_ratio() {
    let payload = vec![b'a'; 4_096];
    let max_total = ODS_MIMETYPE.len() as u64 + payload.len() as u64 + 64;
    let bytes = odf_zip(&[("content.xml", &payload, CompressionMethod::Deflated)]);

    let err = read_with_limits(&bytes, limits(8, 8_192, max_total, 2));

    assert_package_limit(err, "compression ratio");
}

fn read_with_limits(bytes: &[u8], limits: ZipLimits) -> OfficeError {
    OdfPackage::read_with_limits(bytes, &limits)
        .err()
        .expect("package should exceed the ZIP limits")
}

fn limits(
    max_entries: usize,
    max_entry_bytes: u64,
    max_total_bytes: u64,
    max_ratio: u64,
) -> ZipLimits {
    ZipLimits {
        max_entries,
        max_entry_bytes,
        max_total_bytes,
        max_ratio,
    }
}

fn assert_package_limit(error: OfficeError, expected_limit: &'static str) {
    match error {
        OfficeError::PackageTooLarge { limit, .. } => assert_eq!(limit, expected_limit),
        other => panic!("expected PackageTooLarge, got {other:?}"),
    }
}

fn odf_zip(entries: &[(&str, &[u8], CompressionMethod)]) -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = ZipWriter::new(&mut cursor);
        let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        writer.start_file("mimetype", stored).unwrap();
        writer.write_all(ODS_MIMETYPE.as_bytes()).unwrap();
        for (name, data, compression) in entries {
            let options = SimpleFileOptions::default().compression_method(*compression);
            writer.start_file(*name, options).unwrap();
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap();
    }
    cursor.into_inner()
}
