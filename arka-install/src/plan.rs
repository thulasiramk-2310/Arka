//! Planning an install. The plan comes from systemd-repart's own dry run, so what the person
//! reviews is what repart will write. Nothing in this file writes to a disk.

use crate::disk::{self, Disk, Partition};
use crate::run;
use serde::{Deserialize, Serialize};

pub const GIB: u64 = 1 << 30;
pub const MIB: u64 = 1 << 20;
/// Smallest free area ArkaOS installs into (image, one update and the person's files).
pub const MIN_FREE: u64 = 40 * GIB;
/// Room ArkaOS needs on a shared EFI partition: about 8.3 MB measured, plus room for updates.
pub const MIN_ESP_FREE: u64 = 30 * MIB;
pub const BOOT_SIZE: u64 = 2 * GIB;
pub const ESP_SIZE: u64 = 600 * MIB;
pub const PLAN_DIR: &str = "/run/arka-install";
pub const DEFS_DIR: &str = "/run/arka-install/repart";

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Into the largest free area, next to whatever is on the disk.
    Free,
    /// A disk with no partitions.
    Empty,
    /// Erase the whole disk.
    Erase,
}

impl Mode {
    pub fn parse(s: &str) -> Option<Mode> {
        match s {
            "free" => Some(Mode::Free),
            "empty" => Some(Mode::Empty),
            "erase" => Some(Mode::Erase),
            _ => None,
        }
    }
    fn empty_arg(self) -> &'static str {
        match self {
            Mode::Free => "refuse",
            Mode::Empty => "allow",
            Mode::Erase => "force",
        }
    }
}

/// One row of `systemd-repart --json` output.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Row {
    #[serde(rename = "type")]
    pub ptype: String,
    pub label: Option<String>,
    pub uuid: Option<String>,
    pub partno: Option<u64>,
    /// The definition file that made or matched this row; null for partitions repart leaves alone.
    pub file: Option<String>,
    pub node: Option<String>,
    pub offset: Option<u64>,
    pub old_size: Option<u64>,
    pub raw_size: Option<u64>,
    pub activity: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Plan {
    pub version: u32,
    pub mode: Mode,
    pub disk: Disk,
    pub encrypt: bool,
    pub keymap: String,
    /// The podman image bootc installs from.
    pub image: String,
    /// Where the installed system takes its updates from.
    pub target_imgref: String,
    pub empty_arg: String,
    pub rows: Vec<Row>,
    /// Partitions that must be exactly the same after the install.
    pub kept: Vec<Partition>,
    /// The existing EFI partition ArkaOS shares (free mode).
    pub esp_node: Option<String>,
    /// The name bootupd gives the firmware boot entry (from the image's /etc/system-release).
    pub entry_label: String,
    /// Firmware entries with that name, which bootupd deletes before adding its own.
    pub replaces_entries: Vec<(String, String)>,
    pub warnings: Vec<String>,
    pub review: Vec<String>,
}

pub struct Request {
    pub disk: String,
    pub mode: Mode,
    pub encrypt: bool,
    pub keymap: String,
    pub image: String,
    pub target_imgref: String,
}

/// A refusal: the install is not safe or not possible as asked. The message is for the person.
pub struct Refusal(pub String);

/// Sizes in the binary units Windows' Disk Management shows (it labels GiB as "GB"), so the
/// numbers match what the person sees there.
pub fn gb(bytes: u64) -> String {
    if bytes >= GIB {
        format!("{:.1} GB", bytes as f64 / GIB as f64)
    } else {
        format!("{} MB", bytes / MIB)
    }
}

/// Whether a disk allows a mode, and why not. Pure: used by both the plan and the screens.
pub fn check_mode(d: &Disk, mode: Mode) -> Result<(), String> {
    if d.in_use {
        return Err("This disk holds the system that is running now.".into());
    }
    if d.size_bytes < MIN_FREE {
        return Err(format!("This disk is {}; ArkaOS needs at least {}.", gb(d.size_bytes), gb(MIN_FREE)));
    }
    match mode {
        Mode::Erase => Ok(()),
        Mode::Empty => {
            if d.partitions.is_empty() {
                Ok(())
            } else {
                Err("This disk has partitions on it.".into())
            }
        }
        Mode::Free => {
            if d.table.as_deref() != Some("gpt") {
                return Err("ArkaOS installs next to another system only on disks with a GPT partition table.".into());
            }
            let esp = d.esp.as_ref().ok_or("This disk has no EFI system partition to share.")?;
            if esp.free_bytes < MIN_ESP_FREE {
                return Err(format!(
                    "The EFI system partition has {} free; ArkaOS needs {}.",
                    gb(esp.free_bytes),
                    gb(MIN_ESP_FREE)
                ));
            }
            let largest = d.largest_gap().map(|g| g.size).unwrap_or(0);
            if largest < MIN_FREE {
                return Err(format!(
                    "The largest free area is {}; ArkaOS needs {}. Shrink a Windows drive in Disk Management to make room.",
                    gb(largest),
                    gb(MIN_FREE)
                ));
            }
            Ok(())
        }
    }
}

