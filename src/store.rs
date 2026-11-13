//! Mémoire locale des couleurs et effets touche par touche.
//!
//! Le clavier ne sait pas enregistrer une couleur par touche : le mode direct est volatil.
//! On garde donc la dernière carte appliquée dans un petit fichier texte
//! (`%APPDATA%\omnictl\keys.txt`), une ligne par touche : `id=rrggbb`, suivie au besoin
//! du code de son effet (`id=rrggbb,b` pour une respiration ; voir [`KeyFx`]).

use crate::keyfx::{Gradient, KeyFx, KeyStyle};
use crate::proto::Rgb;
use std::path::PathBuf;

/// Nom du fichier qui, posé à côté de l'exécutable, rend l'application portable.
pub const PORTABLE_MARKER: &str = "portable.txt";

/// Dossier des réglages en mode portable : `settings` à côté de l'exécutable, quand le
/// fichier [`PORTABLE_MARKER`] s'y trouve. Rien n'est alors écrit dans le profil.
pub fn portable_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    dir.join(PORTABLE_MARKER).is_file().then(|| dir.join("settings"))
}

pub fn path() -> Option<PathBuf> {
    if let Some(dir) = portable_dir() {
        return Some(dir.join("keys.txt"));
    }
    // Windows : %APPDATA%. Ailleurs : $XDG_CONFIG_HOME, ou ~/.config à défaut.
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from))
        .or_else(|| Some(PathBuf::from(std::env::var_os("HOME")?).join(".config")))?;
    Some(base.join("omnictl").join("keys.txt"))
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
    std::fs::write(path, styles_text(styles))
}

fn styles_text(styles: &[(u8, KeyStyle)]) -> String {
    styles.iter().map(|(id, style)| format_line(*id, style)).collect()
}

/// Fichier des couleurs associées à une application (`apps\<exécutable>.txt`). Le nom est
/// réduit à des caractères sûrs : il vient du système, mais finit dans un chemin.
fn app_file(exe: &str) -> Option<PathBuf> {
    let name = app_name(exe);
    if name.is_empty() {
        return None;
    }
    Some(path()?.with_file_name("apps").join(format!("{name}.txt")))
}

fn app_name(exe: &str) -> String {
    exe.to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        .collect::<String>()
        .trim_matches('.')
        .to_string()
}

/// Couleurs associées à l'application `exe` (nom de son exécutable), s'il y en a.
pub fn app_styles(exe: &str) -> Option<Vec<(u8, KeyStyle)>> {
    let text = std::fs::read_to_string(app_file(exe)?).ok()?;
    Some(text.lines().filter_map(parse_line).collect())
}

pub fn save_app(exe: &str, styles: &[(u8, KeyStyle)]) -> std::io::Result<()> {
    let path = app_file(exe).ok_or_else(|| std::io::Error::other("nom d'application inutilisable"))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, styles_text(styles))
}

pub fn remove_app(exe: &str) -> std::io::Result<()> {
    match app_file(exe) {
        Some(path) if path.exists() => std::fs::remove_file(path),
        _ => Ok(()),
    }
}

/// Applications ayant des couleurs associées, par ordre alphabétique.
pub fn list_apps() -> Vec<String> {
    let Some(dir) = path().map(|p| p.with_file_name("apps")) else {
        return Vec::new();
    };
    let mut apps: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| Some(entry.file_name().to_str()?.strip_suffix(".txt")?.to_string()))
        .collect();
    apps.sort();
    apps
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

/// Petit réglage texte, dans un fichier à son nom. `None` s'il n'a jamais été écrit.
pub fn value(name: &str) -> Option<String> {
    std::fs::read_to_string(flag_path(name)?).ok().map(|text| text.trim().to_string())
}

pub fn set_value(name: &str, text: &str) -> std::io::Result<()> {
    let path = flag_path(name).ok_or_else(|| std::io::Error::other("dossier de configuration introuvable"))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, text)
}

fn parse_gradient(text: &str) -> Option<Gradient> {
    let (colors, scroll) = text.trim().split_once(';')?;
    let stops: Option<Vec<Rgb>> = colors.split(',').map(Rgb::parse).collect();
    Some(Gradient { stops: stops.filter(|s| !s.is_empty())?, scroll: scroll == "1" })
}

