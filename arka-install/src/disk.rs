//! Reading the disks: partition tables, free space, filesystems, Windows and BitLocker.
//! Nothing in this file writes to a disk. EFI partitions are mounted read-only to look inside.

use crate::run;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub const ESP: &str = "C12A7328-F81F-11D2-BA4B-00A0C93EC93B";
pub const MS_BASIC_DATA: &str = "EBD0A0A2-B9E5-4433-87C0-68B6B72699C7";
pub const MS_RESERVED: &str = "E3C9E316-0B5C-4DB8-817D-F92DF00215AE";
pub const MS_RECOVERY: &str = "DE94BBA4-06D1-4D40-A16A-BFD50179D6AC";
/// Partition types that mean another Linux system lives on the disk.
pub const LINUX_TYPES: &[&str] = &[
    "0FC63DAF-8483-4772-8E79-3D69D8477DE4", // Linux filesystem
    "4F68BCE3-E8CD-4DB1-96E7-FBCAF984B709", // root (x86-64)
    "933AC7E1-2EB4-4F13-B844-0E14E2AEF915", // /home
    "0657FD6D-A4AB-43C4-84E5-0933C84B4F4F", // swap
    "BC13C2FF-59E6-4262-A352-B275FD6F7172", // XBOOTLDR (/boot)
    "E6D6D379-F507-44C2-A23C-238F2A3DF928", // LVM
    "CA7D7CCB-63ED-4C53-861C-1742536059CC", // LUKS
];
const LINUX_FS: &[&str] = &["ext2", "ext3", "ext4", "xfs", "btrfs", "crypto_LUKS", "LVM2_member", "swap", "f2fs"];

/// Free-space areas are aligned to 1 MiB, like every partitioning tool does.
pub const ALIGN_BYTES: u64 = 1 << 20;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Partition {
    pub node: String,
    pub start: u64,
    pub size: u64,
    #[serde(rename = "type")]
    pub ptype: String,
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Table {
    pub label: String,
    #[serde(default)]
    pub id: Option<String>,
    pub device: String,
    #[serde(default)]
    pub firstlba: Option<u64>,
    #[serde(default)]
    pub lastlba: Option<u64>,
    #[serde(default)]
    pub sectorsize: Option<u64>,
    #[serde(default)]
    pub partitions: Vec<Partition>,
}

#[derive(Deserialize)]
struct SfdiskDoc {
    partitiontable: Table,
}

/// Parses `sfdisk --json` output.
pub fn parse_sfdisk(json: &str) -> Result<Table, String> {
    serde_json::from_str::<SfdiskDoc>(json)
        .map(|d| d.partitiontable)
        .map_err(|e| format!("could not read the partition table: {e}"))
}

impl Table {
    pub fn sector(&self) -> u64 {
        self.sectorsize.unwrap_or(512)
    }
}

/// A free area on a disk, in bytes from the start of the disk.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Gap {
    pub offset: u64,
    pub size: u64,
}

/// Free areas between and after the partitions of a GPT table, aligned to 1 MiB.
pub fn gaps(t: &Table) -> Vec<Gap> {
    let ss = t.sector();
    let first = t.firstlba.unwrap_or(34) * ss;
    let end = (t.lastlba.unwrap_or(0) + 1) * ss;
    let mut parts: Vec<(u64, u64)> = t.partitions.iter().map(|p| (p.start * ss, (p.start + p.size) * ss)).collect();
    parts.sort();
    let mut out = Vec::new();
    let mut cursor = first;
    let push = |a: u64, b: u64, out: &mut Vec<Gap>| {
        let a = a.div_ceil(ALIGN_BYTES) * ALIGN_BYTES;
        let b = b / ALIGN_BYTES * ALIGN_BYTES;
        if b > a {
            out.push(Gap { offset: a, size: b - a });
        }
    };
    for (s, e) in parts {
        if s > cursor {
            push(cursor, s, &mut out);
        }
        cursor = cursor.max(e);
    }
    if end > cursor {
        push(cursor, end, &mut out);
    }
    out
}

