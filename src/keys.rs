//! Table des touches du Scope II 96 : identifiant de LED, nom, scancode et position physique.
//!
//! L'identifiant de LED vaut `colonne × 8 + ligne` dans la matrice électrique du clavier
//! (table reprise d'OpenRGB, `AsusAuraKeyboardLayouts.h`, puis vérifiée sur le matériel).
//! Les noms sont *positionnels* (ceux d'un clavier QWERTY US) : la touche nommée `q` est
//! celle située à droite de Tab, quelle que soit la lettre gravée dessus. Le scancode
//! permet de retrouver la lettre réellement gravée selon la disposition active de Windows.

pub struct Key {
    /// Identifiant de LED envoyé au clavier.
    pub id: u8,
    /// Nom positionnel (QWERTY US).
    pub name: &'static str,
    /// Scancode « set 1 » de la touche, 0 si sans objet.
    pub scan: u8,
    /// (Sert au dessin du clavier dans l'interface graphique, à venir.)
    #[allow(dead_code)]
    /// Position et taille en quarts d'unité de touche : (x, y, largeur, hauteur).
    pub rect: (u8, u8, u8, u8),
}

const fn k(id: u8, name: &'static str, scan: u8, x: u8, y: u8, w: u8, h: u8) -> Key {
    Key { id, name, scan, rect: (x, y, w, h) }
}

