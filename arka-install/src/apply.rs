//! Carrying out a reviewed plan. This is the only part that writes to a disk, and it writes only
//! after re-checking that the disk is exactly as it was when the plan was reviewed.

use crate::disk::{self, Partition};
use crate::plan::{self, Mode, Plan, Row, DEFS_DIR, PLAN_DIR};
use crate::run::{self, log};
use serde::Serialize;
use std::io::{BufRead, Write};
use std::os::unix::fs::OpenOptionsExt;

const KEY: &str = "/run/arka-install/key";
const TARGET: &str = "/run/arka-install/target";
const BOOT_RW: &str = "/run/arka-install/boot";
const LUKS_NAME: &str = "arka-install-root";

/// Prints a progress line for the screens: {"event":"step","step":"..."}.
fn step(name: &str) {
    println!("{}", serde_json::json!({"event": "step", "step": name}));
    let _ = std::io::stdout().flush();
    log(&format!("== {name}"));
}

/// Undoes mounts, closes the encrypted volume and removes the key file, whatever happens.
struct Cleanup {
    mounted_target: bool,
    mounted_boot: bool,
    opened_luks: bool,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        if self.mounted_boot {
            let _ = run::capture("umount", &[BOOT_RW]);
        }
        if self.mounted_target {
            let _ = run::capture("umount", &["-R", TARGET]);
        }
        if self.opened_luks {
            let _ = run::capture("cryptsetup", &["close", LUKS_NAME]);
        }
        let _ = std::fs::remove_file(KEY);
    }
}

#[derive(Serialize)]
pub struct Outcome {
    pub kept_unchanged: bool,
    pub windows_boot_files_unchanged: Option<bool>,
    pub created: Vec<String>,
    pub boot_entry: Option<String>,
    pub log: String,
}

fn read_passphrase() -> Result<String, String> {
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line).map_err(|e| format!("could not read the passphrase: {e}"))?;
    let p = line.trim_end_matches(['\n', '\r']).to_string();
    if p.is_empty() {
        return Err("The passphrase is empty.".into());
    }
    Ok(p)
}

fn write_key(pass: &str) -> Result<(), String> {
    let mut f = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(KEY)
        .map_err(|e| format!("could not write the key file: {e}"))?;
    f.write_all(pass.as_bytes()).map_err(|e| e.to_string())
}

/// Rows that matter for "same plan": ArkaOS's partitions and where they go.
fn ours(rows: &[Row]) -> Vec<(Option<String>, Option<u64>, Option<u64>, String)> {
    rows.iter()
        .filter(|r| r.file.is_some())
        .map(|r| (r.label.clone(), r.offset, r.raw_size, r.activity.clone()))
        .collect()
}

fn node_of(rows: &[Row], label: &str) -> Result<String, String> {
    rows.iter()
        .find(|r| r.file.is_some() && r.label.as_deref() == Some(label))
        .and_then(|r| r.uuid.clone())
        .map(|u| format!("/dev/disk/by-partuuid/{}", u.to_ascii_lowercase()))
        .ok_or_else(|| format!("repart made no {label} partition"))
}

/// Compares the partitions that must stay with the disk as it is now.
pub fn kept_unchanged(kept: &[Partition], now: &[Partition]) -> Result<(), String> {
    for k in kept {
        let same = now.iter().any(|n| {
            n.node == k.node
                && n.start == k.start
                && n.size == k.size
                && n.ptype.eq_ignore_ascii_case(&k.ptype)
                && n.uuid.as_deref().map(str::to_ascii_lowercase) == k.uuid.as_deref().map(str::to_ascii_lowercase)
        });
        if !same {
            return Err(format!("{} is not the same as before the install", k.node));
        }
    }
    Ok(())
}

