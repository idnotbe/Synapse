use std::{fs, path::PathBuf};

use synapse_core::{Backend, ProfileUseScope};
use synapse_profiles::{
    ProfileError, package_manifest_digest, parse_package_manifest_bytes_with_digest,
    parse_package_manifest_file,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn package_manifest_accepts_happy_fixture() -> TestResult {
    let path = fixture("happy_package_manifest.toml");
    let bytes = fs::read(&path)?;
    let digest = package_manifest_digest(&bytes);
    let manifest = parse_package_manifest_bytes_with_digest(&path, &bytes, &digest)?;

    assert_eq!(manifest.package_id, "profile.luanti.minetest");
    assert_eq!(manifest.profile_id, "luanti.minetest");
    assert_eq!(
        manifest.permissions.use_scope,
        ProfileUseScope::OperatorOwnedTest
    );
    assert_eq!(
        manifest.input.backends,
        [Backend::Software, Backend::Hardware, Backend::Vigem]
    );
    assert_eq!(
        manifest.targets[0].process_name.as_deref(),
        Some("luanti.exe")
    );
    println!(
        "readback=package_manifest edge=happy before=fixture_path:{} after_digest={} package_id={} profile_id={} target_id={}",
        path.display(),
        digest,
        manifest.package_id,
        manifest.profile_id,
        manifest.targets[0].target_id
    );
    Ok(())
}

#[test]
fn package_manifest_rejects_missing_provenance_fixture() {
    let path = fixture("edge_missing_provenance_manifest.toml");
    let result = parse_package_manifest_file(&path);
    let Err(error) = result else {
        panic!("missing provenance fixture parsed successfully");
    };
    assert!(matches!(error, ProfileError::Parse { .. }));
    assert!(error.to_string().contains("missing field `source`"));
}

#[test]
fn package_manifest_rejects_incompatible_target_fixture() {
    let path = fixture("edge_incompatible_target_manifest.toml");
    let result = parse_package_manifest_file(&path);
    let Err(error) = result else {
        panic!("incompatible target fixture parsed successfully");
    };
    assert!(matches!(error, ProfileError::Parse { .. }));
    assert!(error.to_string().contains("assumptions.os"));
}

#[test]
fn package_manifest_rejects_manifest_digest_mismatch() -> TestResult {
    let path = fixture("happy_package_manifest.toml");
    let bytes = fs::read(&path)?;
    let result = parse_package_manifest_bytes_with_digest(
        &path,
        &bytes,
        "sha256:9999999999999999999999999999999999999999999999999999999999999999",
    );
    let Err(error) = result else {
        panic!("digest mismatch parsed successfully");
    };
    assert!(matches!(error, ProfileError::Parse { .. }));
    assert!(error.to_string().contains("manifest digest mismatch"));
    Ok(())
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("docs")
        .join("computergames")
        .join("fixtures")
        .join("profile_package_manifest")
        .join(name)
}