/// Reads `blkid -p -o export` output into key/value pairs (TYPE, LABEL, UUID, ...).
pub fn parse_blkid(export: &str) -> BTreeMap<String, String> {
    export
        .lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PartInfo {
    #[serde(flatten)]
    pub part: Partition,
    /// Filesystem found on the partition by blkid ("vfat", "ntfs", "BitLocker", ...), if any.
    pub fs: Option<String>,
    pub fs_label: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct EspInfo {
    pub node: String,
    pub has_windows_boot: bool,
    pub free_bytes: u64,
    pub size_bytes: u64,
    /// sha256 of every file under EFI/Microsoft, to prove afterwards that Windows' boot files are unchanged.
    pub microsoft_files: BTreeMap<String, String>,
    pub other_systems: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Disk {
    pub path: String,
    pub model: Option<String>,
    pub serial: Option<String>,
    pub size_bytes: u64,
    pub transport: Option<String>,
    pub removable: bool,
    /// "gpt", "dos", or None for a disk without a partition table.
    pub table: Option<String>,
    pub partitions: Vec<PartInfo>,
    pub gaps: Vec<Gap>,
    pub esp: Option<EspInfo>,
    pub has_windows: bool,
    pub bitlocker: Vec<String>,
    pub other_linux: Vec<String>,
    /// Holds the system that is running now (the live medium or this computer's own system).
    pub in_use: bool,
    /// sha256 of `sfdisk --json` output: the plan is refused if this changes before install.
    pub fingerprint: Option<String>,
}

impl Disk {
    pub fn largest_gap(&self) -> Option<&Gap> {
        self.gaps.iter().max_by_key(|g| g.size)
    }
}

#[derive(Deserialize)]
struct LsblkDoc {
    blockdevices: Vec<LsblkDev>,
}
#[derive(Deserialize)]
struct LsblkDev {
    path: String,
    #[serde(rename = "type")]
    dtype: String,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default)]
    ro: Option<bool>,
    #[serde(default)]
    rm: Option<bool>,
    #[serde(default)]
    tran: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    serial: Option<String>,
    #[serde(default)]
    children: Option<Vec<LsblkDev>>,
}

pub fn sha256(data: &[u8]) -> String {
    // Small, dependency-free: hand the bytes to sha256sum.
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut child = match Command::new("sha256sum").stdin(Stdio::piped()).stdout(Stdio::piped()).spawn() {
        Ok(c) => c,
        Err(_) => return String::new(),
    };
    if let Some(mut si) = child.stdin.take() {
        let _ = si.write_all(data);
    }
    child
        .wait_with_output()
        .map(|o| String::from_utf8_lossy(&o.stdout).split_whitespace().next().unwrap_or("").to_string())
        .unwrap_or_default()
}

/// The fingerprint of a disk's partition table, or None when it has no table.
pub fn fingerprint(disk: &str) -> Option<String> {
    let o = run::capture("sfdisk", &["--json", disk]).ok()?;
    o.ok.then(|| sha256(o.stdout.as_bytes()))
}

/// Disks that hold the running system: they are never offered as a target.
fn disks_in_use() -> Vec<String> {
    let mut sources = Vec::new();
    for m in ["/", "/sysroot", "/boot", "/boot/efi", "/run/initramfs/live", "/var"] {
        if let Ok(s) = run::stdout("findmnt", &["-no", "SOURCE", m]) {
            for line in s.lines() {
                let src = line.trim().split('[').next().unwrap_or("").to_string();
                if src.starts_with("/dev/") {
                    sources.push(src);
                }
            }
        }
    }
    let mut disks = Vec::new();
    for src in sources {
        // -s lists the device and everything under it (partition, then disk; through LUKS too).
        if let Ok(out) = run::stdout("lsblk", &["-J", "-s", "-o", "PATH,TYPE", &src]) {
            if let Ok(doc) = serde_json::from_str::<LsblkDoc>(&out) {
                fn walk(d: &LsblkDev, acc: &mut Vec<String>) {
                    if d.dtype == "disk" {
                        acc.push(d.path.clone());
                    }
                    for c in d.children.iter().flatten() {
                        walk(c, acc);
                    }
                }
                for d in &doc.blockdevices {
                    walk(d, &mut disks);
                }
            }
        }
    }
    disks.sort();
    disks.dedup();
    disks
}

/// Looks inside an EFI partition (mounted read-only) for Windows and other systems.
fn inspect_esp(node: &str) -> Result<EspInfo, String> {
    // Already mounted somewhere (a second mount of the same FAT volume is refused): read it there.
    let existing = run::stdout("findmnt", &["-no", "TARGET", "--first-only", "--source", node])
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let mounted_here = existing.is_none();
    let dir = existing.unwrap_or_else(|| format!("/run/arka-install/probe-{}", node.trim_start_matches("/dev/").replace('/', "-")));
    if mounted_here {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        run::stdout("mount", &["-o", "ro,nodev,nosuid,noexec", "-t", "vfat", node, &dir])?;
    }
    let result = (|| {
        let mut info = EspInfo { node: node.to_string(), ..Default::default() };
        let efi = Path::new(&dir).join("EFI");
        // FAT is case-insensitive but Linux shows the stored case: match directory names ignoring case.
        if let Ok(rd) = std::fs::read_dir(&efi) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.eq_ignore_ascii_case("Microsoft") {
                    collect_hashes(&e.path(), &dir, &mut info.microsoft_files);
                    info.has_windows_boot = info
                        .microsoft_files
                        .keys()
                        .any(|k| k.to_ascii_lowercase().ends_with("/boot/bootmgfw.efi"));
                } else if !name.eq_ignore_ascii_case("BOOT") {
                    info.other_systems.push(name);
                }
            }
        }
        let df = run::stdout("df", &["-B1", "--output=size,avail", &dir])?;
        let nums: Vec<u64> = df.lines().nth(1).unwrap_or("").split_whitespace().filter_map(|n| n.parse().ok()).collect();
        info.size_bytes = *nums.first().unwrap_or(&0);
        info.free_bytes = *nums.get(1).unwrap_or(&0);
        Ok(info)
    })();
    if mounted_here {
        let _ = run::capture("umount", &[&dir]);
        let _ = std::fs::remove_dir(&dir);
    }
    result
}