pub fn apply(p: &Plan) -> Result<Outcome, String> {
    if !run::is_root() {
        return Err("arka-install apply must run as root.".into());
    }
    std::fs::create_dir_all(PLAN_DIR).map_err(|e| e.to_string())?;
    let mut cleanup = Cleanup { mounted_target: false, mounted_boot: false, opened_luks: false };

    step("check");
    let now = disk::fingerprint(&p.disk.path);
    if p.mode != Mode::Empty || p.disk.fingerprint.is_some() {
        if now != p.disk.fingerprint {
            return Err("The disk changed after the plan was made. Nothing was written; plan again.".into());
        }
    }
    // The definitions live in /run: write them again from the plan's own settings and re-run the
    // dry run; the install goes ahead only if repart still plans exactly what was reviewed.
    if !std::path::Path::new(DEFS_DIR).is_dir() {
        return Err("The partition definitions are gone (was the computer restarted?). Plan again.".into());
    }
    let again = plan::dry_run(&p.disk.path, &p.empty_arg)?;
    if ours(&again) != ours(&p.rows) {
        return Err("systemd-repart now plans something different from what was reviewed. Nothing was written; plan again.".into());
    }

    if p.encrypt {
        let pass = read_passphrase()?;
        write_key(&pass)?;
    }

    step("partition");
    let defs = format!("--definitions={DEFS_DIR}");
    let empty = format!("--empty={}", p.empty_arg);
    // --seed=random: partition and volume IDs must differ between installs (by default repart
    // derives them from this system's machine ID, so two installs from one live session match).
    let mut args = vec!["--dry-run=no", "--seed=random", defs.as_str(), empty.as_str(), "--json=short", "--no-pager"];
    let keyarg = format!("--key-file={KEY}");
    if p.encrypt {
        args.push(&keyarg);
    }
    args.push(&p.disk.path);
    let out = run::stdout("systemd-repart", &args)?;
    let done: Vec<Row> = serde_json::from_str(out.trim()).map_err(|e| format!("could not read repart's result: {e}"))?;
    let _ = run::capture("udevadm", &["settle"]);

    let boot = node_of(&done, "arka-boot")?;
    let root = node_of(&done, "arka-root")?;
    let esp = match p.mode {
        Mode::Free => p.esp_node.clone().ok_or("the plan names no EFI partition")?,
        _ => node_of(&done, "EFI System Partition")?,
    };

    step("mount");
    // repart made the filesystem inside the encrypted volume; open it with the same key, then
    // delete the key file at once.
    let (root_dev, luks_uuid) = if p.encrypt {
        let uuid = run::stdout("cryptsetup", &["luksUUID", &root])?.trim().to_string();
        let opened = run::stdout("cryptsetup", &["open", "--key-file", KEY, &root, LUKS_NAME]);
        let _ = std::fs::remove_file(KEY);
        opened?;
        cleanup.opened_luks = true;
        (format!("/dev/mapper/{LUKS_NAME}"), Some(uuid))
    } else {
        (root.clone(), None)
    };
    mount_all(&root_dev, &boot, &esp, &mut cleanup)?;
    install_and_finish(p, &root_dev, &boot, &esp, luks_uuid.as_deref(), &mut cleanup)
}

fn mount_all(root_dev: &str, boot: &str, esp: &str, c: &mut Cleanup) -> Result<(), String> {
    std::fs::create_dir_all(TARGET).map_err(|e| e.to_string())?;
    run::stdout("mount", &[root_dev, TARGET])?;
    c.mounted_target = true;
    std::fs::create_dir_all(format!("{TARGET}/boot")).map_err(|e| e.to_string())?;
    run::stdout("mount", &[boot, &format!("{TARGET}/boot")])?;
    std::fs::create_dir_all(format!("{TARGET}/boot/efi")).map_err(|e| e.to_string())?;
    run::stdout("mount", &[esp, &format!("{TARGET}/boot/efi")])?;
    Ok(())
}

