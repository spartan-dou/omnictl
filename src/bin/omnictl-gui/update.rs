//! Mise à jour depuis les releases GitHub du projet.
//!
//! Au démarrage, l'application regarde si une version plus récente est publiée. Rien n'est
//! installé sans un clic de l'utilisateur. L'installation télécharge les exécutables de la
//! dernière release, vérifie leur empreinte SHA-256 contre le fichier publié avec eux, puis
//! remplace les fichiers en place (l'ancien est renommé, Windows ne permettant pas d'écraser
//! un programme en cours d'exécution) et relance l'application.
//!
//! Confiance : les fichiers viennent du dépôt ci-dessous, en HTTPS. Quiconque peut publier
//! une release sur ce dépôt peut donc distribuer une mise à jour.

use crate::win;
use std::path::{Path, PathBuf};

const BASE: &str = "https://github.com/spartan-dou/omnictl/releases/latest/download/";
const ARCH: &str = if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" };

/// Télécharge un fichier de la dernière release.
fn fetch(name: &str) -> Option<Vec<u8>> {
    let tmp = std::env::temp_dir().join(format!("omnictl-update-{}-{name}", std::process::id()));
    let ok = win::download(&format!("{BASE}{name}"), &tmp);
    let data = if ok { std::fs::read(&tmp).ok() } else { None };
    let _ = std::fs::remove_file(&tmp);
    data
}

fn parse_version(text: &str) -> Option<(u32, u32, u32)> {
    let mut parts = text.trim().trim_start_matches('v').split('.').map(|p| p.parse::<u32>().ok());
    let version = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(version)
}

/// `latest` est-elle strictement plus récente que `current` ?
fn is_newer(latest: &str, current: &str) -> bool {
    matches!((parse_version(latest), parse_version(current)), (Some(l), Some(c)) if l > c)
}

