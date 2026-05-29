use std::{env, ffi::OsStr, fs, path::PathBuf, process};

use serde::Serialize;
use synapse_everquest::{
    EverQuestMapInventoryError, EverQuestMapSetInventory, inventory_map_set, sha256_file,
};

fn main() {
    let args = parse_args().unwrap_or_else(|error| {
        eprintln!("{error}");
        eprintln!(
            "usage: eq-map-inventory --root <everquest-install-root> [--set <name>] [--manifest-out <path>] [--archive <path>] [--source-url <url>] [--license-note <text>]"
        );
        process::exit(2);
    });

    match run(&args) {
        Ok(output) => {
            print!("{output}");
        }
        Err(error) => {
            eprintln!("error={error}");
            process::exit(1);
        }
    }
}

struct Args {
    root: PathBuf,
    set_name: Option<String>,
    manifest_out: Option<PathBuf>,
    archive: Option<PathBuf>,
    source_urls: Vec<String>,
    license_note: Option<String>,
}

#[derive(Serialize)]
struct EverQuestMapProvenanceManifest {
    schema_version: u32,
    generated_by: &'static str,
    inventory: EverQuestMapSetInventory,
    acquisition: EverQuestMapAcquisitionProvenance,
    rollback: EverQuestMapRollbackPlan,
}

#[derive(Serialize)]
struct EverQuestMapAcquisitionProvenance {
    source_urls: Vec<String>,
    original_archive_path: Option<PathBuf>,
    original_archive_sha256: Option<String>,
    license_note: Option<String>,
}

#[derive(Serialize)]
struct EverQuestMapRollbackPlan {
    base_maps_overwritten: bool,
    rollback_action: String,
    rollback_target: PathBuf,
}

fn run(args: &Args) -> Result<String, Box<dyn std::error::Error>> {
    let inventory = inventory_map_set(&args.root, args.set_name.as_deref())?;
    let manifest = build_manifest(inventory, args)?;
    let mut out = format_inventory(&manifest);
    if let Some(manifest_out) = &args.manifest_out {
        let json = serde_json::to_string_pretty(&manifest)?;
        if let Some(parent) = manifest_out
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }
        fs::write(manifest_out, json)?;
        push_line(&mut out, format!("manifest_out={}", manifest_out.display()));
    }
    Ok(out)
}

fn build_manifest(
    inventory: EverQuestMapSetInventory,
    args: &Args,
) -> Result<EverQuestMapProvenanceManifest, EverQuestMapInventoryError> {
    let archive_sha256 = args.archive.as_deref().map(sha256_file).transpose()?;
    let rollback_target = inventory.set_dir.clone();
    let base_maps_overwritten = false;
    let rollback_action = if inventory.set_name == "default" {
        "inventory-only; create a backup before any future base-map replacement".to_owned()
    } else {
        format!("delete map set directory {}", rollback_target.display())
    };
    Ok(EverQuestMapProvenanceManifest {
        schema_version: 1,
        generated_by: "eq-map-inventory",
        inventory,
        acquisition: EverQuestMapAcquisitionProvenance {
            source_urls: args.source_urls.clone(),
            original_archive_path: args.archive.clone(),
            original_archive_sha256: archive_sha256,
            license_note: args.license_note.clone(),
        },
        rollback: EverQuestMapRollbackPlan {
            base_maps_overwritten,
            rollback_action,
            rollback_target,
        },
    })
}

fn parse_args() -> Result<Args, String> {
    let mut args = env::args_os();
    let _program = args.next();
    let mut root = None;
    let mut set_name = None;
    let mut manifest_out = None;
    let mut archive = None;
    let mut source_urls = Vec::new();
    let mut license_note = None;

    while let Some(flag) = args.next() {
        if flag == OsStr::new("--root") {
            root = Some(required_path(&mut args, "--root")?);
        } else if flag == OsStr::new("--set") {
            set_name = Some(required_string(&mut args, "--set")?);
        } else if flag == OsStr::new("--manifest-out") {
            manifest_out = Some(required_path(&mut args, "--manifest-out")?);
        } else if flag == OsStr::new("--archive") {
            archive = Some(required_path(&mut args, "--archive")?);
        } else if flag == OsStr::new("--source-url") {
            source_urls.push(required_string(&mut args, "--source-url")?);
        } else if flag == OsStr::new("--license-note") {
            license_note = Some(required_string(&mut args, "--license-note")?);
        } else {
            return Err(format!(
                "unknown argument {}",
                PathBuf::from(flag).display()
            ));
        }
    }

    Ok(Args {
        root: root.ok_or_else(|| "--root is required".to_owned())?,
        set_name,
        manifest_out,
        archive,
        source_urls,
        license_note,
    })
}

fn format_inventory(manifest: &EverQuestMapProvenanceManifest) -> String {
    let inventory = &manifest.inventory;
    let mut out = String::new();
    push_line(
        &mut out,
        format!("install_root={}", inventory.install_root.display()),
    );
    push_line(
        &mut out,
        format!("maps_root={}", inventory.maps_root.display()),
    );
    push_line(&mut out, format!("set_name={}", inventory.set_name));
    push_line(&mut out, format!("set_dir={}", inventory.set_dir.display()));
    push_line(&mut out, format!("source_kind={:?}", inventory.source_kind));
    push_line(&mut out, format!("file_count={}", inventory.file_count));
    push_line(
        &mut out,
        format!("parseable_file_count={}", inventory.parseable_file_count),
    );
    push_line(
        &mut out,
        format!("skipped_file_count={}", inventory.skipped_file_count),
    );
    push_line(&mut out, format!("total_bytes={}", inventory.total_bytes));
    push_line(
        &mut out,
        format!("aggregate_sha256={}", inventory.aggregate_sha256),
    );
    push_line(&mut out, format!("zone_count={}", inventory.zone_count));
    push_line(
        &mut out,
        format!("duplicate_zone_count={}", inventory.duplicate_zone_count),
    );
    push_line(
        &mut out,
        format!("duplicate_label_count={}", inventory.duplicate_label_count),
    );
    push_line(
        &mut out,
        format!("samples_truncated={}", inventory.samples_truncated),
    );
    if let Some(sha256) = &manifest.acquisition.original_archive_sha256 {
        push_line(&mut out, format!("archive_sha256={sha256}"));
    }
    push_line(
        &mut out,
        format!(
            "base_maps_overwritten={}",
            manifest.rollback.base_maps_overwritten
        ),
    );
    push_line(
        &mut out,
        format!("rollback_action={}", manifest.rollback.rollback_action),
    );
    out
}

fn required_path(args: &mut env::ArgsOs, flag: &str) -> Result<PathBuf, String> {
    args.next()
        .map(PathBuf::from)
        .ok_or_else(|| format!("{flag} requires a path"))
}

fn required_string(args: &mut env::ArgsOs, flag: &str) -> Result<String, String> {
    let value = args
        .next()
        .ok_or_else(|| format!("{flag} requires a value"))?;
    value
        .into_string()
        .map_err(|_| format!("{flag} value must be valid UTF-8"))
}

fn push_line(out: &mut String, line: impl AsRef<str>) {
    out.push_str(line.as_ref());
    out.push('\n');
}
