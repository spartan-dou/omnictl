//! Interface graphique de scope96 : une fenêtre Windows native, entièrement dessinée par le
//! programme (voir `win.rs`), sans bibliothèque graphique.
//!
//! Architecture : le fil de l'interface ne parle jamais au clavier. Il envoie des commandes à
//! un fil de travail, qui les regroupe (un glissement de curseur produit des dizaines de
//! commandes dont seule la dernière compte), les exécute, relit l'état du clavier et prévient
//! la fenêtre. Un clavier en veille ou débranché ne fige donc jamais l'affichage.

#![windows_subsystem = "windows"]

mod win;

use scope96::keys;
use scope96::link::{Error, Link};
use scope96::proto::{self, Connection, Direction, Effect, Keyboard, Mode, Rgb, Speed};
use scope96::store;
use std::cell::RefCell;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use win::{rc, Align, Canvas, Rc, HWND};

const W: f32 = 860.0;
const H: f32 = 600.0;
/// Marge latérale.
const M: f32 = 28.0;
const WM_UPDATE: u32 = win::WM_APP + 1;
const REFRESH_MS: u32 = 20_000;

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

struct Theme {
    bg: u32,
    surface: u32,
    hover: u32,
    border: u32,
    text: u32,
    dim: u32,
    accent: u32,
    on_accent: u32,
    danger: u32,
    key: u32,
}

const DARK: Theme = Theme {
    bg: 0xFF15171B,
    surface: 0xFF1F2228,
    hover: 0xFF2A2E36,
    border: 0xFF30353E,
    text: 0xFFE9EBEE,
    dim: 0xFF8C94A2,
    accent: 0xFFFF3355,
    on_accent: 0xFFFFFFFF,
    danger: 0xFFFF6B6B,
    key: 0xFF262A31,
};

const LIGHT: Theme = Theme {
    bg: 0xFFF4F5F7,
    surface: 0xFFFFFFFF,
    hover: 0xFFECEEF2,
    border: 0xFFDADDE3,
    text: 0xFF1B1E23,
    dim: 0xFF687080,
    accent: 0xFFE0284A,
    on_accent: 0xFFFFFFFF,
    danger: 0xFFC62828,
    key: 0xFFE4E7EC,
};