fn format_gradient(gradient: &Gradient) -> String {
    let colors: Vec<String> = gradient.stops.iter().map(|c| c.hex()[1..].to_string()).collect();
    format!("{};{}", colors.join(","), gradient.scroll as u8)
}

/// Dégradé commun des touches réglées sur « Dégradé ».
pub fn gradient() -> Gradient {
    value("gradient").and_then(|text| parse_gradient(&text)).unwrap_or_default()
}

pub fn set_gradient(gradient: &Gradient) -> std::io::Result<()> {
    set_value("gradient", &format_gradient(gradient))
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

// ───────────── Export / import de toute la configuration ─────────────
//
// Un seul fichier texte, lisible : une ligne d'en-tête, puis chaque fichier de réglages sous
// la forme `[nom]` suivi de son contenu, chaque ligne préfixée par `| `.

const EXPORT_HEADER: &str = "# omnictl configuration 1";
/// Réglages propres à un PC, jamais exportés : le gardien va avec un démarrage automatique.
const LOCAL_ONLY: &[&str] = &["guard"];

/// Nom d'entrée acceptable : `fichier` ou `apps/fichier`, caractères sûrs uniquement. Le
/// fichier importé peut venir d'ailleurs : rien ne doit pouvoir écrire hors du dossier.
fn entry_ok(name: &str) -> bool {
    let file = name.strip_prefix("apps/").unwrap_or(name);
    !file.is_empty()
        && !file.starts_with('.')
        && file.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        && !LOCAL_ONLY.contains(&name)
}

/// Fichiers de réglages du dossier `dir`, par nom d'entrée.
fn config_files(dir: &std::path::Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    for (prefix, sub) in [("", dir.to_path_buf()), ("apps/", dir.join("apps"))] {
        for entry in std::fs::read_dir(&sub).into_iter().flatten().flatten() {
            let path = entry.path();
            let Some(file) = entry.file_name().to_str().map(str::to_string) else { continue };
            let name = format!("{prefix}{file}");
            if path.is_file() && entry_ok(&name) {
                out.push((name, path));
            }
        }
    }
    out.sort();
    out
}

fn export_dir(dir: &std::path::Path) -> String {
    let mut out = format!("{EXPORT_HEADER}\n");
    for (name, path) in config_files(dir) {
        out.push_str(&format!("[{name}]\n"));
        for line in std::fs::read_to_string(path).unwrap_or_default().lines() {
            out.push_str(&format!("| {line}\n"));
        }
    }
    out
}

/// Lit un export : liste (nom, contenu). Erreur si le fichier n'en est pas un.
fn parse_export(text: &str) -> Result<Vec<(String, String)>, String> {
    let mut lines = text.trim_start_matches('\u{FEFF}').lines();
    if lines.next().map(str::trim) != Some(EXPORT_HEADER) {
        return Err("ce fichier n'est pas un export de configuration omnictl".into());
    }
    let mut entries: Vec<(String, String)> = Vec::new();
    for line in lines {
        let line = line.trim_end_matches('\r');
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            if !entry_ok(name) {
                return Err(format!("entrée refusée : {name}"));
            }
            entries.push((name.to_string(), String::new()));
        } else if let Some(content) = line.strip_prefix("| ").or(line.strip_prefix("|").filter(|r| r.is_empty())) {
            let (_, body) = entries.last_mut().ok_or("contenu avant la première entrée")?;
            body.push_str(content);
            body.push('\n');
        } else if !line.trim().is_empty() {
            return Err(format!("ligne inattendue : {line}"));
        }
    }
    Ok(entries)
}

/// Remplace la configuration de `dir` par celle de l'export (les réglages propres au PC
/// restent). Tout est vérifié avant d'écrire quoi que ce soit.
fn import_dir(dir: &std::path::Path, text: &str) -> Result<usize, String> {
    let entries = parse_export(text)?;
    let io = |e: std::io::Error| format!("écriture impossible : {e}");
    std::fs::create_dir_all(dir.join("apps")).map_err(io)?;
    for (_, path) in config_files(dir) {
        std::fs::remove_file(path).map_err(io)?;
    }
    for (name, body) in &entries {
        std::fs::write(dir.join(name), body).map_err(io)?;
    }
    Ok(entries.len())
}

