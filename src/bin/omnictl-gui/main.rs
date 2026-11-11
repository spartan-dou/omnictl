//! Interface graphique d'omnictl : une fenêtre Windows native, entièrement dessinée par le
//! programme (voir `win.rs`) dans l'esprit de Windows 11 — volet de navigation, cartes de
//! réglages, couleur d'accentuation du système, thème clair ou sombre.
//!
//! Architecture : le fil de l'interface ne parle jamais aux périphériques. Il envoie des
//! commandes à un fil de travail, qui les regroupe (un glissement de curseur produit des
//! dizaines de commandes dont seule la dernière compte), les exécute, relit l'état et
//! prévient la fenêtre. Un périphérique en veille ou débranché ne fige donc jamais l'affichage.

#![windows_subsystem = "windows"]

mod win;

use omnictl::keys;
use omnictl::link::{Error, Link, Via};
use omnictl::mouse::{Mouse, MouseLight, MouseMode};
use omnictl::proto::{self, Connection, Direction, Effect, Keyboard, Mode, Rgb, Speed};
use omnictl::store;
use std::cell::RefCell;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use win::{rc, Align, Canvas, Rc, HWND};

/// Taille logique du contenu ; la fenêtre le met à l'échelle.
const W: f32 = 1100.0;
const H: f32 = 740.0;
/// Largeur du volet de navigation.
const NAV: f32 = 232.0;
/// Bord gauche et largeur de la zone de contenu.
const CX: f32 = NAV + 28.0;
const CW: f32 = W - CX - 28.0;
/// Hauteur d'une carte de réglage et pas vertical entre deux cartes.
const ROW: f32 = 52.0;
const STEP: f32 = 56.0;

const WM_UPDATE: u32 = win::WM_APP + 1;
/// Clic sur l'icône de la zone de notification.
const WM_TRAY: u32 = win::WM_APP + 2;
const REFRESH_MS: u32 = 20_000;
/// Intervalle des rondes du gardien.
const GUARD_POLL: Duration = Duration::from_secs(3);
const TRAY_TIP: &str = "omnictl — gardien des couleurs du clavier";

const PRESETS: [Rgb; 10] = [
    Rgb(255, 0, 0),
    Rgb(255, 96, 0),
    Rgb(255, 208, 0),
    Rgb(0, 255, 64),
    Rgb(0, 224, 255),
    Rgb(0, 64, 255),
    Rgb(128, 0, 255),
    Rgb(255, 64, 160),
    Rgb(255, 255, 255),
    Rgb(0, 0, 0),
];

/// Couleurs de l'interface, calquées sur celles de Windows 11.
struct Theme {
    /// Fond de la fenêtre (derrière le volet de navigation).
    bg: u32,
    /// Fond de la zone de contenu.
    layer: u32,
    card: u32,
    card_border: u32,
    control: u32,
    control_border: u32,
    hover: u32,
    /// Élément de navigation sélectionné.
    subtle: u32,
    text: u32,
    dim: u32,
    /// Contour marqué (interrupteur éteint).
    strong: u32,
    accent: u32,
    on_accent: u32,
    danger: u32,
}

fn theme(dark: bool) -> Theme {
    let (r, g, b) = win::accent();
    let accent = Rgb(r, g, b);
    if dark {
        Theme {
            bg: 0xFF202020,
            layer: 0xFF272727,
            card: 0xFF2D2D2D,
            card_border: 0xFF1F1F1F,
            control: 0xFF373737,
            control_border: 0xFF484848,
            hover: 0xFF3F3F3F,
            subtle: 0xFF2F2F2F,
            text: 0xFFFFFFFF,
            dim: 0xFFC5C5C5,
            strong: 0xFF9E9E9E,
            // Sur fond sombre, Windows éclaircit la couleur d'accentuation et écrit en noir dessus.
            accent: argb(mix(accent, Rgb(255, 255, 255), 0.45)),
            on_accent: 0xFF000000,
            danger: 0xFFFF99A4,
        }
    } else {
        Theme {
            bg: 0xFFF3F3F3,
            layer: 0xFFF9F9F9,
            card: 0xFFFFFFFF,
            card_border: 0xFFE5E5E5,
            control: 0xFFFDFDFD,
            control_border: 0xFFD4D4D4,
            hover: 0xFFF2F2F2,
            subtle: 0xFFE9E9E9,
            text: 0xFF1B1B1B,
            dim: 0xFF5F5F5F,
            strong: 0xFF8A8A8A,
            accent: argb(accent),
            on_accent: 0xFFFFFFFF,
            danger: 0xFFC42B1C,
        }
    }
}

fn argb(c: Rgb) -> u32 {
    0xFF00_0000 | (c.0 as u32) << 16 | (c.1 as u32) << 8 | c.2 as u32
}

/// Mélange linéaire de deux couleurs ; `t` = part de `b`.
fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Rgb(f(a.0, b.0), f(a.1, b.1), f(a.2, b.2))
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> Rgb {
    let h = h.rem_euclid(360.0) / 60.0;
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    let to = |f: f32| ((f + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    Rgb(to(r), to(g), to(b))
}

fn rgb_to_hsv(c: Rgb) -> (f32, f32, f32) {
    let (r, g, b) = (c.0 as f32 / 255.0, c.1 as f32 / 255.0, c.2 as f32 / 255.0);
    let max = r.max(g).max(b);
    let d = max - r.min(g).min(b);
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h, if max == 0.0 { 0.0 } else { d / max }, max)
}

/// Ce que la souris peut reproduire d'un effet du clavier : elle n'a qu'une LED et trois
/// effets colorés. Les effets sans équivalent deviennent une couleur fixe.
fn mouse_from_effect(fx: &Effect) -> MouseLight {
    let mode = match fx.mode {
        Mode::Breathing => MouseMode::Breathing,
        Mode::ColorCycle => MouseMode::ColorCycle,
        // Dégradés sans couleur choisie : l'arc-en-ciel se traduit par un cycle.
        Mode::Wave | Mode::Ripple | Mode::Quicksand if fx.colors.is_empty() => MouseMode::ColorCycle,
        _ => MouseMode::Static,
    };
    MouseLight {
        mode,
        color: fx.colors.first().copied().unwrap_or(Rgb(255, 255, 255)),
        level: ((fx.brightness as f32 / 25.0).round() as u8).min(4),
    }
}

// ───────────────────────────── Fil de travail ─────────────────────────────

#[derive(Clone)]
enum Cmd {
    Refresh,
    Effect(Effect),
    SaveEffect(Effect),
    Keys(Vec<(u8, Rgb)>),
    ApplyKeys,
    Profile(u8),
    Sleep(u8),
    Saving(u8),
    Low(u8),
    Wdl(bool),
    /// Active ou coupe le gardien dans le fil de travail.
    Guard(bool),
    MouseLight(MouseLight),
    MouseSave(MouseLight),
}

/// Ce que l'interface affiche du clavier.
#[derive(Clone)]
struct Snapshot {
    via: &'static str,
    firmware: String,
    profile: u8,
    effect: u8,
    percent: u8,
    charging: bool,
    sleep: u8,
    saving: u8,
    low: u8,
    wdl: bool,
    lever: u8,
}

/// Ce que l'interface affiche de la souris.
#[derive(Clone)]
struct MouseSnap {
    firmware: String,
    percent: u8,
    mode: u8,
    level: u8,
}

#[derive(Default)]
struct Shared {
    snap: Option<Snapshot>,
    mouse: Option<MouseSnap>,
    message: String,
    is_error: bool,
}

/// Fusionne les commandes consécutives de même nature : seule la dernière valeur compte.
fn coalesce(batch: Vec<Cmd>) -> Vec<Cmd> {
    let mut out: Vec<Cmd> = Vec::new();
    for cmd in batch {
        let merged = match (out.last_mut(), &cmd) {
            (Some(Cmd::Effect(prev)), Cmd::Effect(new)) => {
                *prev = new.clone();
                true
            }
            (Some(Cmd::MouseLight(prev)), Cmd::MouseLight(new)) => {
                *prev = *new;
                true
            }
            (Some(Cmd::Keys(prev)), Cmd::Keys(new)) => {
                for &(id, c) in new {
                    match prev.iter_mut().find(|(i, _)| *i == id) {
                        Some(slot) => slot.1 = c,
                        None => prev.push((id, c)),
                    }
                }
                true
            }
            // L'état est relu après chaque lot : une demande de plus n'apporte rien.
            (Some(_), Cmd::Refresh) => true,
            _ => false,
        };
        if !merged {
            out.push(cmd);
        }
    }
    out
}

/// Les deux périphériques, ouverts à la demande et rouverts après une erreur.
#[derive(Default)]
struct Devices {
    kb: Option<Keyboard>,
    mouse: Option<Mouse>,
}

impl Devices {
    fn kb(&mut self) -> Result<&mut Keyboard, Error> {
        if self.kb.is_none() {
            self.kb = Some(Keyboard::new(Link::open(None)?));
        }
        Ok(self.kb.as_mut().expect("rempli juste au-dessus"))
    }

    fn mouse(&mut self) -> Result<&mut Mouse, Error> {
        if self.mouse.is_none() {
            self.mouse = Some(Mouse::open()?);
        }
        Ok(self.mouse.as_mut().expect("rempli juste au-dessus"))
    }

    /// Le clavier répond-il ? (Via le dongle, un clavier en veille est « injoignable ».)
    fn kb_reachable(&mut self) -> bool {
        let ok = self
            .kb()
            .and_then(|kb| kb.status())
            .is_ok_and(|st| st.connection != Connection::Unreachable);
        if !ok {
            self.kb = None;
        }
        ok
    }

    /// Exécute une commande. Renvoie le message à afficher, s'il y en a un.
    fn execute(&mut self, cmd: &Cmd) -> Result<Option<&'static str>, Error> {
        Ok(match cmd {
            Cmd::Refresh | Cmd::Guard(_) => None, // rien à envoyer
            Cmd::Effect(e) => {
                self.kb()?.set_effect(e)?;
                None // appliqué en direct : pas de message à chaque retouche
            }
            Cmd::SaveEffect(e) => {
                let kb = self.kb()?;
                kb.set_effect(e)?;
                kb.save()?;
                Some("Effet enregistré dans le clavier.")
            }
            Cmd::Keys(leds) => {
                self.kb()?.set_direct(leds)?;
                store::save(&store::merge(leds))
                    .map_err(|e| Error::Msg(format!("Couleurs appliquées mais non mémorisées : {e}")))?;
                None
            }
            Cmd::ApplyKeys => {
                let leds = store::load();
                if leds.is_empty() {
                    return Err(Error::Msg("Aucune couleur touche par touche mémorisée.".into()));
                }
                self.kb()?.set_direct(&leds)?;
                Some("Couleurs touche par touche remises en place.")
            }
            Cmd::Profile(n) => {
                self.kb()?.set_profile(*n)?;
                Some("Profil changé.")
            }
            Cmd::Sleep(i) => {
                self.kb()?.set_sleep(*i)?;
                Some("Délai de veille réglé.")
            }
            Cmd::Saving(m) => {
                self.kb()?.set_saving(*m)?;
                Some("Économie d'énergie réglée.")
            }
            Cmd::Low(p) => {
                self.kb()?.set_low_alert(*p)?;
                Some("Alerte batterie réglée.")
            }
            Cmd::Wdl(on) => {
                self.kb()?.set_wdl(*on)?;
                Some(if *on {
                    "Éclairage dynamique Windows activé."
                } else {
                    "Éclairage dynamique Windows désactivé."
                })
            }
            Cmd::MouseLight(light) => {
                self.mouse()?.set_light(light)?;
                None
            }
            Cmd::MouseSave(light) => {
                let mouse = self.mouse()?;
                mouse.set_light(light)?;
                mouse.save()?;
                Some("Éclairage enregistré dans la souris.")
            }
        })
    }

    fn snapshot(&mut self) -> Result<Snapshot, Error> {
        let kb = self.kb()?;
        let st = kb.status()?;
        if st.connection == Connection::Unreachable {
            return Err(Error::Msg(
                "Clavier en veille ou éteint : appuyez sur une touche pour le réveiller.".into(),
            ));
        }
        let pw = kb.power()?;
        Ok(Snapshot {
            via: match kb.link.via {
                Via::Usb => "USB",
                Via::Dongle => "2,4 GHz",
            },
            firmware: st.firmware,
            profile: st.profile,
            effect: st.effect,
            percent: pw.percent,
            charging: pw.charging,
            sleep: pw.sleep,
            saving: pw.saving,
            low: pw.low_alert,
            wdl: kb.wdl()?,
            lever: kb.lever()?,
        })
    }

    /// État de la souris, ou `None` si elle ne répond pas (éteinte, en veille, non appairée).
    fn mouse_snapshot(&mut self) -> Option<MouseSnap> {
        match self.mouse().and_then(|m| m.status()) {
            Ok(st) => Some(MouseSnap { firmware: st.firmware, percent: st.percent, mode: st.mode, level: st.level }),
            Err(_) => {
                self.mouse = None;
                None
            }
        }
    }
}

