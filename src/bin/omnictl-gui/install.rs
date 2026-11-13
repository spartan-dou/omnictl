//! Installation pour l'utilisateur courant, sans droits administrateur.
//!
//! L'exécutable est son propre installeur : lancé depuis un dossier quelconque, il propose
//! de se copier dans `%LOCALAPPDATA%\Programs\omnictl`, d'ajouter un raccourci au menu
//! Démarrer et de s'inscrire dans la liste des applications de Windows (pour la
//! désinstallation). Tout se passe dans le profil de l'utilisateur et sa branche de registre.

use crate::win;
use std::path::{Path, PathBuf};

const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\omnictl";
const ARCH: &str = if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" };

pub fn install_dir() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("Programs").join("omnictl"))
}

fn shortcut_path() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("APPDATA")?).join(r"Microsoft\Windows\Start Menu\Programs\omnictl.lnk"))
}

fn same_dir(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().trim_end_matches('\\').eq_ignore_ascii_case(b.to_string_lossy().trim_end_matches('\\'))
}

/// Fichier posé par le paquet MSI à côté de l'exécutable.
const MSI_MARKER: &str = "msi.txt";

/// Comment cette copie de l'application a été mise en place.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// Exécutable lancé d'un dossier quelconque (Téléchargements…) : on propose d'installer.
    Loose,
    /// Version portable : réglages à côté de l'exécutable, rien d'écrit sur le PC.
    Portable,
    /// Installée pour l'utilisateur par l'application elle-même.
    User,
    /// Installée par le paquet MSI (Program Files) : les mises à jour passent par un MSI.
    Msi,
}

pub fn kind() -> Kind {
    let here = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf));
    if omnictl::store::portable_dir().is_some() {
        Kind::Portable
    } else if here.as_deref().is_some_and(|d| d.join(MSI_MARKER).is_file()) {
        Kind::Msi
    } else if is_installed() {
        Kind::User
    } else {
        Kind::Loose
    }
}

/// L'application tourne-t-elle depuis son dossier d'installation ?
pub fn is_installed() -> bool {
    let exe = std::env::current_exe().ok();
    match (exe.as_deref().and_then(Path::parent), install_dir()) {
        (Some(here), Some(dir)) => same_dir(here, &dir),
        _ => false,
    }
}

/// Installe l'application et renvoie le chemin de l'exécutable installé, à lancer ensuite.
pub fn install() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = install_dir().ok_or("dossier d'installation introuvable")?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("création du dossier impossible : {e}"))?;
    let target = dir.join("omnictl-gui.exe");
    std::fs::copy(&exe, &target).map_err(|e| format!("copie impossible : {e}"))?;
    // La ligne de commande suit si elle se trouve à côté.
    if let Some(here) = exe.parent() {
        for name in [format!("omnictl-{ARCH}.exe"), "omnictl.exe".to_string()] {
            if here.join(&name).exists() {
                let _ = std::fs::copy(here.join(&name), dir.join("omnictl.exe"));
                break;
            }
        }
    }
    let shortcut = shortcut_path().ok_or("menu Démarrer introuvable")?;
    if !win::create_shortcut(&shortcut, &target, "Éclairage et réglages du clavier et de la souris ROG") {
        return Err("création du raccourci impossible".into());
    }
    let exe_text = target.to_string_lossy().to_string();
    let size_kb = std::fs::metadata(&target).map(|m| m.len() / 1024).unwrap_or(0) as u32;
    let ok = win::reg_set_str(UNINSTALL_KEY, "DisplayName", "omnictl")
        && win::reg_set_str(UNINSTALL_KEY, "DisplayVersion", env!("CARGO_PKG_VERSION"))
        && win::reg_set_str(UNINSTALL_KEY, "Publisher", "omnictl (projet libre)")
        && win::reg_set_str(UNINSTALL_KEY, "DisplayIcon", &exe_text)
        && win::reg_set_str(UNINSTALL_KEY, "InstallLocation", &dir.to_string_lossy())
        && win::reg_set_str(UNINSTALL_KEY, "UninstallString", &format!("\"{exe_text}\" --uninstall"))
        && win::reg_set_u32(UNINSTALL_KEY, "NoModify", 1)
        && win::reg_set_u32(UNINSTALL_KEY, "NoRepair", 1)
        && win::reg_set_u32(UNINSTALL_KEY, "EstimatedSize", size_kb);
    if !ok {
        return Err("inscription dans les applications de Windows impossible".into());
    }
    Ok(target)
}

/// Met à jour le numéro de version affiché par Windows (après une mise à jour).
pub fn refresh_version() {
    if is_installed() {
        win::reg_set_str(UNINSTALL_KEY, "DisplayVersion", env!("CARGO_PKG_VERSION"));
    }
}

/// Désinstalle après confirmation : raccourci, inscription, démarrage automatique, puis le
/// dossier du programme. Les réglages de l'utilisateur (`%APPDATA%\omnictl`) sont conservés.
pub fn uninstall() {
    if !is_installed() {
        win::error_box("omnictl n'est pas installé : cette copie se supprime simplement en effaçant son fichier.");
        return;
    }
    if !win::confirm("Désinstaller omnictl ?\n\nVos réglages sont conservés.") {
        return;
    }
    if let Some(shortcut) = shortcut_path() {
        let _ = std::fs::remove_file(shortcut);
    }
    win::reg_delete_tree(UNINSTALL_KEY);
    win::set_autostart("omnictl", None);
    // Un programme ne peut pas effacer son propre fichier pendant qu'il tourne : une
    // commande différée supprime le dossier une fois celui-ci fermé.
    if let Some(dir) = install_dir() {
        let command = format!("ping -n 3 127.0.0.1 >nul & rmdir /S /Q \"{}\"", dir.display());
        win::spawn_hidden("cmd.exe", &["/C", &command]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_comparison_ignores_case_and_trailing_separator() {
        assert!(same_dir(Path::new(r"C:\Users\A\AppData\Local\Programs\omnictl"), Path::new(r"c:\users\a\appdata\local\programs\OMNICTL\")));
        assert!(!same_dir(Path::new(r"C:\Users\A\Downloads"), Path::new(r"C:\Users\A\AppData\Local\Programs\omnictl")));
    }
}