fn collect_hashes(path: &Path, root: &str, out: &mut BTreeMap<String, String>) {
    if path.is_dir() {
        if let Ok(rd) = std::fs::read_dir(path) {
            for e in rd.flatten() {
                collect_hashes(&e.path(), root, out);
            }
        }
    } else if let Ok(data) = std::fs::read(path) {
        let rel = path.strip_prefix(root).map(|p| p.display().to_string()).unwrap_or_default();
        out.insert(rel, sha256(&data));
    }
}

/// Reads every disk ArkaOS could be installed on.
pub fn probe() -> Result<Vec<Disk>, String> {
    std::fs::create_dir_all("/run/arka-install").map_err(|e| e.to_string())?;
    let in_use = disks_in_use();
    let out = run::stdout("lsblk", &["-J", "-b", "-d", "-o", "PATH,TYPE,SIZE,RO,RM,TRAN,MODEL,SERIAL"])?;
    let doc: LsblkDoc = serde_json::from_str(&out).map_err(|e| format!("could not read the disk list: {e}"))?;
    let mut disks = Vec::new();
    for d in doc.blockdevices {
        let name = d.path.trim_start_matches("/dev/");
        if d.dtype != "disk" || d.ro.unwrap_or(false) || d.size.unwrap_or(0) == 0 {
            continue;
        }
        if name.starts_with("zram") || name.starts_with("loop") || name.starts_with("sr") {
            continue;
        }
        disks.push(read_disk(
            &d.path,
            d.model.map(|s| s.trim().to_string()),
            d.serial,
            d.size.unwrap_or(0),
            d.tran,
            d.rm.unwrap_or(false),
            in_use.contains(&d.path),
        ));
    }
    Ok(disks)
}

