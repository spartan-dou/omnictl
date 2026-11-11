//! Commandes du clavier (protocole ASUS des Scope II), au-dessus du transport.
//!
//! Sources, recoupées entre elles puis vérifiées sur le matériel :
//! - OpenRGB (`AsusAuraKeyboardController.cpp`) ;
//! - le pilote ASUS `AacKbHal_x64.dll` (classes `AacX901*`), lu par décompilation ;
//! - les modules d'Armoury Crate pour le sens des réglages.
//!
//! Voir `PROTOCOL.md` pour le détail octet par octet.

use crate::link::{Error, Link, Result, Via};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub const BLACK: Rgb = Rgb(0, 0, 0);

    /// Accepte `#rrggbb`, `rrggbb`, `#rgb` ou un nom courant (français ou anglais).
    pub fn parse(s: &str) -> Option<Rgb> {
        let s = s.trim().to_lowercase();
        let named = match s.as_str() {
            "red" | "rouge" => Some(Rgb(255, 0, 0)),
            "green" | "vert" => Some(Rgb(0, 255, 0)),
            "blue" | "bleu" => Some(Rgb(0, 0, 255)),
            "white" | "blanc" => Some(Rgb(255, 255, 255)),
            "black" | "noir" | "off" => Some(Rgb(0, 0, 0)),
            "yellow" | "jaune" => Some(Rgb(255, 255, 0)),
            "cyan" => Some(Rgb(0, 255, 255)),
            "magenta" => Some(Rgb(255, 0, 255)),
            "orange" => Some(Rgb(255, 96, 0)),
            "purple" | "violet" => Some(Rgb(128, 0, 255)),
            "pink" | "rose" => Some(Rgb(255, 64, 160)),
            _ => None,
        };
        if named.is_some() {
            return named;
        }
        let h = s.strip_prefix('#').unwrap_or(&s);
        if !h.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        match h.len() {
            6 => {
                let v = u32::from_str_radix(h, 16).ok()?;
                Some(Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
            }
            3 => {
                let v = u16::from_str_radix(h, 16).ok()?;
                let d = |n: u16| ((n & 0xF) * 17) as u8;
                Some(Rgb(d(v >> 8), d(v >> 4), d(v)))
            }
            _ => None,
        }
    }

    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Static = 0,
    Breathing = 1,
    ColorCycle = 2,
    Reactive = 3,
    Wave = 4,
    Ripple = 5,
    Starry = 6,
    Quicksand = 7,
    Current = 8,
    RainDrop = 9,
}

// Les champs has_* décrivent les options de chaque effet pour l'interface graphique, à venir.
#[allow(dead_code)]
pub struct ModeInfo {
    pub mode: Mode,
    pub name: &'static str,
    pub label: &'static str,
    /// Nombre de couleurs acceptées (min, max). (0, 0) = aucune.
    pub colors: (usize, usize),
    pub has_speed: bool,
    pub has_direction: bool,
    pub has_random: bool,
    /// Octet de vitesse pour rapide / moyen / lent (table du pilote ASUS).
    speeds: [u8; 3],
}

const fn info(
    mode: Mode,
    name: &'static str,
    label: &'static str,
    colors: (usize, usize),
    flags: (bool, bool, bool),
    speeds: [u8; 3],
) -> ModeInfo {
    ModeInfo {
        mode,
        name,
        label,
        colors,
        has_speed: flags.0,
        has_direction: flags.1,
        has_random: flags.2,
        speeds,
    }
}

pub static MODES: &[ModeInfo] = &[
    info(Mode::Static, "static", "Statique", (1, 1), (false, false, false), [0xFF; 3]),
    info(Mode::Breathing, "breathing", "Respiration", (1, 2), (true, false, true), [0x0F, 0x1E, 0x2A]),
    info(Mode::ColorCycle, "cycle", "Cycle de couleurs", (0, 0), (true, false, false), [0x12, 0x1F, 0x3E]),
    info(Mode::Reactive, "reactive", "Réactif (à la frappe)", (1, 2), (true, false, true), [0x08, 0x0F, 0x16]),
    info(Mode::Wave, "wave", "Vague arc-en-ciel", (0, 7), (true, true, false), [0x23, 0x3E, 0x55]),
    info(Mode::Ripple, "ripple", "Ondulation (à la frappe)", (0, 7), (true, false, true), [0x50, 0x64, 0x78]),
    info(Mode::Starry, "starry", "Nuit étoilée", (1, 3), (true, false, true), [0x0A, 0x0F, 0x19]),
    info(Mode::Quicksand, "quicksand", "Sables mouvants", (0, 6), (true, true, false), [0x0C, 0x0F, 0x14]),
    info(Mode::Current, "current", "Courant", (1, 3), (true, false, true), [0x07, 0x0A, 0x0C]),
    info(Mode::RainDrop, "raindrop", "Gouttes de pluie", (1, 3), (true, false, true), [0x0E, 0x14, 0x1B]),
];

