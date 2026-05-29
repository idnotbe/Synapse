use std::{fs, path::PathBuf};

use synapse_everquest::{
    EverQuestMapInventoryError, EverQuestMapSetKind, MAP_DIR_NAME, inventory_map_set,
};

#[test]
fn inventories_default_map_set() -> Result<(), EverQuestMapInventoryError> {
    let temp = tempfile::tempdir().map_err(io_error)?;
    let maps = temp.path().join(MAP_DIR_NAME);
    fs::create_dir(&maps).map_err(io_error)?;
    fs::write(
        maps.join("neriaka.txt"),
        "P 1, 2, 3, 0, 0, 0, 3, to_Nektulos_Forest\nP 4, 5, 6, 0, 0, 0, 3, to_Nektulos_Forest\n",
    )
    .map_err(io_error)?;

    let inventory = inventory_map_set(temp.path(), None)?;

    assert_eq!(inventory.set_name, "default");
    assert_eq!(inventory.file_count, 1);
    assert_eq!(inventory.parseable_file_count, 1);
    assert_eq!(inventory.skipped_file_count, 0);
    assert_eq!(inventory.zone_count, 1);
    assert_eq!(inventory.total_point_count, 2);
    assert_eq!(inventory.duplicate_label_count, 1);
    assert!(inventory.aggregate_sha256.starts_with("sha256:"));
    Ok(())
}

#[test]
fn records_corrupt_map_as_skipped() -> Result<(), EverQuestMapInventoryError> {
    let temp = tempfile::tempdir().map_err(io_error)?;
    let maps = temp.path().join(MAP_DIR_NAME).join("Brewall");
    fs::create_dir_all(&maps).map_err(io_error)?;
    fs::write(maps.join("bad.txt"), "Q invalid\n").map_err(io_error)?;

    let inventory = inventory_map_set(temp.path(), Some("Brewall"))?;

    assert_eq!(inventory.source_kind, EverQuestMapSetKind::MapsSubdirectory);
    assert_eq!(inventory.file_count, 1);
    assert_eq!(inventory.parseable_file_count, 0);
    assert_eq!(inventory.skipped_file_count, 1);
    assert_eq!(
        inventory.skipped_files[0].relative_path,
        PathBuf::from("bad.txt")
    );
    Ok(())
}

fn io_error(source: std::io::Error) -> EverQuestMapInventoryError {
    EverQuestMapInventoryError::Io {
        path: PathBuf::from("test"),
        source,
    }
}