fn config_dir() -> Result<PathBuf, String> {
    path().and_then(|p| p.parent().map(PathBuf::from)).ok_or_else(|| "dossier de configuration introuvable".into())
}

/// Toute la configuration (couleurs par touche, profils par application, dégradé, réglages
/// de l'interface) dans un seul texte, à enregistrer dans un fichier.
pub fn export_config() -> Result<String, String> {
    Ok(export_dir(&config_dir()?))
}

/// Remplace la configuration par un export. Renvoie le nombre de fichiers restaurés.
pub fn import_config(text: &str) -> Result<usize, String> {
    import_dir(&config_dir()?, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_import_round_trip() {
        let base = std::env::temp_dir().join(format!("omnictl-test-{}", std::process::id()));
        let (a, b) = (base.join("a"), base.join("b"));
        std::fs::create_dir_all(a.join("apps")).unwrap();
        std::fs::write(a.join("keys.txt"), "2d=ff6000\n08=0040ff,w\n").unwrap();
        std::fs::write(a.join("gradient"), "ff0000,0000ff;1").unwrap();
        std::fs::write(a.join("react"), "2").unwrap();
        std::fs::write(a.join("guard"), "").unwrap();
        std::fs::write(a.join("apps").join("game.exe.txt"), "10=ffffff\n").unwrap();
        let text = export_dir(&a);
        assert!(!text.contains("[guard]"), "réglage propre au PC exporté");

        std::fs::create_dir_all(b.join("apps")).unwrap();
        std::fs::write(b.join("apps").join("old.exe.txt"), "10=000000\n").unwrap();
        std::fs::write(b.join("guard"), "").unwrap();
        assert_eq!(import_dir(&b, &text), Ok(4));
        assert_eq!(std::fs::read_to_string(b.join("keys.txt")).unwrap(), "2d=ff6000\n08=0040ff,w\n");
        assert_eq!(std::fs::read_to_string(b.join("gradient")).unwrap().trim(), "ff0000,0000ff;1");
        assert!(b.join("apps").join("game.exe.txt").exists());
        assert!(!b.join("apps").join("old.exe.txt").exists(), "l'import remplace les profils");
        assert!(b.join("guard").exists(), "le gardien du PC reste tel quel");
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn hostile_exports_are_rejected() {
        let h = EXPORT_HEADER;
        assert!(parse_export("bonjour").is_err());
        assert!(parse_export(&format!("{h}\n[../evil]\n| x\n")).is_err());
        assert!(parse_export(&format!("{h}\n[apps/../../x]\n")).is_err());
        assert!(parse_export(&format!("{h}\n[C:\\x]\n")).is_err());
        assert!(parse_export(&format!("{h}\n[guard]\n")).is_err());
        assert!(parse_export(&format!("{h}\n| sans entrée\n")).is_err());
        assert_eq!(parse_export(&format!("\u{FEFF}{h}\r\n[react]\r\n| 2\r\n")), Ok(vec![("react".into(), "2\n".into())]));
    }

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
    fn app_names_are_reduced_to_safe_file_names() {
        assert_eq!(app_name("Chrome.EXE"), "chrome.exe");
        assert_eq!(app_name("..\\..\\secret.txt"), "secret.txt");
        assert_eq!(app_name("a/b:c*d.exe"), "abcd.exe");
        assert_eq!(app_name("...."), "");
    }

    #[test]
    fn gradient_round_trip() {
        let g = Gradient { stops: vec![Rgb(255, 0, 0), Rgb(0, 0, 255)], scroll: true };
        assert_eq!(format_gradient(&g), "ff0000,0000ff;1");
        assert_eq!(parse_gradient("ff0000,0000ff;1"), Some(g));
        assert_eq!(parse_gradient("ff0000,pasunecouleur;0"), None);
        assert_eq!(parse_gradient(";1"), None);
        assert_eq!(parse_gradient("ff0000"), None);
    }

    #[test]
    fn unknown_effect_falls_back_to_fixed_and_garbage_is_skipped() {
        assert_eq!(parse_line("10=ffffff,zz"), Some((0x10, KeyStyle::fixed(Rgb(255, 255, 255)))));
        assert_eq!(parse_line("pas une ligne"), None);
        assert_eq!(parse_line("zz=ffffff"), None);
    }
}