fn worker(rx: Receiver<Cmd>, shared: Arc<Mutex<Shared>>, hwnd: usize, mut guard: bool) {
    let mut dev = Devices::default();
    // Gardien : le clavier était-il joignable au dernier contrôle ? Faux au départ, pour que
    // les couleurs soient posées dès le premier contact.
    let mut was_reachable = false;
    loop {
        // Sans gardien, on dort jusqu'à la prochaine commande. Avec, on se réveille toutes
        // les quelques secondes pour voir si le clavier vient de revenir.
        let first = if guard {
            match rx.recv_timeout(GUARD_POLL) {
                Ok(cmd) => Some(cmd),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        } else {
            match rx.recv() {
                Ok(cmd) => Some(cmd),
                Err(_) => return,
            }
        };
        let Some(first) = first else {
            // Simple ronde du gardien : rien à dire à l'interface si rien n'a changé.
            let now = dev.kb_reachable();
            if now && !was_reachable {
                let leds = store::load();
                let restored = !leds.is_empty() && dev.kb().and_then(|k| k.set_direct(&leds)).is_ok();
                let snap = dev.snapshot();
                if let Ok(mut s) = shared.lock() {
                    if restored {
                        (s.message, s.is_error) = ("Couleurs touche par touche remises en place.".into(), false);
                    }
                    s.snap = snap.ok();
                }
                win::post(hwnd, WM_UPDATE);
            }
            was_reachable = now;
            continue;
        };
        let mut batch = vec![first];
        while let Ok(more) = rx.try_recv() {
            batch.push(more);
        }
        let mut message: Option<(String, bool)> = None;
        for cmd in coalesce(batch) {
            if let Cmd::Guard(on) = cmd {
                guard = on;
                was_reachable = false; // à l'activation, poser les couleurs à la prochaine ronde
                continue;
            }
            match dev.execute(&cmd) {
                Ok(Some(text)) => message = Some((text.to_string(), false)),
                Ok(None) => {}
                Err(e) => {
                    message = Some((e.to_string(), true));
                    // Rouvrir au prochain essai : le périphérique a pu changer de mode.
                    if matches!(cmd, Cmd::MouseLight(_) | Cmd::MouseSave(_)) {
                        dev.mouse = None;
                    } else {
                        dev.kb = None;
                    }
                }
            }
        }
        let snap = dev.snapshot();
        if snap.is_err() {
            dev.kb = None;
            was_reachable = false; // le gardien agira au retour du clavier
        }
        let mouse = dev.mouse_snapshot();
        if let Ok(mut s) = shared.lock() {
            match (&snap, message) {
                (_, Some((text, is_error))) => (s.message, s.is_error) = (text, is_error),
                (Err(e), None) => (s.message, s.is_error) = (e.to_string(), true),
                // Tout va bien de nouveau : effacer une ancienne erreur.
                (Ok(_), None) if s.is_error => (s.message, s.is_error) = (String::new(), false),
                (Ok(_), None) => {}
            }
            s.snap = snap.ok();
            s.mouse = mouse;
        }
        win::post(hwnd, WM_UPDATE);
    }
}

// ───────────────────────────── Interface ─────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum Device {
    Keyboard,
    Mouse,
}

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Light,
    Keys,
    Settings,
}

/// Zone cliquable, reconstruite à chaque dessin.
#[derive(Clone, Copy, PartialEq)]
enum Hit {
    Device(Device),
    Tab(Tab),
    Mode(Mode),
    Color(usize),
    AddColor,
    DelColor,
    Random,
    Speed(Speed),
    Dir(Direction),
    Brightness,
    SaveEffect,
    Preset(usize),
    CustomBrush,
    Key(u8),
    Fill,
    Clear,
    Reapply,
    Guard,
    /// Couleur du châssis dans la vue : `true` = clavier blanc.
    Chassis(bool),
    Profile(u8),
    Sleep(u8),
    Saving(u8),
    Low,
    Wdl,
    MouseMode(MouseMode),
    MouseColor,
    MouseLevel(u8),
    MouseSave,
    Sync,
    PickSv,
    PickHue,
    PickPreset(usize),
    PickOk,
    PickCancel,
}

/// Ce que le sélecteur de couleur est en train de modifier.
#[derive(Clone, Copy)]
enum PickTarget {
    EffectColor(usize),
    Brush,
    MouseColor,
}

/// Sélecteur de couleur intégré (teinte / saturation / luminosité), affiché par-dessus la page.
struct Picker {
    target: PickTarget,
    /// Couleur à rétablir si l'utilisateur annule.
    original: Rgb,
    /// Teinte en degrés (0 à 360), saturation et valeur entre 0 et 1.
    h: f32,
    s: f32,
    v: f32,
}

impl Picker {
    fn new(target: PickTarget, color: Rgb) -> Picker {
        let (h, s, v) = rgb_to_hsv(color);
        Picker { target, original: color, h, s, v }
    }

    fn color(&self) -> Rgb {
        hsv_to_rgb(self.h, self.s, self.v)
    }
}