fn install_and_finish(p: &Plan, root_dev: &str, boot: &str, esp: &str, luks_uuid: Option<&str>, c: &mut Cleanup) -> Result<Outcome, String> {
    step("install");
    let vol = format!("{TARGET}:/target");
    let mut kargs: Vec<String> = Vec::new();
    if let Some(u) = luks_uuid {
        kargs.push(format!("rd.luks.uuid=luks-{u}"));
    }
    kargs.push(format!("rd.vconsole.keymap={}", p.keymap));
    kargs.push(format!("vconsole.keymap={}", p.keymap));
    let common = ["install", "to-filesystem", "--skip-fetch-check", "--bootloader", "grub", "--target-imgref", &p.target_imgref];
    let karg_args = kargs.iter().flat_map(|k| ["--karg".to_string(), k.clone()]);
    if p.image.starts_with("oci:") {
        // The live ISO: the image is an OCI directory on the ISO and this system is that
        // image, so bootc runs here and reads the layers from the directory.
        let mut args: Vec<String> = common.iter().map(|s| s.to_string()).collect();
        args.splice(2..2, ["--source-imgref".to_string(), p.image.clone()]);
        args.extend(karg_args);
        args.push(TARGET.into());
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        run::stream("bootc", &refs)?;
    } else {
        let mut args: Vec<String> = [
            "run", "--rm", "--privileged", "--pid=host", "--security-opt", "label=type:unconfined_t",
            "-v", "/dev:/dev", "-v", "/var/lib/containers:/var/lib/containers", "-v", &vol, &p.image, "bootc",
        ]
        .iter()
        .chain(common.iter())
        .map(|s| s.to_string())
        .collect();
        args.extend(karg_args);
        args.push("/target".into());
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        run::stream("podman", &refs)?;
    }

    step("verify");
    let now = run::stdout("sfdisk", &["--json", &p.disk.path]).and_then(|s| disk::parse_sfdisk(&s))?;
    let kept_ok = kept_unchanged(&p.kept, &now.partitions);
    let ms_ok = match (&p.mode, &p.disk.esp) {
        (Mode::Free, Some(e)) if !e.microsoft_files.is_empty() => {
            let mut after = std::collections::BTreeMap::new();
            hash_tree(std::path::Path::new(&format!("{TARGET}/boot/efi")), &format!("{TARGET}/boot/efi"), &mut after);
            let before: std::collections::BTreeMap<String, String> =
                e.microsoft_files.iter().map(|(k, v)| (k.to_ascii_lowercase(), v.clone())).collect();
            let after: std::collections::BTreeMap<String, String> = after
                .into_iter()
                .filter(|(k, _)| k.to_ascii_lowercase().starts_with("efi/microsoft/"))
                .map(|(k, v)| (k.to_ascii_lowercase(), v))
                .collect();
            Some(before == after)
        }
        _ => None,
    };
    match &kept_ok {
        Ok(()) => log("verify: every partition that was kept is unchanged"),
        Err(e) => log(&format!("verify FAILED: {e}")),
    }
    if let Some(ok) = ms_ok {
        log(&format!("verify: Windows boot files {}", if ok { "unchanged" } else { "CHANGED" }));
    }
    let entries = run::stdout("efibootmgr", &[]).map(|o| disk::parse_efibootmgr(&o)).unwrap_or_default();
    let order = run::stdout("efibootmgr", &[]).ok().and_then(|o| {
        o.lines().find(|l| l.starts_with("BootOrder:")).map(|l| l.trim_start_matches("BootOrder:").trim().to_string())
    });
    let boot_entry = entries.iter().find(|(_, l)| *l == p.entry_label).map(|(n, l)| format!("Boot{n} {l}"));
    log(&format!("firmware entry: {:?}, boot order: {:?}", boot_entry, order));

    run::stdout("umount", &["-R", TARGET])?;
    c.mounted_target = false;
    // bootc leaves the root filesystem read-only too: mount it again on its own to give the
    // installed system's /var a copy of the log.
    run::stdout("mount", &[root_dev, TARGET])?;
    c.mounted_target = true;
    let mut copied = false;
    if let Ok(deploys) = std::fs::read_dir(format!("{TARGET}/ostree/deploy")) {
        for d in deploys.flatten() {
            let dir = d.path().join("var/log");
            if std::fs::create_dir_all(&dir).is_ok() && std::fs::copy(run::LOG, dir.join("arka-install.log")).is_ok() {
                copied = true;
            }
        }
    }
    if !copied {
        log("could not copy the install log into the installed system");
    }
    run::stdout("umount", &[TARGET])?;
    c.mounted_target = false;
    if c.opened_luks {
        run::stdout("cryptsetup", &["close", LUKS_NAME])?;
        c.opened_luks = false;
    }

    step("boot-menu");
    // bootc leaves /boot read-only; mount it on its own to add the Windows entry.
    std::fs::create_dir_all(BOOT_RW).map_err(|e| e.to_string())?;
    run::stdout("mount", &[boot, BOOT_RW])?;
    c.mounted_boot = true;
    let custom = format!("{BOOT_RW}/grub2/custom.cfg");
    // Only next to Windows: on an empty or erased disk there is nothing for the entry to find.
    let windows_kept = p.mode == Mode::Free && p.disk.has_windows;
    if windows_kept && std::path::Path::new(&format!("{BOOT_RW}/grub2")).is_dir() && !std::path::Path::new(&custom).exists() {
        std::fs::write(&custom, WINDOWS_ENTRY).map_err(|e| format!("could not write the Windows menu entry: {e}"))?;
        log("added the Windows boot menu entry (shown only when Windows' boot manager is found)");
    }
    run::stdout("umount", &[BOOT_RW])?;
    c.mounted_boot = false;
    let _ = esp;

    kept_ok?;
    if ms_ok == Some(false) {
        return Err("Windows' boot files on the EFI partition changed during the install. See the install log.".into());
    }
    Ok(Outcome {
        kept_unchanged: true,
        windows_boot_files_unchanged: ms_ok,
        created: p.rows.iter().filter(|r| r.file.is_some()).filter_map(|r| r.label.clone()).collect(),
        boot_entry,
        log: run::LOG.into(),
    })
}

fn hash_tree(path: &std::path::Path, root: &str, out: &mut std::collections::BTreeMap<String, String>) {
    if path.is_dir() {
        if let Ok(rd) = std::fs::read_dir(path) {
            for e in rd.flatten() {
                hash_tree(&e.path(), root, out);
            }
        }
    } else if let Ok(data) = std::fs::read(path) {
        let rel = path.strip_prefix(root).map(|p| p.display().to_string()).unwrap_or_default();
        out.insert(rel, disk::sha256(&data));
    }
}

const WINDOWS_ENTRY: &str = "# Added by the ArkaOS installer: Windows, when its boot manager is on any disk.
insmod part_gpt
insmod fat
insmod chain
if search --no-floppy --file --set=windows /EFI/Microsoft/Boot/bootmgfw.efi; then
  menuentry 'Windows' --class windows --class os {
    set root=$windows
    chainloader /EFI/Microsoft/Boot/bootmgfw.efi
  }
fi
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kept_partitions_compare() {
        let before = disk::parse_sfdisk(include_str!("../tests/fixtures/sfdisk-windows-free.json")).unwrap().partitions;
        let mut after = before.clone();
        after.push(Partition { node: "/dev/vdb5".into(), start: 50898944, size: 4194304, ptype: "x".into(), uuid: None, name: None });
        assert!(kept_unchanged(&before, &after).is_ok());
        after[2].size -= 2048; // Windows data partition shrunk
        assert!(kept_unchanged(&before, &after).is_err());
    }
}
