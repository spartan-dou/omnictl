//! Mémoire locale des couleurs et effets touche par touche.
//!
//! Le clavier ne sait pas enregistrer une couleur par touche : le mode direct est volatil.
//! On garde donc la dernière carte appliquée dans un petit fichier texte
//! (`%APPDATA%\omnictl\keys.txt`), une ligne par touche : `id=rrggbb`, suivie au besoin
//! du code de son effet (`id=rrggbb,b` pour une respiration ; voir [`KeyFx`]).

use crate::keyfx::{KeyFx, KeyStyle};
use crate::proto::Rgb;
use std::path::PathBuf;

pub fn path() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA").or_else(|| std::env::var_os("HOME"))?;
    Some(PathBuf::from(base).join("omnictl").join("keys.txt"))
}

fn parse_line(line: &str) -> Option<(u8, KeyStyle)> {
    let (id, rest) = line.trim().split_once('=')?;
    let (color, fx) = rest.split_once(',').unwrap_or((rest, ""));
    Some((
        u8::from_str_radix(id, 16).ok()?,
        KeyStyle { color: Rgb::parse(color)?, fx: KeyFx::from_code(fx).unwrap_or_default() },
    ))
}

fn format_line(id: u8, style: &KeyStyle) -> String {
    let fx = style.fx.code();
    let sep = if fx.is_empty() { "" } else { "," };
    format!("{id:02x}={}{sep}{fx}\n", &style.color.hex()[1..])
}

/// Couleur et effet de chaque touche mémorisée.
pub fn load_styles() -> Vec<(u8, KeyStyle)> {
    let Some(text) = path().and_then(|p| std::fs::read_to_string(p).ok()) else {
        return Vec::new();
    };
    text.lines().filter_map(parse_line).collect()
}

pub fn save_styles(styles: &[(u8, KeyStyle)]) -> std::io::Result<()> {
    let path = path().ok_or_else(|| std::io::Error::other("dossier de configuration introuvable"))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text: String = styles.iter().map(|(id, style)| format_line(*id, style)).collect();
    std::fs::write(path, text)
}

/// Couleurs de base seules (sans les effets), pour une application en une fois.
pub fn load() -> Vec<(u8, Rgb)> {
    load_styles().into_iter().map(|(id, style)| (id, style.color)).collect()
}

/// Donne une couleur fixe aux touches citées et laisse les autres comme elles sont.
pub fn update_colors(changes: &[(u8, Rgb)]) -> std::io::Result<()> {
    let mut all = load_styles();
    for &(id, color) in changes {
        let style = KeyStyle::fixed(color);
        match all.iter_mut().find(|(i, _)| *i == id) {
            Some(slot) => slot.1 = style,
            None => all.push((id, style)),
        }
    }
    save_styles(&all)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_round_trip() {
        let plain = KeyStyle::fixed(Rgb(255, 96, 0));
        let wave = KeyStyle { color: Rgb(0, 64, 255), fx: KeyFx::Wave };
        assert_eq!(format_line(0x2d, &plain), "2d=ff6000\n");
        assert_eq!(format_line(0x08, &wave), "08=0040ff,w\n");
        assert_eq!(parse_line("2d=ff6000"), Some((0x2d, plain)));
        assert_eq!(parse_line("08=0040ff,w"), Some((0x08, wave)));
    }

    #[test]
    fn unknown_effect_falls_back_to_fixed_and_garbage_is_skipped() {
        assert_eq!(parse_line("10=ffffff,zz"), Some((0x10, KeyStyle::fixed(Rgb(255, 255, 255)))));
        assert_eq!(parse_line("pas une ligne"), None);
        assert_eq!(parse_line("zz=ffffff"), None);
    }
}