fn read_disk(path: &str, model: Option<String>, serial: Option<String>, size: u64, tran: Option<String>, rm: bool, in_use: bool) -> Disk {
    let sf = run::capture("sfdisk", &["--json", path]).ok().filter(|o| o.ok);
    let table = sf.as_ref().and_then(|o| parse_sfdisk(&o.stdout).ok());
    let fingerprint = sf.as_ref().map(|o| sha256(o.stdout.as_bytes()));
    let mut partitions = Vec::new();
    let (mut has_windows, mut bitlocker, mut other_linux, mut esp) = (false, Vec::new(), Vec::new(), None);
    if let Some(t) = &table {
        for p in &t.partitions {
            let b = run::capture("blkid", &["-p", "-o", "export", &p.node])
                .ok()
                .filter(|o| o.ok)
                .map(|o| parse_blkid(&o.stdout))
                .unwrap_or_default();
            let fs = b.get("TYPE").cloned();
            let ptype = p.ptype.to_ascii_uppercase();
            if fs.as_deref() == Some("BitLocker") {
                bitlocker.push(p.node.clone());
                has_windows = true;
            }
            if ptype == MS_BASIC_DATA && fs.as_deref() == Some("ntfs") {
                has_windows = true;
            }
            if LINUX_TYPES.contains(&ptype.as_str()) || fs.as_deref().map(|f| LINUX_FS.contains(&f)).unwrap_or(false) {
                other_linux.push(p.node.clone());
            }
            if ptype == ESP && esp.is_none() && fs.as_deref() == Some("vfat") {
                match inspect_esp(&p.node) {
                    Ok(info) => {
                        has_windows |= info.has_windows_boot;
                        esp = Some(info);
                    }
                    Err(e) => run::log(&format!("could not look inside {}: {e}", p.node)),
                }
            }
            partitions.push(PartInfo { part: p.clone(), fs, fs_label: b.get("LABEL").cloned() });
        }
    }
    let gaps = match &table {
        Some(t) if t.label == "gpt" => gaps(t),
        _ => Vec::new(),
    };
    Disk {
        path: path.to_string(),
        model,
        serial,
        size_bytes: size,
        transport: tran,
        removable: rm,
        table: table.as_ref().map(|t| t.label.clone()),
        partitions,
        gaps,
        esp,
        has_windows,
        bitlocker,
        other_linux,
        in_use,
        fingerprint,
    }
}

/// Firmware boot entries, as (number, label) pairs, from `efibootmgr` output.
pub fn parse_efibootmgr(out: &str) -> Vec<(String, String)> {
    out.lines()
        .filter_map(|l| {
            let rest = l.strip_prefix("Boot")?;
            let num = rest.get(0..4)?;
            if !num.chars().all(|c| c.is_ascii_hexdigit()) {
                return None;
            }
            let label = rest.get(5..)?.split('\t').next()?.trim();
            Some((num.to_string(), label.to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIN_FREE: &str = include_str!("../tests/fixtures/sfdisk-windows-free.json");

    #[test]
    fn gaps_between_windows_partitions() {
        let t = parse_sfdisk(WIN_FREE).unwrap();
        let g = gaps(&t);
        // The 34.7 GB area between the Windows data partition and the recovery partition.
        let big = g.iter().max_by_key(|g| g.size).unwrap();
        assert_eq!(big.offset, 50898944 * 512);
        assert_eq!(big.size, (123729920 - 50898944) * 512);
        for x in &g {
            assert_eq!(x.offset % ALIGN_BYTES, 0);
            assert_eq!(x.size % ALIGN_BYTES, 0);
        }
    }

    #[test]
    fn empty_gpt_is_one_gap() {
        let t = parse_sfdisk(
            r#"{"partitiontable":{"label":"gpt","device":"/dev/x","unit":"sectors","firstlba":2048,"lastlba":125829086,"sectorsize":512,"partitions":[]}}"#,
        )
        .unwrap();
        let g = gaps(&t);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].offset, 2048 * 512);
    }

    #[test]
    fn blkid_bitlocker() {
        let b = parse_blkid("DEVNAME=/dev/vdb3\nVERSION=2\nTYPE=BitLocker\nUSAGE=crypto\n");
        assert_eq!(b.get("TYPE").map(String::as_str), Some("BitLocker"));
    }

    #[test]
    fn efibootmgr_labels() {
        let out = "BootCurrent: 0004\nTimeout: 0 seconds\nBootOrder: 0004,0002\nBoot0002* UEFI Misc Device\tPciRoot(0x0)\nBoot0004* Fedora\tHD(1,GPT,e7a7929d)/\\EFI\\fedora\\shimx64.efi\nBoot0005  Windows Boot Manager\tHD(1)\n";
        let e = parse_efibootmgr(out);
        assert_eq!(
            e,
            vec![
                ("0002".into(), "UEFI Misc Device".into()),
                ("0004".into(), "Fedora".into()),
                ("0005".into(), "Windows Boot Manager".into())
            ]
        );
    }
}
