//! Couche HID minimale pour Linux, par l'interface `hidraw` du noyau (aucune dépendance).
//!
//! Chaque collection HID apparaît comme `/dev/hidrawN` ; ses identifiants et son descripteur
//! de rapport se lisent dans `/sys/class/hidraw/hidrawN/device/`. L'accès demande le droit de
//! lecture/écriture sur le périphérique : voir `linux/60-omnictl.rules`.
//!
//! **Non essayé sur du matériel** : écrit d'après la documentation de hidraw et calqué sur
//! la couche Windows, qui, elle, est vérifiée.

use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::path::PathBuf;

#[derive(Debug)]
pub struct HidError(pub &'static str, pub u32);

impl fmt::Display for HidError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (errno {})", self.0, self.1)
    }
}

fn last(what: &'static str) -> HidError {
    HidError(what, std::io::Error::last_os_error().raw_os_error().unwrap_or(0) as u32)
}

/// Une collection HID présente sur le système.
#[derive(Clone, Debug)]
pub struct Info {
    pub path: PathBuf,
    pub pid: u16,
    pub usage_page: u16,
    /// La collection numérote ses rapports (un report ID précède chaque rapport).
    pub numbered: bool,
}

/// Lit dans un descripteur de rapport HID la première « usage page » et la présence de
/// report ID. Les éléments sont des couples préfixe + données ; le préfixe donne la taille.
fn parse_descriptor(desc: &[u8]) -> (u16, bool) {
    let mut usage_page = None;
    let mut numbered = false;
    let mut i = 0;
    while i < desc.len() {
        let prefix = desc[i];
        if prefix == 0xFE {
            // Élément long : taille des données dans l'octet suivant.
            i += 3 + desc.get(i + 1).copied().unwrap_or(0) as usize;
            continue;
        }
        let size = [0usize, 1, 2, 4][(prefix & 0x03) as usize];
        let data = desc.get(i + 1..i + 1 + size).unwrap_or(&[]);
        let value = data.iter().rev().fold(0u32, |acc, b| acc << 8 | *b as u32);
        match prefix & 0xFC {
            0x04 if usage_page.is_none() => usage_page = Some(value as u16), // Usage Page
            0x84 => numbered = true,                                        // Report ID
            _ => {}
        }
        i += 1 + size;
    }
    (usage_page.unwrap_or(0), numbered)
}

/// Extrait (VID, PID) d'un fichier `uevent` : ligne `HID_ID=0003:00000B05:00001ACE`.
fn parse_ids(uevent: &str) -> Option<(u16, u16)> {
    let line = uevent.lines().find_map(|l| l.strip_prefix("HID_ID="))?;
    let mut parts = line.split(':').skip(1);
    let vid = u32::from_str_radix(parts.next()?, 16).ok()?;
    let pid = u32::from_str_radix(parts.next()?, 16).ok()?;
    Some((vid as u16, pid as u16))
}

/// Énumère les collections HID dont le VID correspond.
pub fn enumerate(vid: u16) -> Vec<Info> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir("/sys/class/hidraw") else {
        return out;
    };
    for entry in entries.flatten() {
        let device = entry.path().join("device");
        let Some((v, pid)) = std::fs::read_to_string(device.join("uevent")).ok().as_deref().and_then(parse_ids) else {
            continue;
        };
        if v != vid {
            continue;
        }
        let Ok(desc) = std::fs::read(device.join("report_descriptor")) else {
            continue;
        };
        let (usage_page, numbered) = parse_descriptor(&desc);
        out.push(Info { path: PathBuf::from("/dev").join(entry.file_name()), pid, usage_page, numbered });
    }
    out
}

#[repr(C)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

extern "C" {
    fn poll(fds: *mut PollFd, count: std::ffi::c_ulong, timeout_ms: i32) -> i32;
}

const POLLIN: i16 = 0x0001;

/// Une collection HID ouverte en lecture/écriture.
pub struct Device {
    file: File,
    numbered: bool,
    buf: [u8; 256],
}