// Glyphes de la police d'icônes de Windows.
const ICON_KEYBOARD: char = '\u{E765}';
const ICON_MOUSE: char = '\u{E962}';
const ICON_COLOR: char = '\u{E790}';
const ICON_SHUFFLE: char = '\u{E8B1}';
const ICON_SPEED: char = '\u{EC4A}';
const ICON_DIRECTION: char = '\u{E8AB}';
const ICON_BRIGHTNESS: char = '\u{E706}';
const ICON_PROFILE: char = '\u{E77B}';
const ICON_SLEEP: char = '\u{E708}';
const ICON_POWER: char = '\u{E7E8}';
const ICON_ALERT: char = '\u{E7BA}';
const ICON_WINDOWS: char = '\u{E771}';
const ICON_WHEEL: char = '\u{E767}';
const ICON_PIN: char = '\u{E840}';
const ICON_SYNC: char = '\u{E895}';
const ICON_EFFECT: char = '\u{EA80}';

struct Ui<'a> {
    c: &'a Canvas,
    t: &'a Theme,
    hits: Vec<(Rc, Hit)>,
    hover: Option<Hit>,
}

impl Ui<'_> {
    fn hit(&mut self, r: Rc, hit: Hit) {
        self.hits.push((r, hit));
    }

    /// Titre de section.
    fn heading(&self, text: &str, x: f32, y: f32) {
        self.c.text(text, rc(x, y, 400.0, 20.0), 14.0, true, self.t.text, Align::Left);
    }

    fn card(&self, r: Rc) {
        self.c.fill(r, 6.0, self.t.card);
        self.c.stroke(r, 6.0, self.t.card_border, 1.0);
    }

    /// Carte de réglage : icône, intitulé et description à gauche ; le contrôle se pose à
    /// droite, à l'abscisse renvoyée (bord droit utile).
    fn setting(&self, y: f32, icon: char, title: &str, desc: &str) -> f32 {
        let r = rc(CX, y, CW, ROW);
        self.card(r);
        self.c.icon(icon, rc(r.x + 12.0, r.y, 32.0, r.h), 17.0, self.t.text);
        if desc.is_empty() {
            self.c.text(title, rc(r.x + 54.0, r.y, 360.0, r.h), 14.0, false, self.t.text, Align::Left);
        } else {
            self.c.text(title, rc(r.x + 54.0, r.y + 7.0, 360.0, 20.0), 14.0, false, self.t.text, Align::Left);
            self.c.text(desc, rc(r.x + 54.0, r.y + 26.0, 420.0, 18.0), 12.0, false, self.t.dim, Align::Left);
        }
        r.x + r.w - 14.0
    }

    fn button(&mut self, r: Rc, text: &str, accent: bool, hit: Hit) {
        let hovered = self.hover == Some(hit);
        if accent {
            let fill = if hovered { self.t.text } else { self.t.accent };
            let fg = if hovered { self.t.layer } else { self.t.on_accent };
            self.c.fill(r, 4.0, fill);
            self.c.text(text, r, 14.0, true, fg, Align::Center);
        } else {
            self.c.fill(r, 4.0, if hovered { self.t.hover } else { self.t.control });
            self.c.stroke(r, 4.0, self.t.control_border, 1.0);
            self.c.text(text, r, 14.0, false, self.t.text, Align::Center);
        }
        self.hit(r, hit);
    }

    /// Choix exclusif sur une rangée, calé à droite sur `right`.
    fn segmented(&mut self, right: f32, cy: f32, width: f32, labels: &[&str], selected: Option<usize>, hit: impl Fn(usize) -> Hit) {
        let r = rc(right - width, cy - 16.0, width, 32.0);
        self.c.fill(r, 4.0, self.t.control);
        self.c.stroke(r, 4.0, self.t.control_border, 1.0);
        let w = r.w / labels.len() as f32;
        for (i, label) in labels.iter().enumerate() {
            let cell = rc(r.x + i as f32 * w, r.y, w, r.h);
            let inner = rc(cell.x + 3.0, cell.y + 3.0, cell.w - 6.0, cell.h - 6.0);
            let this = hit(i);
            let active = selected == Some(i);
            if active {
                self.c.fill(inner, 3.0, self.t.accent);
            } else if self.hover == Some(this) {
                self.c.fill(inner, 3.0, self.t.hover);
            }
            let fg = if active { self.t.on_accent } else { self.t.text };
            self.c.text(label, cell, 13.0, active, fg, Align::Center);
            self.hit(cell, this);
        }
    }

    /// Interrupteur de Windows 11, calé à droite sur `right`.
    fn toggle(&mut self, right: f32, cy: f32, on: bool, hit: Hit) {
        let r = rc(right - 40.0, cy - 10.0, 40.0, 20.0);
        if on {
            self.c.fill(r, 10.0, self.t.accent);
            self.c.fill(rc(r.x + 24.0, r.y + 4.0, 12.0, 12.0), 6.0, self.t.on_accent);
        } else {
            self.c.fill(r, 10.0, self.t.control);
            self.c.stroke(r, 10.0, self.t.strong, 1.0);
            self.c.fill(rc(r.x + 4.0, r.y + 4.0, 12.0, 12.0), 6.0, self.t.strong);
        }
        self.c.text(if on { "Activé" } else { "Désactivé" }, rc(r.x - 90.0, r.y, 80.0, r.h), 13.0, false, self.t.text, Align::Right);
        self.hit(rc(r.x - 6.0, r.y - 8.0, 52.0, 36.0), hit);
    }

    /// Curseur horizontal calé à droite sur `right` ; `value` entre 0 et 1.
    fn slider(&mut self, right: f32, cy: f32, width: f32, value: f32, hit: Hit) {
        let x = right - width;
        self.c.fill(rc(x + 10.0, cy - 2.0, width - 20.0, 4.0), 2.0, self.t.strong);
        self.c.fill(rc(x + 10.0, cy - 2.0, ((width - 20.0) * value).max(4.0), 4.0), 2.0, self.t.accent);
        let kx = x + 10.0 + (width - 20.0) * value;
        self.c.fill(rc(kx - 10.0, cy - 10.0, 20.0, 20.0), 10.0, self.t.card);
        self.c.stroke(rc(kx - 10.0, cy - 10.0, 20.0, 20.0), 10.0, self.t.control_border, 1.0);
        self.c.fill(rc(kx - 6.0, cy - 6.0, 12.0, 12.0), 6.0, self.t.accent);
        self.hit(rc(x, cy - 16.0, width, 32.0), hit);
    }

    /// Pastille de couleur ronde.
    fn swatch(&mut self, x: f32, y: f32, d: f32, color: Rgb, selected: bool, hit: Hit) {
        let r = rc(x, y, d, d);
        if selected {
            self.c.stroke(rc(x - 4.0, y - 4.0, d + 8.0, d + 8.0), d, self.t.accent, 2.0);
        }
        self.c.fill(r, d / 2.0, argb(color));
        self.c.stroke(r, d / 2.0, self.t.control_border, 1.0);
        self.hit(r, hit);
    }

    /// Petit bouton rond portant un symbole (+, −).
    fn round_button(&mut self, x: f32, y: f32, d: f32, symbol: &str, hit: Hit) {
        let r = rc(x, y, d, d);
        self.c.fill(r, d / 2.0, if self.hover == Some(hit) { self.t.hover } else { self.t.control });
        self.c.stroke(r, d / 2.0, self.t.control_border, 1.0);
        self.c.text(symbol, r, 16.0, false, self.t.text, Align::Center);
        self.hit(r, hit);
    }

    /// Bande arc-en-ciel, décalée de `shift` degrés.
    fn rainbow(&self, r: Rc, shift: f32) {
        let seg = r.w / 6.0;
        for i in 0..6 {
            let from = argb(hsv_to_rgb(shift + i as f32 * 60.0, 1.0, 1.0));
            let to = argb(hsv_to_rgb(shift + (i + 1) as f32 * 60.0, 1.0, 1.0));
            self.c.gradient(rc(r.x + i as f32 * seg, r.y, seg + 0.5, r.h), from, to, false);
        }
    }

    /// Aperçu de ce que fait un effet, dans le bandeau d'une tuile.
    fn effect_preview(&self, r: Rc, mode: Mode) {
        let night = 0xFF14161C;
        let dots = |color: u32, xs: &[f32]| {
            self.c.bar(r, night);
            for (i, x) in xs.iter().enumerate() {
                let y = r.y + 3.0 + (i % 3) as f32 * (r.h - 10.0) / 2.0;
                self.c.fill(rc(r.x + r.w * x, y, 4.0, 4.0), 2.0, color);
            }
        };
        match mode {
            Mode::Static => self.c.bar(r, 0xFFFF3355),
            Mode::Breathing => self.c.gradient(r, 0xFFFF3355, 0x22FF3355, false),
            Mode::ColorCycle => self.rainbow(r, 0.0),
            Mode::Wave => self.rainbow(r, 200.0),
            Mode::Reactive => dots(0xFFFFD000, &[0.15, 0.5, 0.8]),
            Mode::Ripple => {
                self.c.bar(r, night);
                for (i, color) in [0xFF0040FF, 0xFF00E0FF, 0xFFFFFFFF].iter().enumerate() {
                    let inset = r.w * (0.18 + 0.12 * i as f32);
                    self.c.bar(rc(r.x + inset, r.y + 2.0 * i as f32 + 2.0, r.w - 2.0 * inset, r.h - 4.0 * i as f32 - 4.0), *color);
                }
            }
            Mode::Starry => dots(0xFFFFFFFF, &[0.08, 0.2, 0.33, 0.47, 0.6, 0.74, 0.88]),
            Mode::Quicksand => self.c.gradient(r, 0xFFFFD000, 0xFFFF4000, true),
            Mode::Current => self.c.gradient(r, 0xFF0040FF, 0xFF00E0FF, false),
            Mode::RainDrop => dots(0xFF00E0FF, &[0.12, 0.3, 0.5, 0.68, 0.86]),
        }
    }
}

