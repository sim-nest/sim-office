const DOC_LEDGER_MANIFEST: &str = include_str!("../../crates/sim-lib-doc-ledger/Cargo.toml");
const OFFICE_PACK_MANIFEST: &str = include_str!("../../crates/sim-lib-office-pack/Cargo.toml");
const LEDGER_GIT: &str = r#"git = "https://github.com/sim-nest/sim-ledger""#;

#[test]
fn ledger_git_dependencies_share_one_rev() {
    let manifests = [
        ("sim-lib-doc-ledger", DOC_LEDGER_MANIFEST),
        ("sim-lib-office-pack", OFFICE_PACK_MANIFEST),
    ];
    let revs = manifests
        .iter()
        .flat_map(|(name, manifest)| ledger_git_revs(name, manifest))
        .collect::<Vec<_>>();
    assert!(
        !revs.is_empty(),
        "expected at least one sim-ledger git dependency rev"
    );
    let expected = &revs[0].1;
    for (dependency, rev) in &revs {
        assert_eq!(
            rev, expected,
            "{dependency} uses sim-ledger rev {rev}, expected {expected}"
        );
    }
}

fn ledger_git_revs(manifest_name: &str, manifest: &str) -> Vec<(String, String)> {
    let mut current_dep = None;
    let mut current_uses_ledger = false;
    let mut revs = Vec::new();
    for line in manifest.lines() {
        let trimmed = line.trim();
        if let Some(dependency) = dependency_header(trimmed) {
            current_dep = Some(format!("{manifest_name}:{dependency}"));
            current_uses_ledger = false;
            continue;
        }
        if trimmed == LEDGER_GIT {
            current_uses_ledger = true;
            continue;
        }
        if current_uses_ledger && let Some(rev) = quoted_value(trimmed, "rev") {
            if let Some(dependency) = current_dep.take() {
                revs.push((dependency, rev.to_owned()));
            }
            current_uses_ledger = false;
        }
    }
    revs
}

fn dependency_header(line: &str) -> Option<&str> {
    line.strip_prefix("[dependencies.")
        .and_then(|suffix| suffix.strip_suffix(']'))
}

fn quoted_value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let prefix = format!("{key} = \"");
    line.strip_prefix(&prefix)
        .and_then(|suffix| suffix.strip_suffix('"'))
}