impl Device {
    pub fn open(info: &Info) -> Result<Device, HidError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&info.path)
            .map_err(|e| HidError("ouverture de /dev/hidraw (droits ? voir linux/60-omnictl.rules)", e.raw_os_error().unwrap_or(0) as u32))?;
        Ok(Device { file, numbered: info.numbered, buf: [0; 256] })
    }

    /// Écrit un rapport de sortie. `report[0]` est le report ID (0 si la collection n'en
    /// utilise pas) : c'est exactement ce que hidraw attend.
    pub fn write(&mut self, report: &[u8]) -> Result<(), HidError> {
        self.file.write_all(report).map_err(|e| HidError("écriture HID", e.raw_os_error().unwrap_or(0) as u32))
    }

    /// Lit un rapport d'entrée, report ID en tête (0 si la collection n'en utilise pas, pour
    /// présenter la même forme que sous Windows). `None` si rien avant l'échéance.
    pub fn read(&mut self, timeout_ms: u32) -> Result<Option<&[u8]>, HidError> {
        let mut fd = PollFd { fd: self.file.as_raw_fd(), events: POLLIN, revents: 0 };
        match unsafe { poll(&mut fd, 1, timeout_ms.min(i32::MAX as u32) as i32) } {
            0 => return Ok(None),
            n if n < 0 => return Err(last("attente de lecture HID")),
            _ => {}
        }
        // hidraw ne fournit le report ID que si la collection numérote ses rapports.
        let start = if self.numbered { 0 } else { 1 };
        self.buf[0] = 0;
        let n = self.file.read(&mut self.buf[start..]).map_err(|e| HidError("lecture HID", e.raw_os_error().unwrap_or(0) as u32))?;
        Ok(Some(&self.buf[..start + n]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Descripteurs relevés sur le matériel réel.
    const WIRED: [u8; 34] = [
        0x06, 0x00, 0xFF, 0x09, 0x01, 0xA1, 0x01, 0x09, 0x02, 0x15, 0x00, 0x26, 0xFF, 0x00, 0x75, 0x08, 0x95, 0x40, 0x81,
        0x02, 0x09, 0x03, 0x15, 0x00, 0x26, 0xFF, 0x00, 0x75, 0x08, 0x95, 0x40, 0x91, 0x02, 0xC0,
    ];
    const DONGLE_MOUSE: [u8; 36] = [
        0x06, 0x01, 0xFF, 0x09, 0x01, 0xA1, 0x01, 0x85, 0x03, 0x09, 0x02, 0x15, 0x00, 0x26, 0xFF, 0x00, 0x75, 0x08, 0x95,
        0x3F, 0x81, 0x02, 0x09, 0x03, 0x15, 0x00, 0x26, 0xFF, 0x00, 0x75, 0x08, 0x95, 0x3F, 0x91, 0x02, 0xC0,
    ];

    #[test]
    fn descriptors_from_the_real_devices() {
        assert_eq!(parse_descriptor(&WIRED), (0xFF00, false));
        assert_eq!(parse_descriptor(&DONGLE_MOUSE), (0xFF01, true));
        assert_eq!(parse_descriptor(&[0x05, 0x01, 0x09, 0x06]), (0x0001, false)); // clavier standard
    }

    #[test]
    fn truncated_descriptor_does_not_panic() {
        assert_eq!(parse_descriptor(&[0x06, 0x00]), (0x0000, false));
        assert_eq!(parse_descriptor(&[0xFE]), (0, false));
        assert_eq!(parse_descriptor(&[]), (0, false));
    }

    #[test]
    fn ids_from_uevent() {
        let uevent = "DRIVER=hid-generic\nHID_ID=0003:00000B05:00001ACE\nHID_NAME=ASUSTeK ROG OMNI RECEIVER\n";
        assert_eq!(parse_ids(uevent), Some((0x0B05, 0x1ACE)));
        assert_eq!(parse_ids("DRIVER=hid-generic\n"), None);
        assert_eq!(parse_ids("HID_ID=0003:zzzz:0001\n"), None);
    }
}
