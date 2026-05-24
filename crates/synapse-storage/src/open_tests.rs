use super::*;
use std::{error::Error, fs};

use rocksdb::DEFAULT_COLUMN_FAMILY_NAME;
use synapse_core::error_codes;

const TEST_SCHEMA_VERSION: u32 = 7;
const DURABILITY_KEY: &[u8] = b"restart-durability-key";
const DURABILITY_VALUE: &[u8] = b"restart-durability-value";

#[test]
fn open_fresh_db_creates_all_prd_cfs_and_schema_sentinel_with_fsv() -> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("db");
    println!(
        "source_of_truth=db.list_cf edge=fresh before_path={} before={:?}",
        path.display(),
        list_cf_for_fsv(&path)
    );

    let db = Db::open(&path, TEST_SCHEMA_VERSION)?;
    let handles = existing_prd_handles(&db);
    let sentinel = db.inner.get(SCHEMA_VERSION_KEY)?;
    let expected_schema = TEST_SCHEMA_VERSION.to_be_bytes();
    println!(
        "source_of_truth=db.list_cf edge=fresh after_handles={handles:?} after_sentinel={:?} final_value=count:{}",
        sentinel.as_deref(),
        handles.len()
    );
    assert_eq!(handles, sorted_prd_cfs());
    assert_eq!(sentinel.as_deref(), Some(expected_schema.as_slice()));
    drop(db);

    let physical = sorted_list_cf(&path)?;
    println!("source_of_truth=db.list_cf edge=fresh physical_after_drop={physical:?}");
    assert_eq!(physical, sorted_physical_cfs());
    Ok(())
}

#[test]
fn mismatched_schema_errors_then_wipe_retry_succeeds_with_fsv() -> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("db");
    let db = Db::open(&path, 1)?;
    let before = db.inner.get(SCHEMA_VERSION_KEY)?;
    drop(db);
    println!(
        "source_of_truth=db.schema edge=mismatch before_path={} before_sentinel={before:?}",
        path.display()
    );

    let error = match Db::open(&path, TEST_SCHEMA_VERSION) {
        Ok(db) => panic!("Db::open unexpectedly accepted mismatched schema: {db:?}"),
        Err(error) => error,
    };
    println!(
        "source_of_truth=db.schema edge=mismatch after_code={} after_db_exists={}",
        error.code(),
        path.exists()
    );
    assert_eq!(error.code(), error_codes::STORAGE_SCHEMA_MISMATCH);

    fs::remove_dir_all(&path)?;
    println!(
        "source_of_truth=db.schema edge=mismatch after_wipe_exists={}",
        path.exists()
    );
    let db = Db::open(&path, TEST_SCHEMA_VERSION)?;
    let after = db.inner.get(SCHEMA_VERSION_KEY)?;
    let expected_schema = TEST_SCHEMA_VERSION.to_be_bytes();
    println!(
        "source_of_truth=db.schema edge=mismatch retry_sentinel={after:?} final_value=schema_version:{}",
        db.schema_version
    );
    assert_eq!(after.as_deref(), Some(expected_schema.as_slice()));
    Ok(())
}

#[test]
fn process_restart_reads_persisted_key_with_fsv() -> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("db");
    let db = Db::open(&path, TEST_SCHEMA_VERSION)?;
    {
        let cf = db
            .inner
            .cf_handle(cf::CF_KV)
            .ok_or("CF_KV handle missing after open")?;
        println!(
            "source_of_truth=db.restart edge=durability before_key={:?} before_value={:?}",
            String::from_utf8_lossy(DURABILITY_KEY),
            db.inner.get_cf(&cf, DURABILITY_KEY)?
        );
        db.inner.put_cf(&cf, DURABILITY_KEY, DURABILITY_VALUE)?;
        db.inner.flush_cf(&cf)?;
    }
    drop(db);

    let db = Db::open(&path, TEST_SCHEMA_VERSION)?;
    let cf = db
        .inner
        .cf_handle(cf::CF_KV)
        .ok_or("CF_KV handle missing after reopen")?;
    let after = db.inner.get_cf(&cf, DURABILITY_KEY)?;
    println!(
        "source_of_truth=db.restart edge=durability after_value={after:?} final_value={:?}",
        Some(DURABILITY_VALUE)
    );
    assert_eq!(after.as_deref(), Some(DURABILITY_VALUE));
    Ok(())
}