/// Toutes les LED adressables. Les positions sont en quarts d'unité (4 = une touche).
pub static KEYS: &[Key] = &[
    // Rangée des fonctions
    k(0x00, "esc", 0x01, 0, 0, 4, 4),
    k(0x08, "f1", 0x3B, 4, 0, 4, 4),
    k(0x10, "f2", 0x3C, 8, 0, 4, 4),
    k(0x18, "f3", 0x3D, 12, 0, 4, 4),
    k(0x20, "f4", 0x3E, 16, 0, 4, 4),
    k(0x28, "f5", 0x3F, 20, 0, 4, 4),
    k(0x30, "f6", 0x40, 24, 0, 4, 4),
    k(0x38, "f7", 0x41, 28, 0, 4, 4),
    k(0x40, "f8", 0x42, 32, 0, 4, 4),
    k(0x48, "f9", 0x43, 36, 0, 4, 4),
    k(0x50, "f10", 0x44, 40, 0, 4, 4),
    k(0x58, "f11", 0x57, 44, 0, 4, 4),
    k(0x60, "f12", 0x58, 48, 0, 4, 4),
    k(0x68, "insert", 0, 52, 0, 4, 4),
    k(0x70, "delete", 0, 56, 0, 4, 4),
    k(0x78, "pageup", 0, 60, 0, 4, 4),
    k(0x80, "pagedown", 0, 64, 0, 4, 4),
    k(0x88, "logo", 0, 72, 0, 4, 4),
    // Rangée des chiffres
    k(0x01, "grave", 0x29, 0, 4, 4, 4),
    k(0x09, "1", 0x02, 4, 4, 4, 4),
    k(0x11, "2", 0x03, 8, 4, 4, 4),
    k(0x19, "3", 0x04, 12, 4, 4, 4),
    k(0x21, "4", 0x05, 16, 4, 4, 4),
    k(0x29, "5", 0x06, 20, 4, 4, 4),
    k(0x31, "6", 0x07, 24, 4, 4, 4),
    k(0x39, "7", 0x08, 28, 4, 4, 4),
    k(0x41, "8", 0x09, 32, 4, 4, 4),
    k(0x49, "9", 0x0A, 36, 4, 4, 4),
    k(0x51, "0", 0x0B, 40, 4, 4, 4),
    k(0x59, "minus", 0x0C, 44, 4, 4, 4),
    k(0x61, "equal", 0x0D, 48, 4, 4, 4),
    k(0x69, "backspace", 0, 52, 4, 8, 4),
    k(0x79, "numlock", 0, 60, 4, 4, 4),
    k(0x81, "numdivide", 0, 64, 4, 4, 4),
    k(0x89, "nummultiply", 0, 68, 4, 4, 4),
    k(0x91, "numminus", 0, 72, 4, 4, 4),
    // Rangée du haut des lettres
    k(0x02, "tab", 0, 0, 8, 6, 4),
    k(0x0A, "q", 0x10, 6, 8, 4, 4),
    k(0x12, "w", 0x11, 10, 8, 4, 4),
    k(0x1A, "e", 0x12, 14, 8, 4, 4),
    k(0x22, "r", 0x13, 18, 8, 4, 4),
    k(0x2A, "t", 0x14, 22, 8, 4, 4),
    k(0x32, "y", 0x15, 26, 8, 4, 4),
    k(0x3A, "u", 0x16, 30, 8, 4, 4),
    k(0x42, "i", 0x17, 34, 8, 4, 4),
    k(0x4A, "o", 0x18, 38, 8, 4, 4),
    k(0x52, "p", 0x19, 42, 8, 4, 4),
    k(0x5A, "lbracket", 0x1A, 46, 8, 4, 4),
    k(0x62, "rbracket", 0x1B, 50, 8, 4, 4),
    // 0x6A : antislash des claviers ANSI ; absent en ISO, où Entrée (0x6B) occupe deux rangées.
    k(0x6A, "backslash", 0, 54, 8, 0, 0),
    k(0x6B, "enter", 0, 55, 8, 5, 8),
    k(0x7A, "num7", 0, 60, 8, 4, 4),
    k(0x82, "num8", 0, 64, 8, 4, 4),
    k(0x8A, "num9", 0, 68, 8, 4, 4),
    k(0x92, "numplus", 0, 72, 8, 4, 8),
    // Rangée de repos
    k(0x03, "capslock", 0, 0, 12, 7, 4),
    k(0x0B, "a", 0x1E, 7, 12, 4, 4),
    k(0x13, "s", 0x1F, 11, 12, 4, 4),
    k(0x1B, "d", 0x20, 15, 12, 4, 4),
    k(0x23, "f", 0x21, 19, 12, 4, 4),
    k(0x2B, "g", 0x22, 23, 12, 4, 4),
    k(0x33, "h", 0x23, 27, 12, 4, 4),
    k(0x3B, "j", 0x24, 31, 12, 4, 4),
    k(0x43, "k", 0x25, 35, 12, 4, 4),
    k(0x4B, "l", 0x26, 39, 12, 4, 4),
    k(0x53, "semicolon", 0x27, 43, 12, 4, 4),
    k(0x5B, "quote", 0x28, 47, 12, 4, 4),
    k(0x63, "hash", 0x2B, 51, 12, 4, 4),
    k(0x7B, "num4", 0, 60, 12, 4, 4),
    k(0x83, "num5", 0, 64, 12, 4, 4),
    k(0x8B, "num6", 0, 68, 12, 4, 4),
    // Rangée du bas des lettres
    k(0x04, "lshift", 0, 0, 16, 5, 4),
    k(0x0C, "iso", 0x56, 5, 16, 4, 4),
    k(0x14, "z", 0x2C, 9, 16, 4, 4),
    k(0x1C, "x", 0x2D, 13, 16, 4, 4),
    k(0x24, "c", 0x2E, 17, 16, 4, 4),
    k(0x2C, "v", 0x2F, 21, 16, 4, 4),
    k(0x34, "b", 0x30, 25, 16, 4, 4),
    k(0x3C, "n", 0x31, 29, 16, 4, 4),
    k(0x44, "m", 0x32, 33, 16, 4, 4),
    k(0x4C, "comma", 0x33, 37, 16, 4, 4),
    k(0x54, "period", 0x34, 41, 16, 4, 4),
    k(0x5C, "slash", 0x35, 45, 16, 4, 4),
    k(0x64, "rshift", 0, 49, 16, 7, 4),
    k(0x74, "up", 0, 56, 16, 4, 4),
    k(0x7C, "num1", 0, 60, 16, 4, 4),
    k(0x84, "num2", 0, 64, 16, 4, 4),
    k(0x8C, "num3", 0, 68, 16, 4, 4),
    k(0x94, "numenter", 0, 72, 16, 4, 8),
    // Rangée du bas ; la barre d'espace porte trois LED.
    k(0x05, "lctrl", 0, 0, 20, 5, 4),
    k(0x0D, "lwin", 0, 5, 20, 5, 4),
    k(0x15, "lalt", 0, 10, 20, 5, 4),
    k(0x2D, "space1", 0, 15, 20, 8, 4),
    k(0x35, "space2", 0, 23, 20, 9, 4),
    k(0x3D, "space3", 0, 32, 20, 8, 4),
    k(0x55, "ralt", 0, 40, 20, 4, 4),
    k(0x5D, "fn", 0, 44, 20, 4, 4),
    k(0x65, "rctrl", 0, 48, 20, 4, 4),
    k(0x6D, "left", 0, 52, 20, 4, 4),
    k(0x75, "down", 0, 56, 20, 4, 4),
    k(0x7D, "right", 0, 60, 20, 4, 4),
    k(0x85, "num0", 0, 64, 20, 4, 4),
    k(0x8D, "numperiod", 0, 68, 20, 4, 4),
];

