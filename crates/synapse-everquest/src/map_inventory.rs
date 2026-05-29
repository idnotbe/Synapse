use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::map::{EverQuestMapError, EverQuestMapRecord, MAP_DIR_NAME, parse_map_file};

const DEFAULT_MAX_MAP_FILES: usize = 4096;
const MAX_SAMPLE_ROWS: usize = 64;

#[derive(Debug, Error)]
pub enum EverQuestMapInventoryError {
    #[error("EverQuest map inventory path {path} is invalid: {reason}")]
    InvalidPath { path: PathBuf, reason: String },
    #[error("I/O error while reading EverQuest map inventory path {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(
        "EverQuest map set {path} has {file_count} map files, exceeding the {max_files} file limit"
    )]
    TooManyFiles {
        path: PathBuf,
        file_count: usize,
        max_files: usize,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EverQuestMapSetKind {
    BaseMapsDirectory,
    MapsSubdirectory,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EverQuestMapSetInventory {
    pub schema_version: u32,
    pub install_root: PathBuf,
    pub maps_root: PathBuf,
    pub set_name: String,
    pub set_dir: PathBuf,
    pub source_kind: EverQuestMapSetKind,
    pub file_count: usize,
    pub parseable_file_count: usize,
    pub skipped_file_count: usize,
    pub total_bytes: u64,
    pub aggregate_sha256: String,
    pub earliest_modified_unix_ms: Option<i64>,
    pub latest_modified_unix_ms: Option<i64>,
    pub total_line_count: usize,
    pub total_segment_count: usize,
    pub total_point_count: usize,
    pub zone_count: usize,
    pub duplicate_zone_count: usize,
    pub duplicate_zones: Vec<EverQuestDuplicateZone>,
    pub duplicate_label_count: usize,
    pub duplicate_label_samples: Vec<EverQuestDuplicateLabel>,
    pub file_samples: Vec<EverQuestMapFileInventory>,
    pub skipped_files: Vec<EverQuestSkippedMapFile>,
    pub samples_truncated: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EverQuestMapFileInventory {
    pub relative_path: PathBuf,
    pub zone_short_name: String,
    pub len_bytes: u64,
    pub sha256: String,
    pub last_modified_unix_ms: Option<i64>,
    pub line_count: usize,
    pub segment_count: usize,
    pub point_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EverQuestSkippedMapFile {
    pub relative_path: PathBuf,
    pub len_bytes: u64,
    pub sha256: String,
    pub error: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EverQuestDuplicateZone {
    pub zone_short_name: String,
    pub count: usize,
    pub relative_paths: Vec<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EverQuestDuplicateLabel {
    pub zone_short_name: String,
    pub normalized_label: String,
    pub count: usize,
    pub labels: Vec<String>,
}

/// Inventory one local `EverQuest` map set using the default file cap.
///
/// # Errors
///
/// Returns [`EverQuestMapInventoryError`] when the install root, maps
/// directory, selected set directory, or map files cannot be read.
pub fn inventory_map_set(
    install_root: &Path,
    set_name: Option<&str>,
) -> Result<EverQuestMapSetInventory, EverQuestMapInventoryError> {
    inventory_map_set_with_limit(install_root, set_name, DEFAULT_MAX_MAP_FILES)
}

/// Inventory one local `EverQuest` map set using a caller-supplied file cap.
///
/// # Errors
///
/// Returns [`EverQuestMapInventoryError`] when the install root, maps
/// directory, selected set directory, or map files cannot be read.
pub fn inventory_map_set_with_limit(
    install_root: &Path,
    set_name: Option<&str>,
    max_files: usize,
) -> Result<EverQuestMapSetInventory, EverQuestMapInventoryError> {
    let maps_root = install_root.join(MAP_DIR_NAME);
    require_dir(&maps_root, "maps directory is absent")?;
    let (normalized_set_name, set_dir, source_kind) = resolve_set_dir(&maps_root, set_name)?;
    require_dir(&set_dir, "map set directory is absent")?;

    let files = map_text_files(&set_dir)?;
    if files.len() > max_files {
        return Err(EverQuestMapInventoryError::TooManyFiles {
            path: set_dir,
            file_count: files.len(),
            max_files,
        });
    }

    let mut aggregate_hasher = Sha256::new();
    let mut inventory = new_inventory(
        install_root,
        &maps_root,
        &normalized_set_name,
        &set_dir,
        source_kind,
    );
    let mut zone_paths = BTreeMap::<String, Vec<PathBuf>>::new();
    let mut label_counts = BTreeMap::<(String, String), DuplicateLabelAccumulator>::new();

    for path in files {
        add_file_to_inventory(
            &set_dir,
            &path,
            &mut aggregate_hasher,
            &mut inventory,
            &mut zone_paths,
            &mut label_counts,
        )?;
    }

    inventory.aggregate_sha256 = format_sha256(aggregate_hasher.finalize().as_ref());
    inventory.zone_count = zone_paths.len();
    let duplicate_zone_rows = duplicate_zones(zone_paths);
    inventory.duplicate_zone_count = duplicate_zone_rows.len();
    inventory.duplicate_zones = duplicate_zone_rows
        .into_iter()
        .take(MAX_SAMPLE_ROWS)
        .collect();
    let duplicate_label_rows = duplicate_labels(label_counts);
    inventory.duplicate_label_count = duplicate_label_rows.len();
    inventory.duplicate_label_samples = duplicate_label_rows
        .into_iter()
        .take(MAX_SAMPLE_ROWS)
        .collect();
    inventory.samples_truncated = inventory.file_count > MAX_SAMPLE_ROWS
        || inventory.skipped_file_count > MAX_SAMPLE_ROWS
        || inventory.duplicate_zone_count > inventory.duplicate_zones.len()
        || inventory.duplicate_label_count > inventory.duplicate_label_samples.len();

    Ok(inventory)
}

/// Compute a SHA-256 digest for one local file.
///
/// # Errors
///
/// Returns [`EverQuestMapInventoryError::Io`] when the file cannot be opened
/// or read.
pub fn sha256_file(path: &Path) -> Result<String, EverQuestMapInventoryError> {
    let mut file = fs::File::open(path).map_err(|source| EverQuestMapInventoryError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|source| EverQuestMapInventoryError::Io {
                path: path.to_path_buf(),
                source,
            })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format_sha256(hasher.finalize().as_ref()))
}

fn add_file_to_inventory(
    set_dir: &Path,
    path: &Path,
    aggregate_hasher: &mut Sha256,
    inventory: &mut EverQuestMapSetInventory,
    zone_paths: &mut BTreeMap<String, Vec<PathBuf>>,
    label_counts: &mut BTreeMap<(String, String), DuplicateLabelAccumulator>,
) -> Result<(), EverQuestMapInventoryError> {
    let metadata = fs::metadata(path).map_err(|source| EverQuestMapInventoryError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let relative_path = relative_path(set_dir, path);
    let sha256 = sha256_file(path)?;
    inventory.file_count += 1;
    inventory.total_bytes += metadata.len();
    update_modified_bounds(inventory, &metadata);
    aggregate_file_hash(aggregate_hasher, &relative_path, metadata.len(), &sha256);

    match parse_map_file(path) {
        Ok(map) => {
            let zone = map.source.zone_short_name.clone();
            inventory.parseable_file_count += 1;
            inventory.total_line_count += map.line_count;
            inventory.total_segment_count += map.segment_count;
            inventory.total_point_count += map.point_count;
            zone_paths
                .entry(zone.clone())
                .or_default()
                .push(relative_path.clone());
            record_duplicate_labels(&zone, &map.records, label_counts);
            if inventory.file_samples.len() < MAX_SAMPLE_ROWS {
                inventory.file_samples.push(EverQuestMapFileInventory {
                    relative_path,
                    zone_short_name: zone,
                    len_bytes: map.source.len_bytes,
                    sha256,
                    last_modified_unix_ms: map.source.last_modified_unix_ms,
                    line_count: map.line_count,
                    segment_count: map.segment_count,
                    point_count: map.point_count,
                });
            }
        }
        Err(error) => {
            inventory.skipped_file_count += 1;
            if inventory.skipped_files.len() < MAX_SAMPLE_ROWS {
                inventory.skipped_files.push(skipped_file(
                    relative_path,
                    metadata.len(),
                    sha256,
                    &error,
                ));
            }
        }
    }
    Ok(())
}

fn new_inventory(
    install_root: &Path,
    maps_root: &Path,
    set_name: &str,
    set_dir: &Path,
    source_kind: EverQuestMapSetKind,
) -> EverQuestMapSetInventory {
    EverQuestMapSetInventory {
        schema_version: 1,
        install_root: install_root.to_path_buf(),
        maps_root: maps_root.to_path_buf(),
        set_name: set_name.to_owned(),
        set_dir: set_dir.to_path_buf(),
        source_kind,
        file_count: 0,
        parseable_file_count: 0,
        skipped_file_count: 0,
        total_bytes: 0,
        aggregate_sha256: String::new(),
        earliest_modified_unix_ms: None,
        latest_modified_unix_ms: None,
        total_line_count: 0,
        total_segment_count: 0,
        total_point_count: 0,
        zone_count: 0,
        duplicate_zone_count: 0,
        duplicate_zones: Vec::new(),
        duplicate_label_count: 0,
        duplicate_label_samples: Vec::new(),
        file_samples: Vec::new(),
        skipped_files: Vec::new(),
        samples_truncated: false,
    }
}

fn resolve_set_dir(
    maps_root: &Path,
    set_name: Option<&str>,
) -> Result<(String, PathBuf, EverQuestMapSetKind), EverQuestMapInventoryError> {
    let Some(name) = set_name else {
        return Ok((
            "default".to_owned(),
            maps_root.to_path_buf(),
            EverQuestMapSetKind::BaseMapsDirectory,
        ));
    };
    let trimmed = name.trim();
    if trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case("default")
        || trimmed.eq_ignore_ascii_case("base")
    {
        return Ok((
            "default".to_owned(),
            maps_root.to_path_buf(),
            EverQuestMapSetKind::BaseMapsDirectory,
        ));
    }
    let path_name = Path::new(trimmed);
    if path_name.components().count() != 1 {
        return Err(EverQuestMapInventoryError::InvalidPath {
            path: maps_root.join(trimmed),
            reason: "map set name must be one directory name".to_owned(),
        });
    }
    Ok((
        trimmed.to_owned(),
        maps_root.join(trimmed),
        EverQuestMapSetKind::MapsSubdirectory,
    ))
}

fn require_dir(path: &Path, reason: &str) -> Result<(), EverQuestMapInventoryError> {
    if path.is_dir() {
        Ok(())
    } else {
        Err(EverQuestMapInventoryError::InvalidPath {
            path: path.to_path_buf(),
            reason: reason.to_owned(),
        })
    }
}

fn map_text_files(set_dir: &Path) -> Result<Vec<PathBuf>, EverQuestMapInventoryError> {
    let mut files = Vec::new();
    for entry in fs::read_dir(set_dir).map_err(|source| EverQuestMapInventoryError::Io {
        path: set_dir.to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| EverQuestMapInventoryError::Io {
            path: set_dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if is_map_text_path(&path) && path.is_file() {
            files.push(path);
        }
    }
    files.sort_by_key(|path| path.file_name().map(std::ffi::OsStr::to_os_string));
    Ok(files)
}

fn record_duplicate_labels(
    zone: &str,
    records: &[EverQuestMapRecord],
    label_counts: &mut BTreeMap<(String, String), DuplicateLabelAccumulator>,
) {
    for record in records {
        let EverQuestMapRecord::Point(point) = record else {
            continue;
        };
        let key = (zone.to_owned(), normalize_label(&point.label));
        let entry = label_counts.entry(key).or_default();
        entry.count += 1;
        if entry.labels.len() < 4 && !entry.labels.contains(&point.label) {
            entry.labels.push(point.label.clone());
        }
    }
}

fn duplicate_zones(zone_paths: BTreeMap<String, Vec<PathBuf>>) -> Vec<EverQuestDuplicateZone> {
    zone_paths
        .into_iter()
        .filter_map(|(zone_short_name, relative_paths)| {
            if relative_paths.len() > 1 {
                Some(EverQuestDuplicateZone {
                    zone_short_name,
                    count: relative_paths.len(),
                    relative_paths,
                })
            } else {
                None
            }
        })
        .collect()
}

fn duplicate_labels(
    label_counts: BTreeMap<(String, String), DuplicateLabelAccumulator>,
) -> Vec<EverQuestDuplicateLabel> {
    label_counts
        .into_iter()
        .filter_map(|((zone_short_name, normalized_label), accumulator)| {
            if accumulator.count > 1 {
                Some(EverQuestDuplicateLabel {
                    zone_short_name,
                    normalized_label,
                    count: accumulator.count,
                    labels: accumulator.labels,
                })
            } else {
                None
            }
        })
        .collect()
}

fn skipped_file(
    relative_path: PathBuf,
    len_bytes: u64,
    sha256: String,
    error: &EverQuestMapError,
) -> EverQuestSkippedMapFile {
    EverQuestSkippedMapFile {
        relative_path,
        len_bytes,
        sha256,
        error: error.to_string(),
    }
}

fn update_modified_bounds(inventory: &mut EverQuestMapSetInventory, metadata: &fs::Metadata) {
    let Some(modified_ms) = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
    else {
        return;
    };
    inventory.earliest_modified_unix_ms = Some(
        inventory
            .earliest_modified_unix_ms
            .map_or(modified_ms, |existing| existing.min(modified_ms)),
    );
    inventory.latest_modified_unix_ms = Some(
        inventory
            .latest_modified_unix_ms
            .map_or(modified_ms, |existing| existing.max(modified_ms)),
    );
}

fn aggregate_file_hash(
    aggregate_hasher: &mut Sha256,
    relative_path: &Path,
    len_bytes: u64,
    file_sha256: &str,
) {
    aggregate_hasher.update(relative_path.to_string_lossy().as_bytes());
    aggregate_hasher.update([0]);
    aggregate_hasher.update(len_bytes.to_le_bytes());
    aggregate_hasher.update([0]);
    aggregate_hasher.update(file_sha256.as_bytes());
    aggregate_hasher.update([0]);
}

fn relative_path(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
}

fn is_map_text_path(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("txt"))
}

fn normalize_label(label: &str) -> String {
    label
        .chars()
        .flat_map(char::to_lowercase)
        .filter(char::is_ascii_alphanumeric)
        .collect()
}

fn format_sha256(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2 + 7);
    output.push_str("sha256:");
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

#[derive(Default)]
struct DuplicateLabelAccumulator {
    count: usize,
    labels: Vec<String>,
}