#[test]
fn file_path_open_fails_with_storage_open_failed_fsv() -> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("db-file");
    fs::write(&path, b"not a directory")?;
    println!(
        "source_of_truth=db.list_cf edge=file_path before_path={} before_is_file={}",
        path.display(),
        path.is_file()
    );
    let error = match Db::open(&path, TEST_SCHEMA_VERSION) {
        Ok(db) => panic!("Db::open unexpectedly accepted file path: {db:?}"),
        Err(error) => error,
    };
    println!(
        "source_of_truth=db.list_cf edge=file_path after_code={} after_detail={:?} after_is_file={}",
        error.code(),
        open_error_detail(&error),
        path.is_file()
    );
    assert_eq!(error.code(), error_codes::STORAGE_OPEN_FAILED);
    Ok(())
}

#[cfg(windows)]
#[test]
fn non_writable_path_returns_storage_open_failed_with_fsv() -> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("locked-db");
    fs::create_dir_all(&path)?;
    let _guard = deny_current_user_write(&path)?;
    println!(
        "source_of_truth=db.list_cf edge=permission before_path={} before={:?}",
        path.display(),
        list_cf_for_fsv(&path)
    );

    let error = match Db::open(&path, TEST_SCHEMA_VERSION) {
        Ok(db) => panic!("Db::open unexpectedly accepted non-writable path: {db:?}"),
        Err(error) => error,
    };
    println!(
        "source_of_truth=db.list_cf edge=permission after_code={} after_detail={:?} after_exists={}",
        error.code(),
        open_error_detail(&error),
        path.exists()
    );
    assert_eq!(error.code(), error_codes::STORAGE_OPEN_FAILED);
    assert!(open_error_detail(&error).contains("Access is denied"));
    Ok(())
}

fn existing_prd_handles(db: &Db) -> Vec<&'static str> {
    let mut names = cf::ALL_COLUMN_FAMILIES
        .into_iter()
        .filter(|name| db.inner.cf_handle(name).is_some())
        .collect::<Vec<_>>();
    names.sort_unstable();
    names
}

fn sorted_prd_cfs() -> Vec<&'static str> {
    let mut names = cf::ALL_COLUMN_FAMILIES.to_vec();
    names.sort_unstable();
    names
}

fn sorted_physical_cfs() -> Vec<String> {
    let mut names = sorted_prd_cfs()
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    names.push(DEFAULT_COLUMN_FAMILY_NAME.to_owned());
    names.sort_unstable();
    names
}

fn sorted_list_cf(path: &Path) -> Result<Vec<String>, Box<dyn Error>> {
    let mut cfs = DB::list_cf(&Options::default(), path)?;
    cfs.sort_unstable();
    Ok(cfs)
}

fn list_cf_for_fsv(path: &Path) -> Result<Vec<String>, String> {
    sorted_list_cf(path).map_err(|error| error.to_string())
}

fn open_error_detail(error: &StorageError) -> &str {
    match error {
        StorageError::OpenFailed { detail, .. } => detail,
        _ => "",
    }
}

#[cfg(windows)]
struct DenyWriteGuard {
    path: std::path::PathBuf,
    principal: String,
}

#[cfg(windows)]
impl Drop for DenyWriteGuard {
    fn drop(&mut self) {
        let _ = std::process::Command::new("icacls")
            .arg(&self.path)
            .args(["/remove:d", &self.principal])
            .output();
    }
}

#[cfg(windows)]
fn deny_current_user_write(path: &Path) -> Result<DenyWriteGuard, Box<dyn Error>> {
    let domain = std::env::var("USERDOMAIN")?;
    let username = std::env::var("USERNAME")?;
    let principal = format!(r"{domain}\{username}");
    let deny = format!("{principal}:(W)");
    let output = std::process::Command::new("icacls")
        .arg(path)
        .args(["/deny", &deny])
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "icacls deny failed: status={} stdout={} stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(DenyWriteGuard {
        path: path.to_path_buf(),
        principal,
    })
}