/// Groupes nommés, utilisables partout où un nom de touche est attendu.
pub static GROUPS: &[(&str, &[&str])] = &[
    ("space", &["space1", "space2", "space3"]),
    ("wasd", &["w", "a", "s", "d"]),
    ("zqsd", &["w", "a", "s", "d"]), // mêmes positions physiques sur un AZERTY
    ("arrows", &["up", "down", "left", "right"]),
    (
        "frow",
        &["f1", "f2", "f3", "f4", "f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12"],
    ),
    ("digits", &["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"]),
    (
        "numpad",
        &[
            "numlock", "numdivide", "nummultiply", "numminus", "numplus", "numenter", "numperiod",
            "num0", "num1", "num2", "num3", "num4", "num5", "num6", "num7", "num8", "num9",
        ],
    ),
    (
        "mods",
        &[
            "lshift", "rshift", "lctrl", "rctrl", "lalt", "ralt", "lwin", "fn", "tab", "capslock",
            "backspace", "enter", "esc",
        ],
    ),
];

/// Lettre gravée sur chaque touche à scancode, selon la disposition clavier active.
/// Renvoie des couples (scancode, caractère en minuscule).
#[cfg(windows)]
pub fn layout_legends() -> Vec<(u8, char)> {
    #[link(name = "user32")]
    extern "system" {
        fn GetKeyboardLayout(thread: u32) -> *mut std::ffi::c_void;
        fn MapVirtualKeyExW(code: u32, map_type: u32, layout: *mut std::ffi::c_void) -> u32;
    }
    const MAPVK_VSC_TO_VK: u32 = 1;
    const MAPVK_VK_TO_CHAR: u32 = 2;
    let mut out = Vec::new();
    unsafe {
        let layout = GetKeyboardLayout(0);
        for key in KEYS.iter().filter(|k| k.scan != 0 && !k.name.starts_with('f')) {
            let vk = MapVirtualKeyExW(key.scan as u32, MAPVK_VSC_TO_VK, layout);
            // Bit 31 : touche morte (^, ¨…) ; on garde le caractère de base.
            let ch = MapVirtualKeyExW(vk, MAPVK_VK_TO_CHAR, layout) & 0x7FFF_FFFF;
            if let Some(c) = char::from_u32(ch).filter(|c| !c.is_control() && *c != ' ') {
                out.push((key.scan, c.to_lowercase().next().unwrap_or(c)));
            }
        }
    }
    out
}

#[cfg(not(windows))]
pub fn layout_legends() -> Vec<(u8, char)> {
    Vec::new()
}

/// Résout un nom en identifiants de LED. Accepte, dans l'ordre : `all`, un groupe, un nom
/// positionnel, un identifiant hexadécimal (`0x2d`), ou la lettre gravée sur la touche
/// dans la disposition active (`legends`, cf. [`layout_legends`]).
pub fn resolve(name: &str, legends: &[(u8, char)]) -> Option<Vec<u8>> {
    let n = name.trim().to_lowercase();
    if n == "all" || n == "*" {
        return Some(KEYS.iter().map(|k| k.id).collect());
    }
    if let Some((_, members)) = GROUPS.iter().find(|(g, _)| *g == n) {
        return Some(
            members
                .iter()
                .filter_map(|m| KEYS.iter().find(|k| k.name == *m).map(|k| k.id))
                .collect(),
        );
    }
    if let Some(hex) = n.strip_prefix("0x") {
        return u8::from_str_radix(hex, 16).ok().map(|id| vec![id]);
    }
    // Une lettre seule désigne d'abord la touche qui la porte réellement (AZERTY, etc.).
    let mut chars = n.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        if let Some((scan, _)) = legends.iter().find(|(_, l)| *l == c) {
            if let Some(key) = KEYS.iter().find(|k| k.scan == *scan) {
                return Some(vec![key.id]);
            }
        }
    }
    KEYS.iter().find(|k| k.name == n).map(|k| vec![k.id])
}
