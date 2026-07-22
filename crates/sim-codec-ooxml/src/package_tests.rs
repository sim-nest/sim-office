use std::io::{Cursor, Write};

use sim_lib_doc_core::{OfficeError, ZipLimits};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::package::OoxmlPackage;

#[test]
fn ooxml_rejects_too_many_zip_entries() {
    let bytes = zip_bytes(
        &[("a.xml", b"a".as_slice()), ("b.xml", b"b".as_slice())],
        CompressionMethod::Stored,
    );

    let err = read_with_limits(&bytes, limits(1, 64, 256, 100));

    assert_package_limit(err, "entry count");
}

#[test]
fn ooxml_rejects_oversized_zip_entry_before_expansion() {
    let payload = vec![b'a'; 17];
    let bytes = zip_bytes(
        &[("xl/worksheets/sheet1.xml", &payload)],
        CompressionMethod::Stored,
    );

    let err = read_with_limits(&bytes, limits(8, 16, 256, 100));

    assert_package_limit(err, "entry bytes");
}

#[test]
fn ooxml_rejects_zip_entries_over_total_limit() {
    let first = vec![b'a'; 8];
    let second = vec![b'b'; 8];
    let bytes = zip_bytes(
        &[("first.xml", &first), ("second.xml", &second)],
        CompressionMethod::Stored,
    );

    let err = read_with_limits(&bytes, limits(8, 16, 12, 100));

    assert_package_limit(err, "total bytes");
}

#[test]
fn ooxml_rejects_implausible_zip_compression_ratio() {
    let payload = vec![b'a'; 4_096];
    let bytes = zip_bytes(
        &[("xl/sharedStrings.xml", &payload)],
        CompressionMethod::Deflated,
    );

    let err = read_with_limits(&bytes, limits(8, 8_192, 16_384, 2));

    assert_package_limit(err, "compression ratio");
}

fn read_with_limits(bytes: &[u8], limits: ZipLimits) -> OfficeError {
    OoxmlPackage::read_with_limits(bytes, ".xlsx", &limits)
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

fn zip_bytes(entries: &[(&str, &[u8])], compression: CompressionMethod) -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = ZipWriter::new(&mut cursor);
        let options = SimpleFileOptions::default().compression_method(compression);
        for (name, data) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap();
    }
    cursor.into_inner()
}
