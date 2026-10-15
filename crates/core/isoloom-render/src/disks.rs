//! The data disks step ([`Step::Disks`](isoloom_plan::Step::Disks)) as a root shell script for
//! Linux and as PowerShell for Windows. A platform names an attached disk its own way (NVMe ids
//! on AWS, LUNs on Azure, by-id links elsewhere) and may attach it after the machine boots, so
//! the step doesn't ask: it waits for a blank disk of the size asked for and formats it with the
//! disk's name as its label. Blank disks of one size are interchangeable, and once formatted a
//! disk is found by its label, so the step is idempotent, and its fstab line mounts it at every
//! boot.

use std::fmt::Write as _;

use isoloom_plan::DataDisk;

use crate::machine_setup::PKG;
use crate::shell::quote;

/// How long a disk may take to be attached: five-second tries.
const TRIES: u32 = 60;

/// The shell function that formats (when blank) and mounts one disk: `disk <label> <fs> <mount>
/// <bytes>`. A blank disk is a whole disk of that size (to the GiB) with no partition and no
/// signature.
const DISK_SH: &str = r#"disk() {
  label=$1; fs=$2; at=$3; bytes=$4
  mkdir -p "$at"
  if ! blkid -L "$label" >/dev/null 2>&1; then
    i=0; dev=
    while [ -z "$dev" ]; do
      for d in $(lsblk -dnbo NAME,SIZE,TYPE | awk -v b="$bytes" '$3 == "disk" && $2 >= b && $2 < b + 1073741824 { print $1 }'); do
        if [ "$(lsblk -n "/dev/$d" | wc -l)" -eq 1 ] && ! blkid -p "/dev/$d" >/dev/null 2>&1; then dev=/dev/$d; break; fi
      done
      if [ -z "$dev" ]; then
        i=$((i+1)); if [ "$i" -ge TRIES ]; then echo "disk $label: no blank disk of $bytes bytes is attached" >&2; exit 1; fi
        sleep 5
      fi
    done
    if [ "$fs" = xfs ]; then command -v mkfs.xfs >/dev/null || pkg xfsprogs; mkfs.xfs -q -L "$label" "$dev"; else mkfs.ext4 -q -L "$label" "$dev"; fi
    udevadm settle 2>/dev/null || true
  fi
  grep -q "^LABEL=$label " /etc/fstab || printf 'LABEL=%s %s %s defaults,nofail 0 2\n' "$label" "$at" "$fs" >> /etc/fstab
  mountpoint -q "$at" || mount "$at"
}
"#;

/// The step on Linux; `None` when every disk is raw (nothing to format or mount).
pub fn linux(disks: &[DataDisk]) -> Option<String> {
    let lines: Vec<String> = disks
        .iter()
        .filter_map(|d| {
            let at = d.mount.as_deref()?;
            Some(format!("disk {} {} {} {}", quote(&d.name), d.fs.id(), quote(at), d.bytes()))
        })
        .collect();
    if lines.is_empty() {
        return None;
    }
    let mut sh = format!("set -e\n{PKG}\n{}", DISK_SH.replace("TRIES", &TRIES.to_string()));
    for l in lines {
        let _ = writeln!(sh, "{l}");
    }
    Some(sh)
}

/// The step on Windows: each disk brought online, partitioned and formatted NTFS at its drive
/// letter, once (a volume with its label is there already after that).
pub fn windows(disks: &[DataDisk]) -> Option<String> {
    let lines: Vec<String> = disks
        .iter()
        .filter_map(|d| {
            let letter = d.mount.as_deref()?.trim_end_matches(':');
            let label = crate::ps(&d.name);
            let missing = crate::ps(&format!("disk {}: no blank disk of {} bytes is attached", d.name, d.bytes()));
            Some(format!(
                "if (-not (Get-Volume -FileSystemLabel {label} -ErrorAction SilentlyContinue)) {{ $i = 0; do {{ $d = Get-Disk | Where-Object {{ $_.PartitionStyle -eq 'RAW' -and $_.Size -ge {b} -and $_.Size -lt {b} + 1GB }} | Select-Object -First 1; if (-not $d) {{ if (++$i -ge {TRIES}) {{ throw {missing} }}; Start-Sleep 5 }} }} until ($d); if ($d.IsOffline) {{ Set-Disk -Number $d.Number -IsOffline $false }}; Initialize-Disk -Number $d.Number -PartitionStyle GPT -PassThru | New-Partition -UseMaximumSize -DriveLetter {letter} | Format-Volume -FileSystem NTFS -NewFileSystemLabel {label} -Confirm:$false | Out-Null }}",
                b = d.bytes(),
            ))
        })
        .collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use isoloom_spec::model::Filesystem;

    use super::*;

    fn disk(name: &str, fs: Filesystem, mount: Option<&str>) -> DataDisk {
        DataDisk {
            name: name.into(),
            key: format!("db-{name}"),
            size_gb: 2,
            fs,
            mount: mount.map(String::from),
        }
    }

    #[test]
    fn each_formatted_disk_is_found_by_size_then_by_label() {
        let sh = linux(&[disk("data", Filesystem::Xfs, Some("/data")), disk("blk", Filesystem::Raw, None)]).unwrap();
        assert!(sh.contains("disk 'data' xfs '/data' 2147483648\n"), "{sh}");
        assert!(!sh.contains("disk blk"));
        assert!(sh.contains("-ge 60 ]"));
        assert_eq!(linux(&[disk("blk", Filesystem::Raw, None)]), None);
    }

    #[test]
    fn a_windows_disk_gets_its_drive_letter() {
        let ps = windows(&[disk("logs", Filesystem::Ntfs, Some("E:"))]).unwrap();
        assert!(ps.contains("-DriveLetter E ") && ps.contains("-NewFileSystemLabel 'logs'"), "{ps}");
    }
}
