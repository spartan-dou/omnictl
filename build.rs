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

fn main() {
    println!("cargo:rerun-if-changed=assets/omnictl.ico");
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let ico = std::fs::read("assets/omnictl.ico").expect("assets/omnictl.ico introuvable");
    let res = ico_to_res(&ico).expect("assets/omnictl.ico : fichier d'icône invalide");
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR")).join("omnictl.res");
    std::fs::write(&out, res).expect("écriture du fichier de ressources");
    println!("cargo:rustc-link-arg-bins={}", out.display());
}