impl Mode {
    pub fn info(self) -> &'static ModeInfo {
        MODES.iter().find(|m| m.mode == self).expect("mode listé dans MODES")
    }

    pub fn parse(s: &str) -> Option<Mode> {
        let s = s.to_lowercase();
        MODES.iter().find(|m| m.name == s).map(|m| m.mode)
    }

    pub fn from_code(code: u8) -> Option<Mode> {
        MODES.iter().find(|m| m.mode as u8 == code).map(|m| m.mode)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Speed {
    Fast,
    Medium,
    Slow,
    /// Octet brut : plus la valeur est petite, plus l'effet est rapide.
    Raw(u8),
}

impl Speed {
    pub fn parse(s: &str) -> Option<Speed> {
        Some(match s.to_lowercase().as_str() {
            "fast" | "rapide" => Speed::Fast,
            "medium" | "moyen" => Speed::Medium,
            "slow" | "lent" => Speed::Slow,
            n => Speed::Raw(n.parse().ok()?),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
    Horizontal,
    Vertical,
}

impl Direction {
    pub fn parse(s: &str) -> Option<Direction> {
        Some(match s.to_lowercase().as_str() {
            "left" | "gauche" => Direction::Left,
            "right" | "droite" => Direction::Right,
            "up" | "haut" => Direction::Up,
            "down" | "bas" => Direction::Down,
            "horizontal" => Direction::Horizontal,
            "vertical" => Direction::Vertical,
            _ => return None,
        })
    }

    fn code(self) -> u8 {
        match self {
            Direction::Left => 4,
            Direction::Right => 0,
            Direction::Up => 6,
            Direction::Down => 2,
            Direction::Horizontal => 8,
            Direction::Vertical => 1,
        }
    }
}

/// Luminosité : pourcentage, 0 à 100.
pub const BRIGHTNESS_MAX: u8 = 100;

#[derive(Clone, Debug)]
pub struct Effect {
    pub mode: Mode,
    pub colors: Vec<Rgb>,
    pub speed: Speed,
    /// 0 à 100.
    pub brightness: u8,
    pub random: bool,
    pub direction: Direction,
}

impl Effect {
    pub fn new(mode: Mode) -> Effect {
        Effect {
            mode,
            colors: Vec::new(),
            speed: Speed::Medium,
            brightness: BRIGHTNESS_MAX,
            random: false,
            direction: Direction::Left,
        }
    }
}

const RAINBOW: [Rgb; 7] = [
    Rgb(255, 0, 0),
    Rgb(255, 127, 0),
    Rgb(255, 255, 0),
    Rgb(0, 255, 0),
    Rgb(0, 0, 255),
    Rgb(75, 0, 130),
    Rgb(148, 0, 211),
];

/// Délais de mise en veille proposés par le clavier, dans l'ordre de leur index.
pub const SLEEP_CHOICES: [&str; 6] = ["1 min", "2 min", "3 min", "5 min", "10 min", "jamais"];
/// Modes d'économie d'énergie, dans l'ordre de leur valeur.
pub const SAVING_CHOICES: [&str; 3] = ["désactivée", "éteindre l'éclairage", "réduire la luminosité"];
/// Fonctions de la molette, dans l'ordre de leur index.
pub const LEVER_CHOICES: [&str; 4] = ["volume", "piste multimédia", "luminosité du clavier", "personnalisé"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Connection {
    Wired,
    Wireless,
    Bluetooth,
    Unreachable,
    Other(u8),
}

impl Connection {
    pub fn label(self) -> String {
        match self {
            Connection::Wired => "filaire".into(),
            Connection::Wireless => "2,4 GHz".into(),
            Connection::Bluetooth => "Bluetooth".into(),
            Connection::Unreachable => {
                "clavier injoignable (en veille ou éteint : appuyez sur une touche)".into()
            }
            Connection::Other(n) => format!("inconnu ({n})"),
        }
    }
}

pub struct Status {
    pub connection: Connection,
    pub firmware: String,
    /// Version du récepteur, en liaison 2,4 GHz uniquement.
    pub receiver_firmware: Option<String>,
    pub profile: u8,
    pub effect: u8,
}

pub struct Power {
    pub percent: u8,
    pub charging: bool,
    pub millivolts: u16,
    /// Index dans [`SLEEP_CHOICES`].
    pub sleep: u8,
    /// Index dans [`SAVING_CHOICES`].
    pub saving: u8,
    /// Seuil d'alerte batterie faible, en pourcentage.
    pub low_alert: u8,
}

pub struct Keyboard {
    pub link: Link,
}

impl Keyboard {
    pub fn new(link: Link) -> Keyboard {
        Keyboard { link }
    }

    /// Lit un état sur un octet (`12 xx` → octet 4 de la réponse).
    fn get(&mut self, sub: u8) -> Result<u8> {
        Ok(self.link.request(&[0x12, sub])?[4])
    }

    /// Écrit un réglage sur un octet (`51 xx 00 00 valeur`).
    fn set(&mut self, sub: u8, value: u8) -> Result<()> {
        self.link.request(&[0x51, sub, 0, 0, value]).map(|_| ())
    }

    pub fn status(&mut self) -> Result<Status> {
        let connection = match self.get(0x03)? {
            0 => Connection::Wired,
            1 => Connection::Wireless,
            2 => Connection::Bluetooth,
            0xFF => Connection::Unreachable,
            n => Connection::Other(n),
        };
        let r = self.link.request(&[0x12, 0x00])?;
        let version = |i: usize| format!("{:X}.{:02X}.{:02X}", r[i + 2], r[i + 1], r[i]);
        // Via le dongle, la réponse porte deux versions : récepteur d'abord, clavier ensuite.
        // Clavier en veille ou éteint : le dongle répond seul et la seconde est absente.
        let (firmware, receiver_firmware) = if self.link.via == Via::Dongle {
            let keyboard = match connection {
                Connection::Unreachable => "inconnu (clavier injoignable)".to_string(),
                _ => version(11),
            };
            (keyboard, Some(version(4)))
        } else {
            (version(4), None)
        };
        Ok(Status {
            connection,
            firmware,
            receiver_firmware,
            effect: r[9],
            profile: r[10],
        })
    }

    pub fn power(&mut self) -> Result<Power> {
        let r = self.link.request(&[0x12, 0x01])?;
        Ok(Power {
            percent: r[5],
            // « Jamais » s'écrit avec l'index 5 mais se relit `FF` (mesuré).
            sleep: if r[6] == 0xFF { 5 } else { r[6] },
            saving: r[7],
            charging: r[8] == 1,
            low_alert: r[9],
            millivolts: u16::from_le_bytes([r[11], r[12]]),
        })
    }

    pub fn set_sleep(&mut self, index: u8) -> Result<()> {
        if index as usize >= SLEEP_CHOICES.len() {
            return Err(Error::Msg("veille : index 0 à 5".into()));
        }
        self.set(0x38, index)
    }

    pub fn set_saving(&mut self, mode: u8) -> Result<()> {
        if mode as usize >= SAVING_CHOICES.len() {
            return Err(Error::Msg("économie d'énergie : 0 à 2".into()));
        }
        self.set(0x39, mode)
    }

    pub fn set_low_alert(&mut self, percent: u8) -> Result<()> {
        if percent > 100 {
            return Err(Error::Msg("alerte batterie : 0 à 100 %".into()));
        }
        self.set(0x3A, percent)
    }

    /// Bascule sur un profil embarqué (0 = profil par défaut).
    pub fn set_profile(&mut self, n: u8) -> Result<()> {
        self.set(0x00, n)
    }

    /// Fonction active de la molette (index dans [`LEVER_CHOICES`]).
    pub fn lever(&mut self) -> Result<u8> {
        Ok(self.link.request(&[0x22, 0x01])?[4])
    }

    /// Éclairage dynamique Windows (LampArray) : le clavier se laisse-t-il piloter par Windows ?
    pub fn wdl(&mut self) -> Result<bool> {
        Ok(self.get(0x07)? == 1)
    }

    pub fn set_wdl(&mut self, on: bool) -> Result<()> {
        self.link.request(&[0x73, 0, 0, 0, on as u8]).map(|_| ())
    }

    /// Applique un effet géré par le clavier lui-même : il tourne sans logiciel.
    pub fn set_effect(&mut self, e: &Effect) -> Result<()> {
        let info = e.mode.info();
        if e.brightness > BRIGHTNESS_MAX {
            return Err(Error::Msg("luminosité : 0 à 100".into()));
        }
        let (min, max) = info.colors;
        if e.colors.len() > max || (e.colors.len() < min && !e.random) {
            return Err(Error::Msg(match (min, max) {
                (0, 0) => format!("l'effet {} ne prend pas de couleur", info.name),
                (a, b) if a == b => format!("l'effet {} attend {a} couleur(s)", info.name),
                (a, b) => format!("l'effet {} attend {a} à {b} couleur(s)", info.name),
            }));
        }
        if e.random && !info.has_random {
            return Err(Error::Msg(format!("l'effet {} n'a pas de mode aléatoire", info.name)));
        }

        let mut p = [0u8; 63];
        p[0] = 0x51;
        p[1] = 0x2C;
        p[2] = e.mode as u8;
        p[4] = match e.speed {
            Speed::Fast => info.speeds[0],
            Speed::Medium => info.speeds[1],
            Speed::Slow => info.speeds[2],
            Speed::Raw(v) => v,
        };
        p[5] = e.brightness;
        p[7] = 0xFF;
        p[8] = 0xFF;

        match e.mode {
            Mode::Wave | Mode::Ripple => {
                let colors: &[Rgb] = if e.colors.is_empty() { &RAINBOW } else { &e.colors };
                p[6] = e.random as u8;
                if e.mode == Mode::Wave {
                    p[7] = e.direction.code();
                }
                let n = colors.len();
                let stop = |i: usize| (100.0 / n as f32 * (i + 1) as f32) as u8;
                // Dégradé sur trois paquets : couleurs 1-2, puis 3-6, puis 7.
                // L'octet 3 est le nombre de paquets restant à suivre.
                p[3] = 0x02;
                p[8] = 0x02;
                p[9] = n as u8;
                for (i, c) in colors.iter().take(2).enumerate() {
                    p[10 + i * 4..14 + i * 4].copy_from_slice(&[stop(i), c.0, c.1, c.2]);
                }
                self.link.request(&p)?;
                for (remaining, range) in [(1u8, 2..6usize), (0u8, 6..7usize)] {
                    p[3..].fill(0);
                    p[3] = remaining;
                    for (slot, i) in range.enumerate() {
                        if let Some(c) = colors.get(i) {
                            p[4 + slot * 4..8 + slot * 4].copy_from_slice(&[stop(i), c.0, c.1, c.2]);
                        }
                    }
                    self.link.request(&p)?;
                }
            }
            Mode::Quicksand => {
                let mut colors = e.colors.clone();
                if colors.is_empty() {
                    colors.extend_from_slice(&RAINBOW[..6]);
                }
                if colors.len() != 6 {
                    return Err(Error::Msg("l'effet quicksand attend 6 couleurs (ou aucune)".into()));
                }
                p[3] = 0x01;
                p[7] = match e.direction {
                    Direction::Up => Direction::Up.code(),
                    _ => Direction::Down.code(),
                };
                for (i, c) in colors[..3].iter().enumerate() {
                    p[9 + i * 3..12 + i * 3].copy_from_slice(&[c.0, c.1, c.2]);
                }
                self.link.request(&p)?;
                p[3..].fill(0);
                for (i, c) in colors[3..].iter().enumerate() {
                    p[4 + i * 3..7 + i * 3].copy_from_slice(&[c.0, c.1, c.2]);
                }
                self.link.request(&p)?;
            }
            _ => {
                // Quartet haut = 1 : une seconde couleur (fond) accompagne la première.
                let two = e.colors.len() >= 2 && e.colors[1] != Rgb::BLACK;
                p[6] = if e.random {
                    1
                } else if two {
                    0x10
                } else {
                    0
                };
                for (i, c) in e.colors.iter().enumerate() {
                    p[9 + i * 3..12 + i * 3].copy_from_slice(&[c.0, c.1, c.2]);
                }
                self.link.request(&p)?;
            }
        }
        Ok(())
    }

    /// Couleurs directes touche par touche. Volatiles : le clavier revient à son effet
    /// enregistré lorsqu'il est éteint, se met en veille ou reçoit un effet.
    pub fn set_direct(&mut self, leds: &[(u8, Rgb)]) -> Result<()> {
        let per_packet = (self.link.room() - 4) / 4;
        for chunk in leds.chunks(per_packet) {
            let mut p = vec![0xC0, 0x81, chunk.len() as u8, 0x00];
            for (id, c) in chunk {
                p.extend_from_slice(&[*id, c.0, c.1, c.2]);
            }
            // Accusé mesuré : `C0 82 n` (le second octet n'est pas repris tel quel).
            self.link.request_matching(&p, 1, Duration::from_millis(1000))?;
        }
        Ok(())
    }

    /// Enregistre l'effet courant dans le profil actif du clavier.
    pub fn save(&mut self) -> Result<()> {
        self.link.request(&[0x50, 0x55]).map(|_| ())
    }
}