fn argb(c: Rgb) -> u32 {
    0xFF00_0000 | (c.0 as u32) << 16 | (c.1 as u32) << 8 | c.2 as u32
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

#[derive(Default)]
struct Shared {
    snap: Option<Snapshot>,
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

fn keyboard(slot: &mut Option<Keyboard>) -> Result<&mut Keyboard, Error> {
    if slot.is_none() {
        *slot = Some(Keyboard::new(Link::open(None)?));
    }
    Ok(slot.as_mut().expect("rempli juste au-dessus"))
}

/// Exécute une commande. Renvoie le message à afficher, s'il y en a un.
fn execute(slot: &mut Option<Keyboard>, cmd: &Cmd) -> Result<Option<&'static str>, Error> {
    if matches!(cmd, Cmd::Refresh) {
        return Ok(None);
    }
    let kb = keyboard(slot)?;
    Ok(match cmd {
        Cmd::Refresh => None,
        Cmd::Effect(e) => {
            kb.set_effect(e)?;
            None // appliqué en direct : pas de message à chaque retouche
        }
        Cmd::SaveEffect(e) => {
            kb.set_effect(e)?;
            kb.save()?;
            Some("Effet enregistré dans le clavier.")
        }
        Cmd::Keys(leds) => {
            kb.set_direct(leds)?;
            store::save(&store::merge(leds))
                .map_err(|e| Error::Msg(format!("Couleurs appliquées mais non mémorisées : {e}")))?;
            None
        }
        Cmd::ApplyKeys => {
            let leds = store::load();
            if leds.is_empty() {
                return Err(Error::Msg("Aucune couleur touche par touche mémorisée.".into()));
            }
            kb.set_direct(&leds)?;
            Some("Couleurs touche par touche remises en place.")
        }
        Cmd::Profile(n) => {
            kb.set_profile(*n)?;
            Some("Profil changé.")
        }
        Cmd::Sleep(i) => {
            kb.set_sleep(*i)?;
            Some("Délai de veille réglé.")
        }
        Cmd::Saving(m) => {
            kb.set_saving(*m)?;
            Some("Économie d'énergie réglée.")
        }
        Cmd::Low(p) => {
            kb.set_low_alert(*p)?;
            Some("Alerte batterie réglée.")
        }
        Cmd::Wdl(on) => {
            kb.set_wdl(*on)?;
            Some(if *on {
                "Éclairage dynamique Windows activé."
            } else {
                "Éclairage dynamique Windows désactivé."
            })
        }
    })
}

fn snapshot(slot: &mut Option<Keyboard>) -> Result<Snapshot, Error> {
    let kb = keyboard(slot)?;
    let st = kb.status()?;
    if st.connection == Connection::Unreachable {
        return Err(Error::Msg(
            "Clavier en veille ou éteint : appuyez sur une touche pour le réveiller.".into(),
        ));
    }
    let pw = kb.power()?;
    Ok(Snapshot {
        via: match kb.link.via {
            scope96::link::Via::Usb => "USB",
            scope96::link::Via::Dongle => "2,4 GHz",
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

fn worker(rx: Receiver<Cmd>, shared: Arc<Mutex<Shared>>, hwnd: usize) {
    let mut kb: Option<Keyboard> = None;
    while let Ok(first) = rx.recv() {
        let mut batch = vec![first];
        while let Ok(more) = rx.try_recv() {
            batch.push(more);
        }
        let mut message: Option<(String, bool)> = None;
        for cmd in coalesce(batch) {
            match execute(&mut kb, &cmd) {
                Ok(Some(text)) => message = Some((text.to_string(), false)),
                Ok(None) => {}
                Err(e) => {
                    message = Some((e.to_string(), true));
                    kb = None; // rouvrir au prochain essai : le clavier a pu changer de mode
                }
            }
        }
        let snap = snapshot(&mut kb);
        if snap.is_err() {
            kb = None;
        }
        if let Ok(mut s) = shared.lock() {
            match (&snap, message) {
                // L'état du clavier prime : inutile d'afficher l'échec d'une commande
                // quand la vraie cause est que le clavier ne répond pas.
                (Err(e), _) => (s.message, s.is_error) = (e.to_string(), true),
                (Ok(_), Some((text, is_error))) => (s.message, s.is_error) = (text, is_error),
                // Tout va bien de nouveau : effacer une ancienne erreur.
                (Ok(_), None) if s.is_error => (s.message, s.is_error) = (String::new(), false),
                (Ok(_), None) => {}
            }
            s.snap = snap.ok();
        }
        win::post(hwnd, WM_UPDATE);
    }
}

// ───────────────────────────── Interface ─────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Light,
    Keys,
    Settings,
}

/// Zone cliquable, reconstruite à chaque dessin.
#[derive(Clone, Copy, PartialEq)]
enum Hit {
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
    Profile(u8),
    Sleep(u8),
    Saving(u8),
    Low,
    Wdl,
}

/// Action qui ouvre une boîte de dialogue modale, donc à mener hors de tout emprunt de l'état.
enum Deferred {
    PickEffectColor(usize, Rgb),
    PickBrush(Rgb),
}

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

    fn label(&self, text: &str, x: f32, y: f32) {
        self.c.text(text, rc(x, y, 400.0, 16.0), 11.0, true, self.t.dim, Align::Left);
    }

    /// Bouton à bascule (effets, boutons secondaires).
    fn chip(&mut self, r: Rc, text: &str, selected: bool, hit: Hit) {
        let hovered = self.hover == Some(hit);
        let (fill, fg) = match (selected, hovered) {
            (true, _) => (self.t.accent, self.t.on_accent),
            (false, true) => (self.t.hover, self.t.text),
            (false, false) => (self.t.surface, self.t.text),
        };
        self.c.fill(r, 8.0, fill);
        if !selected {
            self.c.stroke(r, 8.0, self.t.border, 1.0);
        }
        self.c.text(text, r, 13.0, selected, fg, Align::Center);
        self.hit(r, hit);
    }

    /// Choix exclusif sur une rangée.
    fn segmented(&mut self, r: Rc, labels: &[&str], selected: Option<usize>, hit: impl Fn(usize) -> Hit) {
        self.c.fill(r, 8.0, self.t.surface);
        self.c.stroke(r, 8.0, self.t.border, 1.0);
        let w = r.w / labels.len() as f32;
        for (i, label) in labels.iter().enumerate() {
            let cell = rc(r.x + i as f32 * w, r.y, w, r.h);
            let inner = rc(cell.x + 3.0, cell.y + 3.0, cell.w - 6.0, cell.h - 6.0);
            let this = hit(i);
            let active = selected == Some(i);
            if active {
                self.c.fill(inner, 6.0, self.t.accent);
            } else if self.hover == Some(this) {
                self.c.fill(inner, 6.0, self.t.hover);
            }
            let fg = if active { self.t.on_accent } else { self.t.text };
            self.c.text(label, cell, 13.0, active, fg, Align::Center);
            self.hit(cell, this);
        }
    }

    fn toggle(&mut self, x: f32, y: f32, on: bool, hit: Hit) {
        let r = rc(x, y, 46.0, 26.0);
        self.c.fill(r, 13.0, if on { self.t.accent } else { self.t.border });
        let knob_x = if on { x + 23.0 } else { x + 3.0 };
        self.c.fill(rc(knob_x, y + 3.0, 20.0, 20.0), 10.0, 0xFFFFFFFF);
        self.hit(rc(x - 4.0, y - 4.0, 54.0, 34.0), hit);
    }

    /// Curseur horizontal ; `value` entre 0 et 1.
    fn slider(&mut self, r: Rc, value: f32, hit: Hit) {
        let mid = r.y + r.h / 2.0;
        self.c.fill(rc(r.x, mid - 3.0, r.w, 6.0), 3.0, self.t.border);
        self.c.fill(rc(r.x, mid - 3.0, (r.w * value).max(6.0), 6.0), 3.0, self.t.accent);
        let knob = rc(r.x + r.w * value - 9.0, mid - 9.0, 18.0, 18.0);
        self.c.fill(knob, 9.0, 0xFFFFFFFF);
        self.c.stroke(knob, 9.0, self.t.accent, 2.0);
        self.hit(rc(r.x - 9.0, r.y - 4.0, r.w + 18.0, r.h + 8.0), hit);
    }

    fn primary(&mut self, r: Rc, text: &str, hit: Hit) {
        let fill = if self.hover == Some(hit) { self.t.text } else { self.t.accent };
        let fg = if self.hover == Some(hit) { self.t.bg } else { self.t.on_accent };
        self.c.fill(r, 8.0, fill);
        self.c.text(text, r, 13.5, true, fg, Align::Center);
        self.hit(r, hit);
    }

    /// Pastille de couleur ronde.
    fn swatch(&mut self, x: f32, y: f32, d: f32, color: Rgb, selected: bool, hit: Hit) {
        let r = rc(x, y, d, d);
        if selected {
            self.c.stroke(rc(x - 4.0, y - 4.0, d + 8.0, d + 8.0), d, self.t.accent, 2.0);
        }
        self.c.fill(r, d / 2.0, argb(color));
        self.c.stroke(r, d / 2.0, self.t.border, 1.0);
        self.hit(r, hit);
    }

    /// Petit bouton rond portant un symbole (+, −).
    fn round_button(&mut self, x: f32, y: f32, d: f32, symbol: &str, hit: Hit) {
        let r = rc(x, y, d, d);
        let fill = if self.hover == Some(hit) { self.t.hover } else { self.t.surface };
        self.c.fill(r, d / 2.0, fill);
        self.c.stroke(r, d / 2.0, self.t.border, 1.0);
        self.c.text(symbol, r, 17.0, false, self.t.text, Align::Center);
        self.hit(r, hit);
    }
}

struct App {
    hwnd: HWND,
    theme: &'static Theme,
    tx: Sender<Cmd>,
    shared: Arc<Mutex<Shared>>,
    snap: Option<Snapshot>,
    message: String,
    is_error: bool,
    /// Faux tant qu'aucun état n'a été reçu du clavier.
    synced: bool,
    tab: Tab,
    fx: Effect,
    brush: Rgb,
    custom_brush: Rgb,
    key_colors: Vec<(u8, Rgb)>,
    legends: Vec<(u8, char)>,
    /// Valeur du curseur « alerte batterie » pendant qu'on le déplace.
    low_edit: Option<u8>,
    hits: Vec<(Rc, Hit)>,
    hover: Option<Hit>,
    drag: Option<Hit>,
    /// Couleurs personnalisées du sélecteur de couleur de Windows.
    custom: [u32; 16],
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

    fn push_effect(&self) {
        self.send(Cmd::Effect(self.fx.clone()));
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
    fn sync(&mut self) {
        if let Ok(s) = self.shared.lock() {
            self.snap = s.snap.clone();
            self.message = s.message.clone();
            self.is_error = s.is_error;
        }
        if !self.synced {
            if let Some(mode) = self.snap.as_ref().and_then(|s| Mode::from_code(s.effect)) {
                self.set_mode(mode); // refléter l'effet en cours, sans rien envoyer
                self.synced = true;
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
        Some(((x - r.x - 9.0) / (r.w - 18.0)).clamp(0.0, 1.0))
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
            _ => {}
        }
    }

    fn mouse_down(&mut self, x: f32, y: f32) -> Option<Deferred> {
        let (_, hit) = self.hit_at(x, y)?;
        let mut deferred = None;
        match hit {
            Hit::Tab(tab) => self.tab = tab,
            Hit::Mode(mode) => {
                self.set_mode(mode);
                self.push_effect();
            }
            Hit::Color(i) => deferred = Some(Deferred::PickEffectColor(i, self.fx.colors[i])),
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
            Hit::SaveEffect => self.send(Cmd::SaveEffect(self.fx.clone())),
            Hit::Preset(i) => self.brush = PRESETS[i],
            Hit::CustomBrush => deferred = Some(Deferred::PickBrush(self.custom_brush)),
            Hit::Fill => self.paint_all(self.brush),
            Hit::Clear => self.paint_all(Rgb::BLACK),
            Hit::Reapply => self.send(Cmd::ApplyKeys),
            Hit::Profile(n) => self.send(Cmd::Profile(n)),
            Hit::Sleep(i) => self.send(Cmd::Sleep(i)),
            Hit::Saving(m) => self.send(Cmd::Saving(m)),
            Hit::Wdl => {
                if let Some(s) = &self.snap {
                    self.send(Cmd::Wdl(!s.wdl));
                }
            }
            Hit::Brightness | Hit::Low | Hit::Key(_) => {
                self.drag = Some(hit);
                win::capture(self.hwnd);
                self.drag_to(hit, x, y);
            }
        }
        win::redraw(self.hwnd);
        deferred
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
        let mut ui = Ui { c, t: self.theme, hits, hover: self.hover };
        c.clear(self.theme.bg);
        self.draw_header(&mut ui);
        match self.tab {
            Tab::Light => self.draw_light(&mut ui),
            Tab::Keys => self.draw_keys(&mut ui),
            Tab::Settings => self.draw_settings(&mut ui),
        }
        let color = if self.is_error { self.theme.danger } else { self.theme.dim };
        c.text(&self.message, rc(M, H - 40.0, W - 2.0 * M, 24.0), 12.5, false, color, Align::Left);
        self.hits = ui.hits;
    }

    fn draw_header(&self, ui: &mut Ui) {
        let t = self.theme;
        // « 96 » en couleur d'accent, accolé à « scope » : GDI+ ajoute une marge d'environ
        // un sixième de cadratin de chaque côté d'un texte, qu'on retire à la jonction.
        let scope_w = ui.c.text_width("scope", 23.0, true);
        ui.c.text("scope", rc(M, 20.0, scope_w + 4.0, 34.0), 23.0, true, t.text, Align::Left);
        ui.c.text("96", rc(M + scope_w - 23.0 / 3.0, 20.0, 60.0, 34.0), 23.0, true, t.accent, Align::Left);
        let status = match &self.snap {
            Some(s) => format!(
                "{}   ·   {} %{}   ·   firmware {}",
                s.via,
                s.percent,
                if s.charging { " en charge" } else { "" },
                s.firmware
            ),
            None if self.synced || self.is_error => "Clavier non joignable".to_string(),
            None => "Recherche du clavier…".to_string(),
        };
        let color = if self.snap.is_some() { t.dim } else { t.danger };
        ui.c.text(&status, rc(300.0, 22.0, W - 300.0 - M, 30.0), 13.0, false, color, Align::Right);

        let tabs = [(Tab::Light, "Éclairage", 76.0), (Tab::Keys, "Touche par touche", 132.0), (Tab::Settings, "Réglages", 70.0)];
        let mut x = M;
        for (tab, label, w) in tabs {
            let active = self.tab == tab;
            let color = if active || ui.hover == Some(Hit::Tab(tab)) { t.text } else { t.dim };
            ui.c.text(label, rc(x, 70.0, w + 20.0, 30.0), 14.5, active, color, Align::Left);
            if active {
                ui.c.bar(rc(x, 102.0, w, 2.5), t.accent);
            }
            ui.hit(rc(x - 8.0, 66.0, w + 16.0, 40.0), Hit::Tab(tab));
            x += w + 30.0;
        }
        ui.c.bar(rc(0.0, 104.5, W, 1.0), t.border);
    }

    fn draw_light(&self, ui: &mut Ui) {
        let t = self.theme;
        let info = self.fx.mode.info();
        let (min, max) = info.colors;

        let y0 = 128.0;
        ui.label("EFFET", M, y0);
        for (i, m) in proto::MODES.iter().enumerate() {
            let r = rc(M + (i % 5) as f32 * 163.0, y0 + 24.0 + (i / 5) as f32 * 48.0, 152.0, 38.0);
            ui.chip(r, short_label(m.mode), self.fx.mode == m.mode, Hit::Mode(m.mode));
        }

        let y1 = y0 + 136.0;
        ui.label("COULEURS", M, y1);
        let row = y1 + 24.0;
        if max == 0 {
            ui.c.text("Cet effet parcourt toutes les couleurs.", rc(M, row, 400.0, 34.0), 13.0, false, t.dim, Align::Left);
        } else {
            let mut x = M;
            for (i, color) in self.fx.colors.iter().enumerate() {
                ui.swatch(x, row, 34.0, *color, false, Hit::Color(i));
                x += 44.0;
            }
            let sand = self.fx.mode == Mode::Quicksand;
            if self.fx.colors.len() < max && !(sand && !self.fx.colors.is_empty()) {
                ui.round_button(x, row, 34.0, "+", Hit::AddColor);
                x += 44.0;
            }
            if self.fx.colors.len() > min {
                ui.round_button(x, row, 34.0, "−", Hit::DelColor);
                x += 44.0;
            }
            if self.fx.colors.is_empty() {
                ui.c.text("Arc-en-ciel par défaut", rc(x + 4.0, row, 200.0, 34.0), 13.0, false, t.dim, Align::Left);
            }
        }
        if info.has_random {
            ui.label("COULEURS ALÉATOIRES", 470.0, y1);
            ui.toggle(470.0, row + 4.0, self.fx.random, Hit::Random);
        }

        let y2 = y1 + 84.0;
        if info.has_speed {
            ui.label("VITESSE", M, y2);
            let speeds = [Speed::Slow, Speed::Medium, Speed::Fast];
            let selected = speeds.iter().position(|s| *s == self.fx.speed);
            ui.segmented(rc(M, y2 + 24.0, 330.0, 36.0), &["Lente", "Moyenne", "Rapide"], selected, |i| Hit::Speed(speeds[i]));
        }
        if info.has_direction {
            ui.label("DIRECTION", 470.0, y2);
            let dirs = self.dirs();
            let labels: Vec<&str> = dirs.iter().map(|(_, l)| *l).collect();
            let selected = dirs.iter().position(|(d, _)| *d == self.fx.direction);
            let width = if dirs.len() == 2 { 140.0 } else { 362.0 };
            ui.segmented(rc(470.0, y2 + 24.0, width, 36.0), &labels, selected, |i| Hit::Dir(dirs[i].0));
        }

        let y3 = y2 + 84.0;
        ui.label("LUMINOSITÉ", M, y3);
        ui.slider(rc(M + 9.0, y3 + 24.0, 690.0, 28.0), self.fx.brightness as f32 / 100.0, Hit::Brightness);
        ui.c.text(&format!("{} %", self.fx.brightness), rc(740.0, y3 + 24.0, W - M - 740.0, 28.0), 13.5, true, t.text, Align::Right);

        ui.primary(rc(M, H - 98.0, 250.0, 42.0), "Enregistrer dans le clavier", Hit::SaveEffect);
        ui.c.text(
            "Les changements s'appliquent en direct. Enregistré, l'effet tourne sans logiciel.",
            rc(M + 268.0, H - 98.0, W - 2.0 * M - 268.0, 42.0),
            12.5,
            false,
            t.dim,
            Align::Left,
        );
    }

    fn draw_keys(&self, ui: &mut Ui) {
        let t = self.theme;
        let y0 = 128.0;
        ui.label("PINCEAU", M, y0);
        for (i, color) in PRESETS.iter().enumerate() {
            ui.swatch(M + 2.0 + i as f32 * 42.0, y0 + 26.0, 30.0, *color, self.brush == *color, Hit::Preset(i));
        }
        let custom_active = !PRESETS.contains(&self.brush);
        let other = rc(M + 2.0 + 10.0 * 42.0 + 8.0, y0 + 26.0, 104.0, 30.0);
        ui.chip(other, "Autre couleur", false, Hit::CustomBrush);
        if custom_active {
            ui.swatch(other.x + other.w + 14.0, y0 + 26.0, 30.0, self.brush, true, Hit::CustomBrush);
        }

        // Clavier : positions en quarts de touche, 10,5 px le quart.
        let (ox, oy, u) = (M + 3.0, y0 + 84.0, 10.5);
        for key in keys::KEYS {
            let (kx, ky, kw, kh) = key.rect;
            if kw == 0 {
                continue; // touche absente de la version ISO
            }
            let r = rc(ox + kx as f32 * u + 1.5, oy + ky as f32 * u + 1.5, kw as f32 * u - 3.0, kh as f32 * u - 3.0);
            let color = self.key_colors.iter().find(|(id, _)| *id == key.id).map(|(_, c)| *c);
            ui.c.fill(r, 5.0, color.map_or(t.key, argb));
            if ui.hover == Some(Hit::Key(key.id)) {
                ui.c.stroke(r, 5.0, t.text, 1.5);
            } else {
                ui.c.stroke(r, 5.0, t.border, 1.0);
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
            // Texte lisible sur n'importe quelle couleur de touche.
            let fg = match color {
                Some(Rgb(r, g, b)) if 299 * r as u32 + 587 * g as u32 + 114 * b as u32 > 140_000 => 0xFF101216,
                Some(_) => 0xFFFFFFFF,
                None => t.dim,
            };
            ui.c.text(&text, r, 10.5, false, fg, Align::Center);
            ui.hit(r, Hit::Key(key.id));
        }

        let by = oy + 252.0 + 18.0;
        ui.chip(rc(M, by, 132.0, 36.0), "Tout remplir", false, Hit::Fill);
        ui.chip(rc(M + 144.0, by, 132.0, 36.0), "Tout éteindre", false, Hit::Clear);
        ui.chip(rc(M + 288.0, by, 132.0, 36.0), "Réappliquer", false, Hit::Reapply);
        ui.c.text(
            "Cliquez ou glissez sur les touches pour les colorer.",
            rc(M + 440.0, by - 4.0, W - 2.0 * M - 440.0, 22.0),
            12.0,
            false,
            t.dim,
            Align::Left,
        );
        ui.c.text(
            "Couleurs perdues en veille : « Réappliquer » les remet.",
            rc(M + 440.0, by + 16.0, W - 2.0 * M - 440.0, 22.0),
            12.0,
            false,
            t.dim,
            Align::Left,
        );
    }

    fn draw_settings(&self, ui: &mut Ui) {
        let t = self.theme;
        let s = self.snap.as_ref();
        let (cx, cw) = (290.0, W - M - 290.0);
        let row = |i: usize| 134.0 + i as f32 * 62.0;
        let name = |ui: &Ui, i: usize, text: &str| {
            ui.c.text(text, rc(M, row(i), 250.0, 36.0), 14.0, false, t.text, Align::Left);
        };

        name(ui, 0, "Profil embarqué");
        ui.segmented(rc(cx, row(0), cw, 36.0), &["Défaut", "1", "2", "3", "4", "5", "6"], s.map(|s| s.profile as usize), |i| {
            Hit::Profile(i as u8)
        });

        name(ui, 1, "Mise en veille après");
        ui.segmented(rc(cx, row(1), cw, 36.0), &["1 min", "2 min", "3 min", "5 min", "10 min", "Jamais"], s.map(|s| s.sleep as usize), |i| {
            Hit::Sleep(i as u8)
        });

        name(ui, 2, "Économie d'énergie");
        ui.segmented(rc(cx, row(2), cw, 36.0), &["Désactivée", "Éteindre l'éclairage", "Baisser la luminosité"], s.map(|s| s.saving as usize), |i| {
            Hit::Saving(i as u8)
        });

        name(ui, 3, "Alerte batterie faible");
        if let Some(low) = self.low_edit.or(s.map(|s| s.low)) {
            ui.slider(rc(cx + 9.0, row(3) + 4.0, cw - 90.0, 28.0), (low as f32 / 50.0).min(1.0), Hit::Low);
            ui.c.text(&format!("{low} %"), rc(cx + cw - 70.0, row(3) + 4.0, 70.0, 28.0), 13.5, true, t.text, Align::Right);
        }

        name(ui, 4, "Éclairage dynamique Windows");
        if let Some(s) = s {
            ui.toggle(cx, row(4) + 5.0, s.wdl, Hit::Wdl);
        }
        ui.c.text(
            "Laisse Windows 11 piloter l'éclairage du clavier lui-même.",
            rc(cx + 62.0, row(4), cw - 62.0, 36.0),
            12.5,
            false,
            t.dim,
            Align::Left,
        );

        name(ui, 5, "Molette");
        let lever = s.and_then(|s| proto::LEVER_CHOICES.get(s.lever as usize)).copied().unwrap_or("—");
        // Majuscule initiale pour l'affichage (les libellés servent aussi en milieu de phrase).
        let mut chars = lever.chars();
        let lever: String = chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default();
        ui.c.text(&lever, rc(cx, row(5), cw, 36.0), 14.0, false, t.dim, Align::Left);
    }
}

// ───────────────────────────── Fenêtre ─────────────────────────────

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

/// Accès à l'état. `None` si l'application n'est pas encore créée ou est déjà empruntée
/// (message reçu pendant une boîte de dialogue modale).
fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|cell| cell.try_borrow_mut().ok().and_then(|mut app| app.as_mut().map(f)))
}

fn run_deferred(hwnd: HWND, deferred: Deferred) {
    let Some(mut custom) = with_app(|a| a.custom) else { return };
    let initial = match &deferred {
        Deferred::PickEffectColor(_, c) | Deferred::PickBrush(c) => (c.0, c.1, c.2),
    };
    // Aucun emprunt de l'état ici : le sélecteur fait tourner sa propre boucle de messages.
    let picked = win::choose_color(hwnd, initial, &mut custom);
    with_app(|a| {
        a.custom = custom;
        let Some((r, g, b)) = picked else { return };
        let color = Rgb(r, g, b);
        match deferred {
            Deferred::PickEffectColor(i, _) => {
                if let Some(slot) = a.fx.colors.get_mut(i) {
                    *slot = color;
                    a.push_effect();
                }
            }
            Deferred::PickBrush(_) => {
                a.custom_brush = color;
                a.brush = color;
            }
        }
        win::redraw(a.hwnd);
    });
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: usize, lparam: isize) -> isize {
    match msg {
        win::WM_PAINT => {
            let painted = with_app(|a| win::paint(hwnd, |c| a.draw(c)));
            if painted.is_none() {
                return win::DefWindowProcW(hwnd, msg, wparam, lparam); // valide la zone à peindre
            }
            0
        }
        win::WM_ERASEBKGND => 1,
        win::WM_LBUTTONDOWN => {
            let (x, y) = win::mouse_pos(hwnd, lparam);
            if let Some(Some(deferred)) = with_app(|a| a.mouse_down(x, y)) {
                run_deferred(hwnd, deferred);
            }
            0
        }
        win::WM_MOUSEMOVE => {
            let (x, y) = win::mouse_pos(hwnd, lparam);
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
            with_app(|a| a.sync());
            0
        }
        win::WM_DPICHANGED => {
            win::apply_suggested_rect(hwnd, lparam);
            win::redraw(hwnd);
            0
        }
        win::WM_DESTROY => {
            win::PostQuitMessage(0);
            0
        }
        _ => win::DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn main() {
    win::init();
    let dark = win::dark_mode();
    let hwnd = win::create_window("scope96", W, H, dark, wndproc);
    if hwnd.is_null() {
        win::error_box("Impossible de créer la fenêtre.");
        return;
    }

    let (tx, rx) = channel();
    let shared = Arc::new(Mutex::new(Shared::default()));
    let worker_shared = Arc::clone(&shared);
    let hwnd_bits = hwnd as usize;
    std::thread::spawn(move || worker(rx, worker_shared, hwnd_bits));

    let app = App {
        hwnd,
        theme: if dark { &DARK } else { &LIGHT },
        tx,
        shared,
        snap: None,
        message: String::new(),
        is_error: false,
        synced: false,
        // `scope96-gui keys` ou `settings` ouvre directement l'onglet voulu.
        tab: match std::env::args().nth(1).as_deref() {
            Some("keys") => Tab::Keys,
            Some("settings") => Tab::Settings,
            _ => Tab::Light,
        },
        fx: {
            let mut fx = Effect::new(Mode::Static);
            fx.colors.push(PRESETS[0]);
            fx
        },
        brush: PRESETS[0],
        custom_brush: Rgb(255, 255, 255),
        key_colors: store::load(),
        legends: keys::layout_legends(),
        low_edit: None,
        hits: Vec::new(),
        hover: None,
        drag: None,
        custom: [0x00FF_FFFF; 16],
    };
    app.send(Cmd::Refresh);
    APP.with(|cell| *cell.borrow_mut() = Some(app));
    win::redraw(hwnd);
    win::set_timer(hwnd, 1, REFRESH_MS);
    win::run_loop();
}