/// Checks repart's dry run: ArkaOS partitions are created, nothing else changes, and nothing
/// new overlaps a partition that stays. Pure, so it is unit-tested against recorded output.
pub fn validate(mode: Mode, rows: &[Row], kept: &[Partition], sector: u64) -> Result<(), String> {
    let ours: Vec<&Row> = rows.iter().filter(|r| r.file.is_some()).collect();
    if ours.is_empty() {
        return Err("repart planned no ArkaOS partitions.".into());
    }
    for r in &ours {
        if r.activity != "create" {
            return Err(format!(
                "An existing partition ({}) has the same type as an ArkaOS partition, so repart would reuse it instead of creating a new one. This usually means another Linux system is on the disk.",
                r.node.clone().unwrap_or_default()
            ));
        }
    }
    match mode {
        Mode::Free => {
            for r in rows.iter().filter(|r| r.file.is_none()) {
                if r.activity != "unchanged" {
                    return Err(format!("repart would change {} ({}).", r.node.clone().unwrap_or_default(), r.activity));
                }
            }
            for r in &ours {
                let (a, len) = (r.offset.unwrap_or(0), r.raw_size.unwrap_or(0));
                for k in kept {
                    let (ks, ke) = (k.start * sector, (k.start + k.size) * sector);
                    if a < ke && ks < a + len {
                        return Err(format!("The planned {} would overlap {}.", r.label.clone().unwrap_or_default(), k.node));
                    }
                }
            }
        }
        Mode::Empty | Mode::Erase => {
            if rows.iter().any(|r| r.file.is_none()) {
                return Err("repart kept a partition on a disk that should be empty.".into());
            }
        }
    }
    Ok(())
}

