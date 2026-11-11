//! Mémoire locale des couleurs touche par touche.
//!
//! Le clavier ne sait pas enregistrer une couleur par touche : le mode direct est volatil.
//! On garde donc la dernière carte appliquée dans un petit fichier texte
//! (`%APPDATA%\omnictl\keys.txt`, une ligne `id=rrggbb` par touche) pour pouvoir la
//! réappliquer avec `omnictl apply`.

use crate::proto::Rgb;
use std::path::PathBuf;

pub fn path() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA").or_else(|| std::env::var_os("HOME"))?;
    Some(PathBuf::from(base).join("omnictl").join("keys.txt"))
}

pub fn load() -> Vec<(u8, Rgb)> {
    let Some(text) = path().and_then(|p| std::fs::read_to_string(p).ok()) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| {
            let (id, color) = line.trim().split_once('=')?;
            Some((u8::from_str_radix(id, 16).ok()?, Rgb::parse(color)?))
        })
        .collect()
}

pub fn save(leds: &[(u8, Rgb)]) -> std::io::Result<()> {
    let path = path().ok_or_else(|| std::io::Error::other("dossier de configuration introuvable"))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text: String = leds
        .iter()
        .map(|(id, c)| format!("{id:02x}={}\n", &c.hex()[1..]))
        .collect();
    std::fs::write(path, text)
}

/// Fusionne `changes` dans la carte enregistrée et renvoie la carte complète.
pub fn merge(changes: &[(u8, Rgb)]) -> Vec<(u8, Rgb)> {
    let mut all = load();
    for &(id, c) in changes {
        match all.iter_mut().find(|(i, _)| *i == id) {
            Some(slot) => slot.1 = c,
            None => all.push((id, c)),
        }
    }
    all
}

/// Réglage oui/non, mémorisé par la présence d'un fichier témoin vide.
fn flag_path(name: &str) -> Option<PathBuf> {
    Some(path()?.with_file_name(name))
}

pub fn flag(name: &str) -> bool {
    flag_path(name).is_some_and(|p| p.exists())
}

pub fn set_flag(name: &str, on: bool) -> std::io::Result<()> {
    let path = flag_path(name).ok_or_else(|| std::io::Error::other("dossier de configuration introuvable"))?;
    if on {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, "")
    } else if path.exists() {
        std::fs::remove_file(path)
    } else {
        Ok(())
    }
}

/// Gardien : l'interface, restée en arrière-plan, renvoie les couleurs touche par touche
/// au clavier chaque fois qu'il se réveille ou se reconnecte.
pub fn guard() -> bool {
    flag("guard")
}

pub fn set_guard(on: bool) -> std::io::Result<()> {
    set_flag("guard", on)
}