struct App {
    hwnd: HWND,
    theme: Theme,
    tx: Sender<Cmd>,
    shared: Arc<Mutex<Shared>>,
    snap: Option<Snapshot>,
    mouse: Option<MouseSnap>,
    message: String,
    is_error: bool,
    /// Faux tant qu'aucun état n'a été reçu du clavier (resp. de la souris).
    synced: bool,
    mouse_synced: bool,
    device: Device,
    tab: Tab,
    fx: Effect,
    mouse_fx: MouseLight,
    /// La souris suit l'éclairage du clavier.
    sync: bool,
    brush: Rgb,
    custom_brush: Rgb,
    key_colors: Vec<(u8, Rgb)>,
    legends: Vec<(u8, char)>,
    /// Valeur du curseur « alerte batterie » pendant qu'on le déplace.
    low_edit: Option<u8>,
    /// Gardien actif : la fenêtre se replie dans la zone de notification au lieu de se fermer.
    guard: bool,
    /// Le clavier de l'utilisateur est la version blanche.
    white: bool,
    hits: Vec<(Rc, Hit)>,
    hover: Option<Hit>,
    drag: Option<Hit>,
    picker: Option<Picker>,
}

fn short_label(mode: Mode) -> &'static str {
    match mode {
        Mode::Static => "Statique",
        Mode::Breathing => "Respiration",
        Mode::ColorCycle => "Cycle",
        Mode::Reactive => "Réactif",
        Mode::Wave => "Vague",
        Mode::Ripple => "Ondulation",
        Mode::Starry => "Nuit étoilée",
        Mode::Quicksand => "Sables mouvants",
        Mode::Current => "Courant",
        Mode::RainDrop => "Gouttes",
    }
}

fn key_label(name: &str) -> &str {
    match name {
        "esc" => "Éch",
        "insert" => "Ins",
        "delete" => "Suppr",
        "pageup" => "Pg↑",
        "pagedown" => "Pg↓",
        "logo" => "ROG",
        "backspace" => "←",
        "numlock" => "Verr",
        "numdivide" => "/",
        "nummultiply" => "*",
        "numminus" => "−",
        "numplus" => "+",
        "numenter" | "enter" => "↵",
        "numperiod" => ".",
        "tab" => "Tab",
        "capslock" => "Maj",
        "lshift" | "rshift" => "⇧",
        "iso" => "<",
        "up" => "↑",
        "down" => "↓",
        "left" => "←",
        "right" => "→",
        "lctrl" | "rctrl" => "Ctrl",
        "lwin" => "Win",
        "lalt" => "Alt",
        "ralt" => "AltGr",
        "fn" => "Fn",
        "space1" | "space2" | "space3" => "",
        other => other.strip_prefix("num").unwrap_or(other),
    }
}

const DIRS_WAVE: [(Direction, &str); 6] = [
    (Direction::Left, "←"),
    (Direction::Right, "→"),
    (Direction::Up, "↑"),
    (Direction::Down, "↓"),
    (Direction::Horizontal, "↔"),
    (Direction::Vertical, "↕"),
];
const DIRS_SAND: [(Direction, &str); 2] = [(Direction::Down, "↓"), (Direction::Up, "↑")];

impl App {
    fn send(&self, cmd: Cmd) {
        // Le fil de travail vit aussi longtemps que la fenêtre ; un envoi ne peut échouer
        // qu'à la fermeture, où il n'y a plus rien à faire.
        let _ = self.tx.send(cmd);
    }