/// Empreinte attendue de `asset` dans un fichier `SHA256SUMS.txt` (lignes « empreinte  nom »).
fn expected_hash<'a>(sums: &'a str, asset: &str) -> Option<&'a str> {
    sums.lines().find_map(|line| {
        let (hash, name) = line.split_once(char::is_whitespace)?;
        (name.trim() == asset && hash.len() == 64).then_some(hash)
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Numéro de la dernière version publiée, si elle est plus récente que celle qui tourne.
/// `None` aussi quand le réseau ne répond pas : l'absence de mise à jour n'est pas une erreur.
pub fn available() -> Option<String> {
    let text = String::from_utf8(fetch("VERSION")?).ok()?;
    let latest = text.trim().trim_start_matches('v').to_string();
    is_newer(&latest, env!("CARGO_PKG_VERSION")).then_some(latest)
}

/// Fichiers à remplacer : l'interface elle-même, et la ligne de commande si elle est à côté.
fn targets() -> Result<Vec<(String, PathBuf)>, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = exe.parent().ok_or("dossier de l'application introuvable")?.to_path_buf();
    let mut out = vec![(format!("omnictl-gui-{ARCH}.exe"), exe)];
    for name in [format!("omnictl-{ARCH}.exe"), "omnictl.exe".to_string()] {
        let path = dir.join(&name);
        if path.exists() {
            out.push((format!("omnictl-{ARCH}.exe"), path));
            break;
        }
    }
    Ok(out)
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

fn sums() -> Result<String, String> {
    fetch("SHA256SUMS.txt")
        .and_then(|d| String::from_utf8(d).ok())
        .ok_or_else(|| "téléchargement des empreintes impossible".to_string())
}

/// Télécharge `asset` et vérifie son empreinte contre la liste publiée.
fn fetch_verified(sums: &str, asset: &str) -> Result<Vec<u8>, String> {
    let data = fetch(asset).ok_or(format!("téléchargement de {asset} impossible"))?;
    let expected = expected_hash(sums, asset).ok_or(format!("empreinte de {asset} absente"))?;
    let actual = win::sha256(&data).map(|h| hex(&h)).ok_or("calcul d'empreinte impossible")?;
    if !actual.eq_ignore_ascii_case(expected) {
        return Err(format!("{asset} : empreinte inattendue, mise à jour abandonnée"));
    }
    Ok(data)
}

/// Installation faite par le paquet MSI : télécharge et vérifie le MSI de la dernière
/// version, puis le lance (Windows demande l'accord d'un administrateur). L'application
/// doit se fermer aussitôt pour que ses fichiers puissent être remplacés.
pub fn install_msi() -> Result<(), String> {
    let asset = format!("omnictl-{ARCH}.msi");
    let data = fetch_verified(&sums()?, &asset)?;
    let path = std::env::temp_dir().join(&asset);
    std::fs::write(&path, data).map_err(|e| format!("écriture impossible : {e}"))?;
    std::process::Command::new("msiexec")
        .arg("/i")
        .arg(&path)
        .arg("/passive")
        .spawn()
        .map_err(|e| format!("lancement de l'installeur impossible : {e}"))?;
    Ok(())
}

/// Télécharge, vérifie puis met en place la dernière version. À la réussite, il reste à
/// relancer l'application.
pub fn install() -> Result<(), String> {
    let sums = sums()?;
    let targets = targets()?;
    // Tout télécharger et vérifier avant de toucher au moindre fichier en place.
    for (asset, path) in &targets {
        let data = fetch_verified(&sums, asset)?;
        std::fs::write(sibling(path, ".new"), data).map_err(|e| format!("écriture impossible : {e}"))?;
    }
    for (_, path) in &targets {
        let (old, new) = (sibling(path, ".old"), sibling(path, ".new"));
        let _ = std::fs::remove_file(&old);
        std::fs::rename(path, &old).map_err(|e| format!("remplacement impossible : {e}"))?;
        if let Err(e) = std::fs::rename(&new, path) {
            let _ = std::fs::rename(&old, path); // remettre l'ancien en place
            return Err(format!("remplacement impossible : {e}"));
        }
    }
    Ok(())
}

/// Supprime les anciens exécutables laissés par une mise à jour précédente.
pub fn cleanup() {
    if let Ok(targets) = targets() {
        for (_, path) in targets {
            let _ = std::fs::remove_file(sibling(&path, ".old"));
            let _ = std::fs::remove_file(sibling(&path, ".new"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_numerically() {
        assert!(is_newer("0.3.0", "0.2.0"));
        assert!(is_newer("v0.10.0", "0.9.9"), "10 > 9, pas une comparaison de texte");
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(!is_newer("0.2.0", "0.2.0"));
        assert!(!is_newer("0.1.9", "0.2.0"));
    }

    #[test]
    fn malformed_versions_never_trigger_an_update() {
        for bad in ["", "abc", "1.2", "1.2.3.4", "1.x.3", "<html>404</html>"] {
            assert!(!is_newer(bad, "0.2.0"), "{bad:?}");
        }
    }

    #[test]
    fn hash_lookup() {
        let h1 = "a".repeat(64);
        let h2 = "b".repeat(64);
        let sums = format!("{h1}  omnictl-gui-x64.exe\n{h2}  omnictl-x64.exe\n");
        assert_eq!(expected_hash(&sums, "omnictl-x64.exe"), Some(h2.as_str()));
        assert_eq!(expected_hash(&sums, "omnictl-gui-x64.exe"), Some(h1.as_str()));
        assert_eq!(expected_hash(&sums, "autre.exe"), None);
        assert_eq!(expected_hash("tropcourt  omnictl-x64.exe\n", "omnictl-x64.exe"), None);
    }

    #[test]
    fn sibling_names() {
        assert_eq!(sibling(Path::new("C:/x/omnictl-gui-x64.exe"), ".old"), PathBuf::from("C:/x/omnictl-gui-x64.exe.old"));
    }
}
