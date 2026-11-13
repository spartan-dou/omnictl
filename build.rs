//! Incorpore l'icône de l'application aux exécutables Windows.
//!
//! Aucun outil externe (`rc.exe`, `windres`) : ce script écrit lui-même le petit fichier de
//! ressources `.res` à partir de `assets/omnictl.ico` et le passe à l'éditeur de liens, ce
//! qui garde la compilation identique avec MSVC et avec la chaîne gnullvm.

use std::path::PathBuf;

const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;
/// Identifiant du groupe d'icônes ; l'Explorateur prend le premier, l'interface charge celui-ci.
const ICON_ID: u16 = 1;

/// Ajoute une ressource (en-tête de 32 octets, données, alignement sur 4 octets).
fn resource(out: &mut Vec<u8>, kind: u16, id: u16, data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&32u32.to_le_bytes());
    out.extend_from_slice(&[0xFF, 0xFF]);
    out.extend_from_slice(&kind.to_le_bytes());
    out.extend_from_slice(&[0xFF, 0xFF]);
    out.extend_from_slice(&id.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // version des données
    out.extend_from_slice(&0x1030u16.to_le_bytes()); // déplaçable, pure, supprimable
    out.extend_from_slice(&0x0409u16.to_le_bytes()); // langue
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(data);
    while out.len() % 4 != 0 {
        out.push(0);
    }
}

/// Convertit un fichier `.ico` en fichier `.res` : une ressource par image, plus le groupe.
fn ico_to_res(ico: &[u8]) -> Option<Vec<u8>> {
    let u16_at = |i: usize| Some(u16::from_le_bytes([*ico.get(i)?, *ico.get(i + 1)?]));
    let u32_at = |i: usize| Some(u32::from_le_bytes(ico.get(i..i + 4)?.try_into().ok()?));
    if u16_at(0)? != 0 || u16_at(2)? != 1 {
        return None;
    }
    let count = u16_at(4)? as usize;
    let mut res = Vec::new();
    resource(&mut res, 0, 0, &[]); // en-tête de fichier : une ressource vide
    let mut group = vec![0, 0, 1, 0];
    group.extend_from_slice(&(count as u16).to_le_bytes());
    for n in 0..count {
        let entry = 6 + 16 * n;
        let size = u32_at(entry + 8)? as usize;
        let offset = u32_at(entry + 12)? as usize;
        let id = n as u16 + 1;
        resource(&mut res, RT_ICON, id, ico.get(offset..offset + size)?);
        // Entrée du groupe : les 12 premiers octets de l'entrée .ico, puis l'identifiant.
        group.extend_from_slice(ico.get(entry..entry + 12)?);
        group.extend_from_slice(&id.to_le_bytes());
    }
    resource(&mut res, RT_GROUP_ICON, ICON_ID, &group);
    Some(res)
}

const RT_VERSION: u16 = 16;

fn utf16z(text: &str) -> Vec<u8> {
    text.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect()
}

fn pad4(out: &mut Vec<u8>) {
    while out.len() % 4 != 0 {
        out.push(0);
    }
}

/// Un bloc de la ressource de version : longueur, longueur de la valeur, type, clé,
/// valeur, puis les blocs enfants, chaque partie alignée sur 4 octets.
fn version_block(key: &str, value: &[u8], value_len: u16, text: bool, children: &[Vec<u8>]) -> Vec<u8> {
    let mut out = vec![0, 0];
    out.extend_from_slice(&value_len.to_le_bytes());
    out.extend_from_slice(&(text as u16).to_le_bytes());
    out.extend_from_slice(&utf16z(key));
    pad4(&mut out);
    out.extend_from_slice(value);
    for child in children {
        pad4(&mut out);
        out.extend_from_slice(child);
    }
    let len = out.len() as u16;
    out[..2].copy_from_slice(&len.to_le_bytes());
    out
}

/// Ressource VERSIONINFO : nom du produit, éditeur, description et version, affichés par
/// l'Explorateur (Propriétés > Détails) et exigés pour la signature du code.
fn version_info(file_name: &str, description: &str) -> Vec<u8> {
    let version = env!("CARGO_PKG_VERSION");
    let mut parts = version.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
    let (major, minor, patch) = (parts.next().unwrap_or(0), parts.next().unwrap_or(0), parts.next().unwrap_or(0));
    let mut fixed = Vec::new();
    for value in [
        0xFEEF_04BDu32,         // signature
        0x0001_0000,            // version de la structure
        (major << 16) | minor,  // version du fichier
        patch << 16,
        (major << 16) | minor,  // version du produit
        patch << 16,
        0x3F,                   // masque des indicateurs
        0,                      // indicateurs
        0x0004_0004,            // Windows NT
        1,                      // application
        0,
        0,
        0,
    ] {
        fixed.extend_from_slice(&value.to_le_bytes());
    }
    let full_version = format!("{version}.0");
    let strings: Vec<Vec<u8>> = [
        ("CompanyName", "omnictl"),
        ("FileDescription", description),
        ("FileVersion", full_version.as_str()),
        ("InternalName", file_name.trim_end_matches(".exe")),
        ("LegalCopyright", "GPL-2.0-or-later"),
        ("OriginalFilename", file_name),
        ("ProductName", "omnictl"),
        ("ProductVersion", full_version.as_str()),
    ]
    .iter()
    .map(|(key, value)| {
        let data = utf16z(value);
        version_block(key, &data, (data.len() / 2) as u16, true, &[])
    })
    .collect();
    let table = version_block("040904B0", &[], 0, true, &strings);
    let string_info = version_block("StringFileInfo", &[], 0, true, &[table]);
    let translation = [0x09, 0x04, 0xB0, 0x04]; // anglais, Unicode
    let var = version_block("Translation", &translation, 4, false, &[]);
    let var_info = version_block("VarFileInfo", &[], 0, true, &[var]);
    version_block("VS_VERSION_INFO", &fixed, fixed.len() as u16, false, &[string_info, var_info])
}

fn main() {
    println!("cargo:rerun-if-changed=assets/omnictl.ico");
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let ico = std::fs::read("assets/omnictl.ico").expect("assets/omnictl.ico introuvable");
    let icons = ico_to_res(&ico).expect("assets/omnictl.ico : fichier d'icône invalide");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    for (bin, description) in [
        ("omnictl", "omnictl - ROG keyboard and mouse control (command line, MCP)"),
        ("omnictl-gui", "omnictl - ROG keyboard and mouse control"),
    ] {
        let mut res = icons.clone();
        resource(&mut res, RT_VERSION, 1, &version_info(&format!("{bin}.exe"), description));
        let out = out_dir.join(format!("{bin}.res"));
        std::fs::write(&out, res).expect("écriture du fichier de ressources");
        println!("cargo:rustc-link-arg-bin={bin}={}", out.display());
    }
}