    fn dirs(&self) -> &'static [(Direction, &'static str)] {
        if self.fx.mode == Mode::Quicksand {
            &DIRS_SAND
        } else {
            &DIRS_WAVE
        }
    }

    /// Change d'effet en gardant un formulaire cohérent avec ce que l'effet accepte.
    fn set_mode(&mut self, mode: Mode) {
        let info = mode.info();
        let (min, max) = info.colors;
        self.fx.mode = mode;
        self.fx.colors.truncate(max);
        while self.fx.colors.len() < min {
            self.fx.colors.push(PRESETS[self.fx.colors.len() % 8]);
        }
        if mode == Mode::Quicksand && self.fx.colors.len() != 6 {
            self.fx.colors.clear(); // 6 couleurs ou aucune (palette d'origine)
        }
        self.fx.random &= info.has_random;
        if !self.dirs().iter().any(|(d, _)| *d == self.fx.direction) {
            self.fx.direction = self.dirs()[0].0;
        }
    }

    fn push_effect(&mut self) {
        // Un effet remplace les couleurs touche par touche : sans cela, le gardien les
        // reposerait par-dessus au prochain réveil du clavier.
        if self.guard {
            self.set_guard(false);
        }
        self.send(Cmd::Effect(self.fx.clone()));
        if self.sync {
            self.mouse_fx = mouse_from_effect(&self.fx);
            self.send(Cmd::MouseLight(self.mouse_fx));
        }
    }

    fn push_mouse(&self) {
        self.send(Cmd::MouseLight(self.mouse_fx));
    }

    /// Régler la souris à la main rompt la synchronisation avec le clavier.
    fn edit_mouse(&mut self, edit: impl FnOnce(&mut MouseLight)) {
        if self.sync {
            self.sync = false;
            let _ = store::set_flag("sync", false);
        }
        edit(&mut self.mouse_fx);
        self.push_mouse();
    }

    fn set_guard(&mut self, on: bool) {
        self.guard = on;
        let _ = store::set_guard(on);
        let command = std::env::current_exe().ok().map(|exe| format!("\"{}\" --background", exe.display()));
        win::set_autostart("omnictl", if on { command.as_deref() } else { None });
        if on {
            win::tray_add(self.hwnd, WM_TRAY, TRAY_TIP);
        } else {
            win::tray_remove(self.hwnd);
        }
        self.send(Cmd::Guard(on));
    }

    fn set_key(&mut self, id: u8, color: Rgb) -> bool {
        match self.key_colors.iter_mut().find(|(i, _)| *i == id) {
            Some(slot) if slot.1 == color => false,
            Some(slot) => {
                slot.1 = color;
                true
            }
            None => {
                self.key_colors.push((id, color));
                true
            }
        }
    }

    fn paint_key(&mut self, id: u8) {
        if self.set_key(id, self.brush) {
            self.send(Cmd::Keys(vec![(id, self.brush)]));
        }
    }

    fn paint_all(&mut self, color: Rgb) {
        let leds: Vec<(u8, Rgb)> = keys::KEYS.iter().map(|k| (k.id, color)).collect();
        for &(id, c) in &leds {
            self.set_key(id, c);
        }
        self.send(Cmd::Keys(leds));
    }

    /// Reprend le dernier état publié par le fil de travail.
    fn sync_state(&mut self) {
        if let Ok(s) = self.shared.lock() {
            self.snap = s.snap.clone();
            self.mouse = s.mouse.clone();
            self.message = s.message.clone();
            self.is_error = s.is_error;
        }
        // Au premier contact, refléter les réglages en cours, sans rien envoyer.
        if !self.synced {
            if let Some(mode) = self.snap.as_ref().and_then(|s| Mode::from_code(s.effect)) {
                self.set_mode(mode);
                self.synced = true;
            }
        }
        if !self.mouse_synced {
            if let Some(m) = &self.mouse {
                if let Some(mode) = MouseMode::from_code(m.mode) {
                    self.mouse_fx.mode = mode;
                }
                self.mouse_fx.level = m.level.min(4);
                self.mouse_synced = true;
            }
        }
        win::redraw(self.hwnd);
    }

    fn hit_at(&self, x: f32, y: f32) -> Option<(Rc, Hit)> {
        // Dernier dessiné = au-dessus.
        self.hits.iter().rev().find(|(r, _)| r.contains(x, y)).copied()
    }

    fn rect_of(&self, hit: Hit) -> Option<Rc> {
        self.hits.iter().find(|(_, h)| *h == hit).map(|(r, _)| *r)
    }

    /// Position relative (0 à 1) du curseur `hit` pour l'abscisse `x`.
    fn slider_value(&self, hit: Hit, x: f32) -> Option<f32> {
        let r = self.rect_of(hit)?;
        Some(((x - r.x - 10.0) / (r.w - 20.0)).clamp(0.0, 1.0))
    }

    /// Répercute en direct la couleur du sélecteur sur ce qu'il modifie.
    fn apply_pick(&mut self) {
        let Some(p) = &self.picker else { return };
        let color = p.color();
        match p.target {
            PickTarget::EffectColor(i) => {
                if self.fx.colors.get(i).is_some_and(|c| *c != color) {
                    self.fx.colors[i] = color;
                    self.push_effect();
                }
            }
            PickTarget::MouseColor => {
                if self.mouse_fx.color != color {
                    self.edit_mouse(|m| m.color = color);
                }
            }
            PickTarget::Brush => {}
        }
    }

    fn drag_to(&mut self, hit: Hit, x: f32, y: f32) {
        match hit {
            Hit::Brightness => {
                if let Some(v) = self.slider_value(hit, x) {
                    let b = ((v * 20.0).round() * 5.0) as u8;
                    if b != self.fx.brightness {
                        self.fx.brightness = b;
                        self.push_effect();
                    }
                }
            }
            Hit::Low => {
                if let Some(v) = self.slider_value(hit, x) {
                    self.low_edit = Some(((v * 10.0).round() * 5.0) as u8);
                }
            }
            Hit::Key(_) => {
                if let Some((_, Hit::Key(id))) = self.hit_at(x, y) {
                    self.paint_key(id);
                }
            }
            Hit::PickSv | Hit::PickHue => {
                let Some(r) = self.rect_of(hit) else { return };
                let fx = ((x - r.x) / r.w).clamp(0.0, 1.0);
                let fy = ((y - r.y) / r.h).clamp(0.0, 1.0);
                if let Some(p) = &mut self.picker {
                    if hit == Hit::PickHue {
                        p.h = fx * 359.9;
                    } else {
                        (p.s, p.v) = (fx, 1.0 - fy);
                    }
                }
                self.apply_pick();
            }
            _ => {}
        }
    }

    fn mouse_down(&mut self, x: f32, y: f32) {
        let Some((_, hit)) = self.hit_at(x, y) else { return };
        match hit {
            Hit::Device(device) => self.device = device,
            Hit::Tab(tab) => self.tab = tab,
            Hit::Mode(mode) => {
                self.set_mode(mode);
                self.push_effect();
            }
            Hit::Color(i) => self.picker = Some(Picker::new(PickTarget::EffectColor(i), self.fx.colors[i])),
            Hit::AddColor => {
                let max = self.fx.mode.info().colors.1;
                if self.fx.mode == Mode::Quicksand {
                    self.fx.colors = PRESETS[..6].to_vec();
                } else if self.fx.colors.len() < max {
                    self.fx.colors.push(PRESETS[self.fx.colors.len() % 8]);
                }
                self.push_effect();
            }
            Hit::DelColor => {
                if self.fx.mode == Mode::Quicksand {
                    self.fx.colors.clear();
                } else if self.fx.colors.len() > self.fx.mode.info().colors.0 {
                    self.fx.colors.pop();
                }
                self.push_effect();
            }
            Hit::Random => {
                self.fx.random = !self.fx.random;
                self.push_effect();
            }
            Hit::Speed(speed) => {
                self.fx.speed = speed;
                self.push_effect();
            }
            Hit::Dir(direction) => {
                self.fx.direction = direction;
                self.push_effect();
            }
            Hit::SaveEffect => {
                if self.guard {
                    self.set_guard(false);
                }
                self.send(Cmd::SaveEffect(self.fx.clone()));
                if self.sync {
                    self.send(Cmd::MouseSave(self.mouse_fx));
                }
            }
            Hit::Preset(i) => self.brush = PRESETS[i],
            Hit::CustomBrush => self.picker = Some(Picker::new(PickTarget::Brush, self.custom_brush)),
            Hit::Fill => self.paint_all(self.brush),
            Hit::Clear => self.paint_all(Rgb::BLACK),
            Hit::Reapply => self.send(Cmd::ApplyKeys),
            Hit::Guard => self.set_guard(!self.guard),
            Hit::Chassis(white) => {
                self.white = white;
                let _ = store::set_flag("white", white);
            }
            Hit::Profile(n) => self.send(Cmd::Profile(n)),
            Hit::Sleep(i) => self.send(Cmd::Sleep(i)),
            Hit::Saving(m) => self.send(Cmd::Saving(m)),
            Hit::Wdl => {
                if let Some(s) = &self.snap {
                    self.send(Cmd::Wdl(!s.wdl));
                }
            }
            Hit::MouseMode(mode) => self.edit_mouse(|m| m.mode = mode),
            Hit::MouseLevel(level) => self.edit_mouse(|m| m.level = level),
            Hit::MouseColor => self.picker = Some(Picker::new(PickTarget::MouseColor, self.mouse_fx.color)),
            Hit::MouseSave => self.send(Cmd::MouseSave(self.mouse_fx)),
            Hit::Sync => {
                self.sync = !self.sync;
                let _ = store::set_flag("sync", self.sync);
                if self.sync {
                    self.mouse_fx = mouse_from_effect(&self.fx);
                    self.push_mouse();
                }
            }
            Hit::PickPreset(i) => {
                if let Some(p) = &mut self.picker {
                    (p.h, p.s, p.v) = rgb_to_hsv(PRESETS[i]);
                }
                self.apply_pick();
            }
            Hit::PickOk => {
                if let Some(p) = self.picker.take() {
                    if matches!(p.target, PickTarget::Brush) {
                        self.custom_brush = p.color();
                        self.brush = p.color();
                    }
                }
            }
            Hit::PickCancel => {
                // Rétablir la couleur d'origine, déjà modifiée en direct.
                if let Some(p) = &mut self.picker {
                    (p.h, p.s, p.v) = rgb_to_hsv(p.original);
                }
                self.apply_pick();
                self.picker = None;
            }
            Hit::Brightness | Hit::Low | Hit::Key(_) | Hit::PickSv | Hit::PickHue => {
                self.drag = Some(hit);
                win::capture(self.hwnd);
                self.drag_to(hit, x, y);
            }
        }
        win::redraw(self.hwnd);
    }

    fn mouse_move(&mut self, x: f32, y: f32) {
        if let Some(hit) = self.drag {
            self.drag_to(hit, x, y);
            win::redraw(self.hwnd);
            return;
        }
        let hover = self.hit_at(x, y).map(|(_, h)| h);
        if hover != self.hover {
            self.hover = hover;
            win::redraw(self.hwnd);
        }
    }

    fn mouse_up(&mut self) {
        if self.drag.take() == Some(Hit::Low) {
            if let Some(p) = self.low_edit.take() {
                self.send(Cmd::Low(p));
            }
        }
        win::release_capture();
        win::redraw(self.hwnd);
    }

    // ── Dessin ──

    fn draw(&mut self, c: &Canvas) {
        let mut hits = std::mem::take(&mut self.hits);
        hits.clear();
        let mut ui = Ui { c, t: &self.theme, hits, hover: self.hover };
        c.clear(self.theme.bg);
        // Zone de contenu : un calque plus clair, arrondi côté volet, qui file hors champ
        // à droite et en bas quand la fenêtre est plus grande que le contenu.
        let layer = rc(NAV, 8.0, 6000.0, 6000.0);
        c.fill(layer, 8.0, self.theme.layer);
        c.stroke(layer, 8.0, self.theme.card_border, 1.0);
        self.draw_nav(&mut ui);
        match (self.device, self.tab) {
            (Device::Mouse, _) => self.draw_mouse(&mut ui),
            (Device::Keyboard, tab) => {
                self.draw_keyboard_header(&mut ui);
                match tab {
                    Tab::Light => self.draw_light(&mut ui),
                    Tab::Keys => self.draw_keys(&mut ui),
                    Tab::Settings => self.draw_settings(&mut ui),
                }
            }
        }
        let color = if self.is_error { self.theme.danger } else { self.theme.dim };
        c.text(&self.message, rc(CX, H - 36.0, CW, 22.0), 12.5, false, color, Align::Left);
        if self.picker.is_some() {
            ui.hits.clear(); // la page derrière le sélecteur n'est plus cliquable
            self.draw_picker(&mut ui);
        }
        self.hits = ui.hits;
    }

    /// Volet de navigation : un élément par appareil, avec sa batterie.
    fn draw_nav(&self, ui: &mut Ui) {
        let t = &self.theme;
        let omni_w = ui.c.text_width("omni", 18.0, true);
        ui.c.text("omni", rc(20.0, 22.0, omni_w + 4.0, 28.0), 18.0, true, t.text, Align::Left);
        // GDI+ ajoute environ un sixième de cadratin de marge de chaque côté d'un texte.
        ui.c.text("ctl", rc(20.0 + omni_w - 6.0, 22.0, 60.0, 28.0), 18.0, true, t.accent, Align::Left);

        let kb = match &self.snap {
            Some(s) => format!("{} % · {}", s.percent, s.via),
            None => "Non joignable".to_string(),
        };
        let mouse = match &self.mouse {
            Some(m) => format!("{} % · 2,4 GHz", m.percent),
            None => "Non joignable".to_string(),
        };
        let items = [
            (Device::Keyboard, ICON_KEYBOARD, "Clavier", kb),
            (Device::Mouse, ICON_MOUSE, "Souris", mouse),
        ];
        for (i, (device, icon, title, subtitle)) in items.into_iter().enumerate() {
            let r = rc(8.0, 74.0 + i as f32 * 58.0, NAV - 16.0, 54.0);
            let hit = Hit::Device(device);
            let selected = self.device == device;
            if selected {
                ui.c.fill(r, 5.0, t.subtle);
                ui.c.fill(rc(r.x, r.y + 17.0, 3.0, 20.0), 1.5, t.accent);
            } else if ui.hover == Some(hit) {
                ui.c.fill(r, 5.0, t.hover);
            }
            ui.c.icon(icon, rc(r.x + 10.0, r.y, 36.0, r.h), 19.0, t.text);
            ui.c.text(title, rc(r.x + 52.0, r.y + 8.0, 150.0, 20.0), 14.0, selected, t.text, Align::Left);
            ui.c.text(&subtitle, rc(r.x + 52.0, r.y + 27.0, 160.0, 18.0), 12.0, false, t.dim, Align::Left);
            ui.hit(r, hit);
        }
        ui.c.text(concat!("omnictl ", env!("CARGO_PKG_VERSION")), rc(20.0, H - 36.0, 180.0, 22.0), 12.0, false, t.dim, Align::Left);
    }

    /// Titre de page, état de l'appareil et onglets du clavier.
    fn draw_keyboard_header(&self, ui: &mut Ui) {
        let t = &self.theme;
        ui.c.text("Clavier", rc(CX, 24.0, 300.0, 40.0), 28.0, true, t.text, Align::Left);
        let status = match &self.snap {
            Some(s) => format!(
                "ROG Strix Scope II 96   ·   {} %{}   ·   firmware {}",
                s.percent,
                if s.charging { " en charge" } else { "" },
                s.firmware
            ),
            None if self.synced || self.is_error => "Clavier non joignable".to_string(),
            None => "Recherche du clavier…".to_string(),
        };
        let color = if self.snap.is_some() { t.dim } else { t.danger };
        ui.c.text(&status, rc(CX + 300.0, 30.0, CW - 300.0, 30.0), 13.0, false, color, Align::Right);

        let tabs = [(Tab::Light, "Éclairage"), (Tab::Keys, "Touche par touche"), (Tab::Settings, "Réglages")];
        let mut x = CX;
        for (tab, label) in tabs {
            let active = self.tab == tab;
            let w = ui.c.text_width(label, 14.0, true) - 4.0;
            let color = if active || ui.hover == Some(Hit::Tab(tab)) { t.text } else { t.dim };
            ui.c.text(label, rc(x - 2.0, 74.0, w + 30.0, 28.0), 14.0, active, color, Align::Left);
            if active {
                ui.c.fill(rc(x, 103.0, w, 3.0), 1.5, t.accent);
            }
            ui.hit(rc(x - 8.0, 70.0, w + 16.0, 38.0), Hit::Tab(tab));
            x += w + 28.0;
        }
    }

    fn draw_light(&self, ui: &mut Ui) {
        let t = &self.theme;
        let info = self.fx.mode.info();
        let (min, max) = info.colors;

        ui.heading("Effet", CX, 124.0);
        let (tw, th, gap) = ((CW - 4.0 * 8.0) / 5.0, 62.0, 8.0);
        for (i, m) in proto::MODES.iter().enumerate() {
            let r = rc(CX + (i % 5) as f32 * (tw + gap), 150.0 + (i / 5) as f32 * (th + gap), tw, th);
            let hit = Hit::Mode(m.mode);
            let selected = self.fx.mode == m.mode;
            ui.c.fill(r, 6.0, if ui.hover == Some(hit) && !selected { t.hover } else { t.card });
            if selected {
                ui.c.stroke(r, 6.0, t.accent, 2.0);
            } else {
                ui.c.stroke(r, 6.0, t.card_border, 1.0);
            }
            ui.effect_preview(rc(r.x + 8.0, r.y + 8.0, r.w - 16.0, 18.0), m.mode);
            ui.c.text(short_label(m.mode), rc(r.x, r.y + 30.0, r.w, 26.0), 13.0, selected, t.text, Align::Center);
            ui.hit(r, hit);
        }

        let mut y = 150.0 + 2.0 * th + gap + 14.0;
        let mid = |y: f32| y + ROW / 2.0;

        let right = ui.setting(y, ICON_COLOR, "Couleurs", if max == 0 { "Cet effet parcourt toutes les couleurs." } else { "" });
        if max > 0 {
            let sand = self.fx.mode == Mode::Quicksand;
            let can_add = self.fx.colors.len() < max && !(sand && !self.fx.colors.is_empty());
            let can_del = self.fx.colors.len() > min;
            let count = self.fx.colors.len() + can_add as usize + can_del as usize;
            let mut x = right - count as f32 * 38.0 + 8.0;
            if self.fx.colors.is_empty() {
                ui.c.text("Arc-en-ciel par défaut", rc(x - 190.0, y, 180.0, ROW), 13.0, false, t.dim, Align::Right);
            }
            for (i, color) in self.fx.colors.iter().enumerate() {
                ui.swatch(x, mid(y) - 15.0, 30.0, *color, false, Hit::Color(i));
                x += 38.0;
            }
            if can_add {
                ui.round_button(x, mid(y) - 15.0, 30.0, "+", Hit::AddColor);
                x += 38.0;
            }
            if can_del {
                ui.round_button(x, mid(y) - 15.0, 30.0, "−", Hit::DelColor);
            }
        }
        y += STEP;

        if info.has_random {
            let right = ui.setting(y, ICON_SHUFFLE, "Couleurs aléatoires", "");
            ui.toggle(right, mid(y), self.fx.random, Hit::Random);
            y += STEP;
        }
        if info.has_speed {
            let right = ui.setting(y, ICON_SPEED, "Vitesse", "");
            let speeds = [Speed::Slow, Speed::Medium, Speed::Fast];
            let selected = speeds.iter().position(|s| *s == self.fx.speed);
            ui.segmented(right, mid(y), 270.0, &["Lente", "Moyenne", "Rapide"], selected, |i| Hit::Speed(speeds[i]));
            y += STEP;
        }
        if info.has_direction {
            let right = ui.setting(y, ICON_DIRECTION, "Direction", "");
            let dirs = self.dirs();
            let labels: Vec<&str> = dirs.iter().map(|(_, l)| *l).collect();
            let selected = dirs.iter().position(|(d, _)| *d == self.fx.direction);
            ui.segmented(right, mid(y), dirs.len() as f32 * 45.0, &labels, selected, |i| Hit::Dir(dirs[i].0));
            y += STEP;
        }
        let right = ui.setting(y, ICON_BRIGHTNESS, "Luminosité", "");
        ui.c.text(&format!("{} %", self.fx.brightness), rc(right - 56.0, y, 56.0, ROW), 13.0, false, t.text, Align::Right);
        ui.slider(right - 64.0, mid(y), 300.0, self.fx.brightness as f32 / 100.0, Hit::Brightness);
        y += STEP + 10.0;

        ui.button(rc(CX, y, 230.0, 34.0), "Enregistrer dans le clavier", true, Hit::SaveEffect);
        ui.c.text(
            "Appliqué en direct. Une fois enregistré, l'effet tourne sans logiciel, sur tout PC.",
            rc(CX + 246.0, y, CW - 246.0, 34.0),
            12.5,
            false,
            t.dim,
            Align::Left,
        );
    }

    fn draw_keys(&self, ui: &mut Ui) {
        let t = &self.theme;
        ui.heading("Pinceau", CX, 124.0);
        for (i, color) in PRESETS.iter().enumerate() {
            ui.swatch(CX + 2.0 + i as f32 * 38.0, 150.0, 28.0, *color, self.brush == *color, Hit::Preset(i));
        }
        let other = rc(CX + 10.0 * 38.0 + 10.0, 148.0, 120.0, 32.0);
        ui.button(other, "Autre couleur", false, Hit::CustomBrush);
        if !PRESETS.contains(&self.brush) {
            ui.swatch(other.x + other.w + 14.0, 150.0, 28.0, self.brush, true, Hit::CustomBrush);
        }
        ui.c.text("Clavier", rc(CX + CW - 220.0, 148.0, 70.0, 32.0), 13.0, false, t.dim, Align::Right);
        ui.segmented(CX + CW, 164.0, 140.0, &["Noir", "Blanc"], Some(self.white as usize), |i| Hit::Chassis(i == 1));

        // Clavier dessiné d'après le vrai : châssis, capuchons, et la couleur de chaque LED qui
        // déborde sous sa touche et éclaire sa légende. Positions en quarts de touche, 10 px le
        // quart. Teintes : châssis, contour, touche éteinte, légende éteinte, voyants, molette.
        let (body, edge, cap_off, legend_off, dot, knob_fill, knob_edge) = if self.white {
            (0xFFE7E9EC, 0xFFC6CAD1, 0xFFF8F8F9, 0xFF9AA1AC, 0xFFC3C7CE, 0xFFD5D8DD, 0xFFB4B9C1)
        } else {
            (0xFF0B0C0F, 0xFF2B2E35, 0xFF1C1E23, 0xFF5C6370, 0xFF30343C, 0xFF1D2025, 0xFF353941)
        };
        let u = 10.0;
        let chassis = rc(CX + (CW - 804.0) / 2.0, 196.0, 78.0 * u + 24.0, 24.0 * u + 24.0);
        ui.c.fill(chassis, 16.0, body);
        ui.c.stroke(chassis, 16.0, edge, 1.0);
        let (ox, oy) = (chassis.x + 12.0, chassis.y + 12.0);
        // Colonne de voyants entre le bloc principal et le pavé numérique.
        for i in 0..5 {
            ui.c.fill(rc(ox + 60.6 * u, oy + (9.6 + i as f32 * 1.5) * u, 0.8 * u, 0.8 * u), 0.4 * u, dot);
        }
        // Molette, en haut à droite (décor : elle n'a pas de LED).
        let knob = rc(ox + 75.4 * u, oy + 0.3 * u, 2.5 * u, 3.4 * u);
        ui.c.fill(knob, 5.0, knob_fill);
        ui.c.stroke(knob, 5.0, knob_edge, 1.0);
        for i in 0..4 {
            ui.c.bar(rc(knob.x + 5.0 + i as f32 * 5.0, knob.y + 5.0, 1.0, knob.h - 10.0), knob_edge);
        }

        for key in keys::KEYS {
            let (kx, ky, kw, kh) = key.rect;
            if kw == 0 {
                continue; // touche absente de la version ISO
            }
            let r = rc(ox + kx as f32 * u + 1.5, oy + ky as f32 * u + 1.5, kw as f32 * u - 3.0, kh as f32 * u - 3.0);
            let lit = self
                .key_colors
                .iter()
                .find(|(id, _)| *id == key.id)
                .map(|(_, c)| *c)
                .filter(|c| *c != Rgb::BLACK);
            let cap = rc(r.x + 2.5, r.y + 2.5, r.w - 5.0, r.h - 5.0);
            let legend_color = match lit {
                Some(c) if self.white => {
                    // Sur un clavier blanc, une LED faible ne noircit pas la touche : elle la
                    // teinte à peine. On sépare donc la teinte (pleine) de l'intensité.
                    let v = c.0.max(c.1).max(c.2) as f32 / 255.0;
                    let hue = Rgb((c.0 as f32 / v) as u8, (c.1 as f32 / v) as u8, (c.2 as f32 / v) as u8);
                    ui.c.fill(r, 6.0, argb(mix(Rgb(205, 208, 214), hue, v)));
                    ui.c.fill(cap, 4.0, argb(mix(Rgb(248, 248, 249), hue, 0.25 * v)));
                    argb(mix(Rgb(120, 126, 136), mix(hue, Rgb(0, 0, 0), 0.35), v))
                }
                Some(c) => {
                    ui.c.fill(r, 6.0, argb(c));
                    ui.c.fill(cap, 4.0, argb(mix(Rgb(20, 22, 26), c, 0.28)));
                    argb(mix(c, Rgb(255, 255, 255), 0.45))
                }
                None => {
                    ui.c.fill(r, 6.0, cap_off);
                    if self.white {
                        ui.c.stroke(r, 6.0, edge, 1.0);
                    }
                    legend_off
                }
            };
            if ui.hover == Some(Hit::Key(key.id)) {
                ui.c.stroke(r, 6.0, if self.white { 0xFF000000 } else { 0xFFFFFFFF }, 1.5);
            }
            let legend = self
                .legends
                .iter()
                .find(|(scan, _)| *scan == key.scan && key.scan != 0)
                .map(|(_, ch)| ch.to_uppercase().to_string());
            let text = legend.unwrap_or_else(|| {
                let l = key_label(key.name);
                // Noms bruts restants (f1…f12, chiffres) en capitales ; les libellés sont déjà mis en forme.
                if l.chars().all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit()) {
                    l.to_uppercase()
                } else {
                    l.to_string()
                }
            });
            ui.c.text(&text, r, 10.5, true, legend_color, Align::Center);
            ui.hit(r, Hit::Key(key.id));
        }

        let by = chassis.y + chassis.h + 14.0;
        ui.button(rc(CX, by, 124.0, 32.0), "Tout remplir", false, Hit::Fill);
        ui.button(rc(CX + 132.0, by, 124.0, 32.0), "Tout éteindre", false, Hit::Clear);
        ui.button(rc(CX + 264.0, by, 124.0, 32.0), "Réappliquer", false, Hit::Reapply);
        ui.c.text("Cliquez ou glissez sur les touches pour les colorer.", rc(CX + 404.0, by, CW - 404.0, 32.0), 12.5, false, t.dim, Align::Left);

        let y = by + 46.0;
        let right = ui.setting(
            y,
            ICON_PIN,
            "Garder ces couleurs",
            "omnictl reste en arrière-plan et les renvoie au clavier à chaque réveil.",
        );
        ui.toggle(right, y + ROW / 2.0, self.guard, Hit::Guard);
    }

    fn draw_settings(&self, ui: &mut Ui) {
        let t = &self.theme;
        let s = self.snap.as_ref();
        let row = |i: usize| 126.0 + i as f32 * STEP;
        let mid = |i: usize| row(i) + ROW / 2.0;

        let right = ui.setting(row(0), ICON_PROFILE, "Profil embarqué", "Aussi au clavier : Fn + 1 à 6.");
        ui.segmented(right, mid(0), 330.0, &["Défaut", "1", "2", "3", "4", "5", "6"], s.map(|s| s.profile as usize), |i| {
            Hit::Profile(i as u8)
        });

        let right = ui.setting(row(1), ICON_SLEEP, "Mise en veille", "Délai d'inactivité sur batterie.");
        ui.segmented(right, mid(1), 400.0, &["1 min", "2 min", "3 min", "5 min", "10 min", "Jamais"], s.map(|s| s.sleep as usize), |i| {
            Hit::Sleep(i as u8)
        });

        let right = ui.setting(row(2), ICON_POWER, "Économie d'énergie", "");
        ui.segmented(right, mid(2), 420.0, &["Désactivée", "Éteindre l'éclairage", "Baisser la luminosité"], s.map(|s| s.saving as usize), |i| {
            Hit::Saving(i as u8)
        });

        let right = ui.setting(row(3), ICON_ALERT, "Alerte batterie faible", "");
        if let Some(low) = self.low_edit.or(s.map(|s| s.low)) {
            ui.c.text(&format!("{low} %"), rc(right - 56.0, row(3), 56.0, ROW), 13.0, false, t.text, Align::Right);
            ui.slider(right - 64.0, mid(3), 300.0, (low as f32 / 50.0).min(1.0), Hit::Low);
        }

        let right = ui.setting(
            row(4),
            ICON_WINDOWS,
            "Éclairage dynamique Windows",
            "Laisse Windows 11 piloter l'éclairage du clavier lui-même.",
        );
        if let Some(s) = s {
            ui.toggle(right, mid(4), s.wdl, Hit::Wdl);
        }

        let right = ui.setting(row(5), ICON_WHEEL, "Molette", "Fonction active ; se change sur le clavier.");
        let lever = s.and_then(|s| proto::LEVER_CHOICES.get(s.lever as usize)).copied().unwrap_or("—");
        // Majuscule initiale pour l'affichage (les libellés servent aussi en milieu de phrase).
        let mut chars = lever.chars();
        let lever: String = chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default();
        ui.c.text(&lever, rc(right - 260.0, row(5), 260.0, ROW), 14.0, false, t.text, Align::Right);
    }

    fn draw_mouse(&self, ui: &mut Ui) {
        let t = &self.theme;
        ui.c.text("Souris", rc(CX, 24.0, 300.0, 40.0), 28.0, true, t.text, Align::Left);
        let status = match &self.mouse {
            Some(m) => format!("ROG Keris II Ace   ·   {} %   ·   firmware {}", m.percent, m.firmware),
            None => "Souris non joignable (éteinte, en veille ou non appairée au dongle)".to_string(),
        };
        let color = if self.mouse.is_some() { t.dim } else { t.danger };
        ui.c.text(&status, rc(CX + 200.0, 30.0, CW - 200.0, 30.0), 13.0, false, color, Align::Right);

        ui.heading("Éclairage", CX, 84.0);
        let row = |i: usize| 112.0 + i as f32 * STEP;
        let mid = |i: usize| row(i) + ROW / 2.0;

        let right = ui.setting(row(0), ICON_EFFECT, "Effet", "« Batterie » : la LED indique le niveau de charge.");
        let labels: Vec<&str> = MouseMode::ALL.iter().map(|(_, l)| *l).collect();
        let selected = MouseMode::ALL.iter().position(|(m, _)| *m == self.mouse_fx.mode);
        ui.segmented(right, mid(0), 380.0, &labels, selected, |i| Hit::MouseMode(MouseMode::ALL[i].0));

        let right = ui.setting(
            row(1),
            ICON_COLOR,
            "Couleur",
            if self.mouse_fx.mode.has_color() { "" } else { "Sans objet pour cet effet." },
        );
        if self.mouse_fx.mode.has_color() {
            ui.swatch(right - 30.0, mid(1) - 15.0, 30.0, self.mouse_fx.color, false, Hit::MouseColor);
        }

        let right = ui.setting(row(2), ICON_BRIGHTNESS, "Luminosité", "");
        ui.segmented(right, mid(2), 330.0, &["Éteint", "25 %", "50 %", "75 %", "100 %"], Some(self.mouse_fx.level as usize), |i| {
            Hit::MouseLevel(i as u8)
        });

        let right = ui.setting(
            row(3),
            ICON_SYNC,
            "Synchroniser avec le clavier",
            "La souris reprend l'effet, la couleur et la luminosité du clavier.",
        );
        ui.toggle(right, mid(3), self.sync, Hit::Sync);

        let y = row(4) + 10.0;
        ui.button(rc(CX, y, 230.0, 34.0), "Enregistrer dans la souris", true, Hit::MouseSave);
        ui.c.text(
            "La souris n'a qu'une LED : les effets du clavier sans équivalent deviennent une couleur fixe.",
            rc(CX + 246.0, y, CW - 246.0, 34.0),
            12.5,
            false,
            t.dim,
            Align::Left,
        );
        ui.c.text(
            "Réglages de performance (DPI, fréquence) : pas encore pris en charge.",
            rc(CX, y + 50.0, CW, 22.0),
            12.5,
            false,
            t.dim,
            Align::Left,
        );
    }

    fn draw_picker(&self, ui: &mut Ui) {
        let Some(p) = &self.picker else { return };
        let t = &self.theme;
        // Largement plus grand que le contenu : la fenêtre peut être plus grande que lui.
        ui.c.bar(rc(-4000.0, -4000.0, 8000.0 + W, 8000.0 + H), 0x66000000);
        let (px, py) = ((W - 320.0) / 2.0, (H - 404.0) / 2.0);
        let panel = rc(px, py, 320.0, 404.0);
        ui.c.fill(panel, 8.0, t.card);
        ui.c.stroke(panel, 8.0, t.control_border, 1.0);
        ui.c.text("Choisir une couleur", rc(px + 20.0, py + 14.0, 280.0, 28.0), 16.0, true, t.text, Align::Left);

        // Carré saturation (horizontale) / luminosité (verticale) pour la teinte courante.
        let sv = rc(px + 20.0, py + 52.0, 280.0, 170.0);
        ui.c.gradient(sv, 0xFFFFFFFF, argb(hsv_to_rgb(p.h, 1.0, 1.0)), false);
        ui.c.gradient(sv, 0x00000000, 0xFF000000, true);
        let (cx, cy) = (sv.x + p.s * sv.w, sv.y + (1.0 - p.v) * sv.h);
        ui.c.stroke(rc(cx - 7.0, cy - 7.0, 14.0, 14.0), 7.0, 0xFF000000, 3.0);
        ui.c.stroke(rc(cx - 7.0, cy - 7.0, 14.0, 14.0), 7.0, 0xFFFFFFFF, 1.5);
        ui.hit(sv, Hit::PickSv);

        let hue = rc(px + 20.0, py + 236.0, 280.0, 18.0);
        ui.rainbow(hue, 0.0);
        let hx = hue.x + p.h / 360.0 * hue.w;
        ui.c.fill(rc(hx - 4.0, hue.y - 3.0, 8.0, hue.h + 6.0), 4.0, 0xFFFFFFFF);
        ui.c.stroke(rc(hx - 4.0, hue.y - 3.0, 8.0, hue.h + 6.0), 4.0, 0xFF000000, 1.0);
        ui.hit(rc(hue.x, hue.y - 6.0, hue.w, hue.h + 12.0), Hit::PickHue);

        for (i, color) in PRESETS.iter().enumerate() {
            ui.swatch(px + 20.0 + i as f32 * 28.6, py + 272.0, 22.0, *color, false, Hit::PickPreset(i));
        }

        let color = p.color();
        let preview = rc(px + 20.0, py + 312.0, 46.0, 30.0);
        ui.c.fill(preview, 4.0, argb(color));
        ui.c.stroke(preview, 4.0, t.control_border, 1.0);
        ui.c.text(&color.hex().to_uppercase(), rc(px + 78.0, py + 312.0, 120.0, 30.0), 14.0, true, t.text, Align::Left);

        ui.button(rc(px + 20.0, py + 356.0, 134.0, 32.0), "Annuler", false, Hit::PickCancel);
        ui.button(rc(px + 166.0, py + 356.0, 134.0, 32.0), "Valider", true, Hit::PickOk);
    }
}