fn write_definitions(mode: Mode, encrypt: bool, root_min: u64) -> Result<(), String> {
    let _ = std::fs::remove_dir_all(DEFS_DIR);
    std::fs::create_dir_all(DEFS_DIR).map_err(|e| e.to_string())?;
    let mut files: Vec<(&str, String)> = Vec::new();
    if mode != Mode::Free {
        files.push((
            "00-esp.conf",
            format!("[Partition]\nType=esp\nLabel=EFI System Partition\nFormat=vfat\nSizeMinBytes={ESP_SIZE}\nSizeMaxBytes={ESP_SIZE}\n"),
        ));
    }
    files.push((
        "10-boot.conf",
        format!("[Partition]\nType=linux-generic\nLabel=arka-boot\nFormat=xfs\nSizeMinBytes={BOOT_SIZE}\nSizeMaxBytes={BOOT_SIZE}\n"),
    ));
    files.push((
        "20-root.conf",
        format!(
            "[Partition]\nType=root\nLabel=arka-root\nFormat=xfs\nEncrypt={}\nSizeMinBytes={root_min}\n",
            if encrypt { "key-file" } else { "off" }
        ),
    ));
    for (name, body) in files {
        std::fs::write(format!("{DEFS_DIR}/{name}"), body).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Runs repart's dry run and returns its rows. Shared by `plan` and by `apply`'s re-check.
pub fn dry_run(disk: &str, empty_arg: &str) -> Result<Vec<Row>, String> {
    let defs = format!("--definitions={DEFS_DIR}");
    let empty = format!("--empty={empty_arg}");
    let o = run::capture("systemd-repart", &["--dry-run=yes", "--seed=random", &defs, &empty, "--json=short", "--no-pager", disk])?;
    let rows: Vec<Row> = serde_json::from_str(o.stdout.trim()).map_err(|_| {
        format!("systemd-repart could not plan this disk: {}", o.stderr.trim())
    })?;
    Ok(rows)
}

/// The name bootupd uses for the firmware entry: /etc/system-release without " release ...".
/// For an `oci:DIR` image (how the live ISO carries it, with the registry's layers as they
/// are) the ISO build saves the image's /etc/system-release as DIR.system-release, because
/// reading one file out of the layers would mean unpacking all of them.
pub fn entry_label(image: &str) -> Result<String, String> {
    let s = match image.strip_prefix("oci:") {
        Some(dir) => {
            let f = format!("{}.system-release", dir.trim_end_matches('/'));
            std::fs::read_to_string(&f).map_err(|e| format!("could not read {f}: {e}"))?
        }
        None => run::stdout("podman", &["run", "--rm", "--net=none", "--entrypoint", "cat", image, "/etc/system-release"])?,
    };
    let s = s.trim();
    Ok(s.split(" release").next().unwrap_or(s).trim().to_string())
}

pub fn make(req: &Request) -> Result<Plan, Refusal> {
    let r = |e: String| Refusal(e);
    // The keyboard layout ends up on the kernel command line: allow layout names only.
    if req.keymap.is_empty() || !req.keymap.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c)) {
        return Err(Refusal(format!("\"{}\" is not a keyboard layout name.", req.keymap)));
    }
    if !std::path::Path::new("/sys/firmware/efi").exists() {
        return Err(Refusal("This computer was started in legacy BIOS mode. ArkaOS installs only on computers started in UEFI mode.".into()));
    }
    let disks = disk::probe().map_err(r)?;
    let d = disks
        .into_iter()
        .find(|d| d.path == req.disk)
        .ok_or_else(|| Refusal(format!("{} is not a disk ArkaOS can install to.", req.disk)))?;
    check_mode(&d, req.mode).map_err(r)?;

    let root_min = match req.mode {
        // Fill the largest free area: only that area fits a root partition this big.
        Mode::Free => d.largest_gap().map(|g| g.size).unwrap_or(0).saturating_sub(BOOT_SIZE + 64 * MIB).max(20 * GIB),
        _ => 20 * GIB,
    };
    write_definitions(req.mode, req.encrypt, root_min).map_err(r)?;
    let empty_arg = req.mode.empty_arg();
    let rows = dry_run(&d.path, empty_arg).map_err(r)?;
    let kept: Vec<Partition> = match req.mode {
        Mode::Free => d.partitions.iter().map(|p| p.part.clone()).collect(),
        _ => Vec::new(),
    };
    let sector = 512;
    validate(req.mode, &rows, &kept, sector).map_err(r)?;

    let entry_label = entry_label(&req.image).map_err(r)?;
    let entries = run::stdout("efibootmgr", &[]).map(|o| disk::parse_efibootmgr(&o)).unwrap_or_default();
    let replaces: Vec<(String, String)> = entries.into_iter().filter(|(_, l)| *l == entry_label).collect();
    if !replaces.is_empty() && entry_label != "ArkaOS" {
        return Err(Refusal(format!(
            "Installing would remove the firmware boot entry \"{}\" (Boot{}), which may belong to another system. This ArkaOS image still names its entry \"{}\".",
            entry_label, replaces[0].0, entry_label
        )));
    }

    let mut warnings = Vec::new();
    if req.mode == Mode::Erase && d.has_windows {
        warnings.push("Windows is on this disk. Erasing deletes Windows and all of its files.".into());
    } else if req.mode == Mode::Erase && !d.partitions.is_empty() {
        warnings.push("Everything on this disk is deleted.".into());
    }
    if !d.bitlocker.is_empty() && req.mode == Mode::Free {
        warnings.push("Windows uses BitLocker on this disk. Save the BitLocker recovery key before installing: Windows may ask for it once after ArkaOS is installed.".into());
    }
    if !d.other_linux.is_empty() && req.mode == Mode::Free {
        warnings.push("Another Linux system is on this disk. It is left as it is.".into());
    }
    if !replaces.is_empty() {
        warnings.push("The firmware boot entry of an earlier ArkaOS install will be replaced.".into());
    }
    if !req.encrypt {
        warnings.push("Encryption is off: anyone with this computer or its disk can read your files.".into());
    }

    let name = d.model.clone().filter(|m| !m.is_empty()).unwrap_or_else(|| "Disk".into());
    let mut review = vec![format!("{} ({}, {})", name, gb(d.size_bytes), d.path)];
    match req.mode {
        Mode::Free => {
            for p in &d.partitions {
                let what = match p.part.ptype.to_ascii_uppercase().as_str() {
                    disk::ESP => "EFI system partition, shared: ArkaOS adds about 8 MB",
                    disk::MS_RESERVED => "Microsoft reserved",
                    disk::MS_RECOVERY => "Windows recovery",
                    disk::MS_BASIC_DATA if p.fs.as_deref() == Some("BitLocker") => "Windows data, BitLocker",
                    disk::MS_BASIC_DATA => "Windows data",
                    _ => "other partition",
                };
                review.push(format!("Keeps {} ({}, {}): not changed", p.part.node, gb(p.part.size * sector), what));
            }
        }
        Mode::Empty => review.push("The disk is empty.".into()),
        Mode::Erase => review.push(format!(
            "Erases everything on this disk: {} partition(s), {} in total.",
            d.partitions.len(),
            gb(d.partitions.iter().map(|p| p.part.size * sector).sum())
        )),
    }
    for row in rows.iter().filter(|r| r.file.is_some()) {
        let what = match row.label.as_deref() {
            Some("arka-boot") => "ArkaOS boot".to_string(),
            Some("arka-root") if req.encrypt => "ArkaOS system and files, encrypted".to_string(),
            Some("arka-root") => "ArkaOS system and files, not encrypted".to_string(),
            Some(l) => l.to_string(),
            None => "partition".to_string(),
        };
        review.push(format!("Creates {} ({}, {})", row.node.clone().unwrap_or_default(), gb(row.raw_size.unwrap_or(0)), what));
    }

    Ok(Plan {
        version: 1,
        mode: req.mode,
        esp_node: d.esp.as_ref().map(|e| e.node.clone()).filter(|_| req.mode == Mode::Free),
        disk: d,
        encrypt: req.encrypt,
        keymap: req.keymap.clone(),
        image: req.image.clone(),
        target_imgref: req.target_imgref.clone(),
        empty_arg: empty_arg.into(),
        rows,
        kept,
        entry_label,
        replaces_entries: replaces,
        warnings,
        review,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(json: &str) -> Vec<Row> {
        serde_json::from_str(json).unwrap()
    }
    fn kept() -> Vec<Partition> {
        disk::parse_sfdisk(include_str!("../tests/fixtures/sfdisk-windows-free.json")).unwrap().partitions
    }
    const SPIKE: &str = include_str!("../tests/fixtures/repart-windows-free.json");

    #[test]
    fn spike_plan_is_valid() {
        assert!(validate(Mode::Free, &rows(SPIKE), &kept(), 512).is_ok());
    }

    #[test]
    fn reused_partition_is_refused() {
        let mut r = rows(SPIKE);
        r[1].activity = "unchanged".into(); // another Linux root matched our definition
        let e = validate(Mode::Free, &r, &kept(), 512).unwrap_err();
        assert!(e.contains("another Linux"), "{e}");
    }

    #[test]
    fn changed_windows_partition_is_refused() {
        let mut r = rows(SPIKE);
        r[4].activity = "resize".into();
        assert!(validate(Mode::Free, &r, &kept(), 512).is_err());
    }

    #[test]
    fn overlap_is_refused() {
        let mut r = rows(SPIKE);
        r[0].offset = Some(567296 * 512); // on top of the Windows data partition
        let e = validate(Mode::Free, &r, &kept(), 512).unwrap_err();
        assert!(e.contains("overlap"), "{e}");
    }

    #[test]
    fn erase_must_keep_nothing() {
        assert!(validate(Mode::Erase, &rows(SPIKE), &[], 512).is_err());
        let ours: Vec<Row> = rows(SPIKE).into_iter().filter(|r| r.file.is_some()).collect();
        assert!(validate(Mode::Erase, &ours, &[], 512).is_ok());
    }

    #[test]
    fn modes_follow_the_disk() {
        let mut d: Disk = serde_json::from_str(include_str!("../tests/fixtures/disk-windows-free.json")).unwrap();
        // The fixture's free area is 34.7 GiB: below the minimum.
        assert!(check_mode(&d, Mode::Free).unwrap_err().contains("Disk Management"));
        d.gaps[0].size = 50 * GIB;
        assert!(check_mode(&d, Mode::Free).is_ok());
        assert!(check_mode(&d, Mode::Empty).is_err());
        assert!(check_mode(&d, Mode::Erase).is_ok());
        d.esp.as_mut().unwrap().free_bytes = 5 * MIB;
        assert!(check_mode(&d, Mode::Free).unwrap_err().contains("EFI system partition"));
        d.esp.as_mut().unwrap().free_bytes = 200 * MIB;
        d.gaps.clear();
        assert!(check_mode(&d, Mode::Free).unwrap_err().contains("Disk Management"));
        d.in_use = true;
        assert!(check_mode(&d, Mode::Erase).is_err());
    }
}