// ───────────────────────────── Fenêtre ─────────────────────────────

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

/// Accès à l'état. `None` si l'application n'est pas encore créée ou est déjà empruntée.
fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|cell| cell.try_borrow_mut().ok().and_then(|mut app| app.as_mut().map(f)))
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: usize, lparam: isize) -> isize {
    match msg {
        win::WM_PAINT => {
            let painted = with_app(|a| win::paint(hwnd, W, H, |c| a.draw(c)));
            if painted.is_none() {
                return win::DefWindowProcW(hwnd, msg, wparam, lparam); // valide la zone à peindre
            }
            0
        }
        win::WM_ERASEBKGND => 1,
        win::WM_LBUTTONDOWN => {
            let (x, y) = win::mouse_pos(hwnd, lparam, W, H);
            with_app(|a| a.mouse_down(x, y));
            0
        }
        win::WM_MOUSEMOVE => {
            let (x, y) = win::mouse_pos(hwnd, lparam, W, H);
            with_app(|a| a.mouse_move(x, y));
            0
        }
        win::WM_LBUTTONUP => {
            with_app(|a| a.mouse_up());
            0
        }
        win::WM_TIMER => {
            with_app(|a| a.send(Cmd::Refresh));
            0
        }
        WM_UPDATE => {
            with_app(|a| a.sync_state());
            0
        }
        win::WM_SIZE => {
            win::redraw(hwnd);
            0
        }
        win::WM_SETTINGCHANGE => {
            // Windows vient peut-être de changer de thème ou de couleur d'accentuation.
            let dark = win::dark_mode();
            with_app(|a| a.theme = theme(dark));
            win::set_title_dark(hwnd, dark);
            win::redraw(hwnd);
            win::DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        win::WM_DPICHANGED => {
            win::apply_suggested_rect(hwnd, lparam);
            win::redraw(hwnd);
            0
        }
        win::WM_CLOSE => {
            // Gardien actif : fermer la fenêtre la replie dans la zone de notification.
            if with_app(|a| a.guard) == Some(true) {
                win::show_window(hwnd, false);
            } else {
                win::destroy_window(hwnd);
            }
            0
        }
        WM_TRAY => {
            match lparam as u32 {
                win::WM_LBUTTONUP => win::show_window(hwnd, true),
                win::WM_RBUTTONUP => {
                    // Menu modal : appelé hors de tout emprunt de l'état.
                    match win::popup_menu(hwnd, &["Ouvrir omnictl", "Quitter"]) {
                        Some(0) => win::show_window(hwnd, true),
                        Some(1) => win::destroy_window(hwnd),
                        _ => {}
                    }
                }
                _ => {}
            }
            0
        }
        win::WM_DESTROY => {
            win::tray_remove(hwnd);
            win::PostQuitMessage(0);
            0
        }
        _ => win::DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn main() {
    win::init();
    let dark = win::dark_mode();
    let guard = store::guard();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |name: &str| args.iter().any(|a| a == name);
    // `--background` : lancé à l'ouverture de session par le gardien, sans montrer la fenêtre.
    let hwnd = win::create_window("omnictl", W, H, dark, !(has("--background") && guard), wndproc);
    if hwnd.is_null() {
        win::error_box("Impossible de créer la fenêtre.");
        return;
    }

    let (tx, rx) = channel();
    let shared = Arc::new(Mutex::new(Shared::default()));
    let worker_shared = Arc::clone(&shared);
    let hwnd_bits = hwnd as usize;
    std::thread::spawn(move || worker(rx, worker_shared, hwnd_bits, guard));
    if guard {
        win::tray_add(hwnd, WM_TRAY, TRAY_TIP);
    }

    let app = App {
        hwnd,
        theme: theme(dark),
        tx,
        shared,
        snap: None,
        mouse: None,
        message: String::new(),
        is_error: false,
        synced: false,
        mouse_synced: false,
        // `omnictl-gui mouse`, `keys` ou `settings` ouvre directement la page voulue.
        device: if has("mouse") { Device::Mouse } else { Device::Keyboard },
        tab: if has("keys") {
            Tab::Keys
        } else if has("settings") {
            Tab::Settings
        } else {
            Tab::Light
        },
        fx: {
            let mut fx = Effect::new(Mode::Static);
            fx.colors.push(PRESETS[0]);
            fx
        },
        mouse_fx: MouseLight { mode: MouseMode::Static, color: PRESETS[0], level: 4 },
        sync: store::flag("sync"),
        brush: PRESETS[0],
        custom_brush: Rgb(255, 255, 255),
        key_colors: store::load(),
        legends: keys::layout_legends(),
        low_edit: None,
        guard,
        white: store::flag("white"),
        hits: Vec::new(),
        hover: None,
        drag: None,
        picker: None,
    };
    app.send(Cmd::Refresh);
    APP.with(|cell| *cell.borrow_mut() = Some(app));
    win::redraw(hwnd);
    win::set_timer(hwnd, 1, REFRESH_MS);
    win::run_loop();
}
