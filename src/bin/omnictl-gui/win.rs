//! Liaisons Windows minimales pour l'interface : fenêtre (user32), tampon hors écran (gdi32),
//! dessin lissé (GDI+), thème (dwmapi, registre).
//!
//! Tout est dessiné par le programme : pas de contrôles Windows, donc un rendu identique en
//! thème clair et sombre, et aucune bibliothèque graphique à embarquer.

#![allow(non_snake_case, non_camel_case_types, clippy::upper_case_acronyms, clippy::too_many_arguments)]

use std::ffi::c_void;
use std::ptr::{null, null_mut};

pub type HANDLE = *mut c_void;
pub type HWND = *mut c_void;
pub type HDC = *mut c_void;
pub type WNDPROC = unsafe extern "system" fn(HWND, u32, usize, isize) -> isize;

pub const WM_DESTROY: u32 = 0x0002;
pub const WM_SIZE: u32 = 0x0005;
pub const WM_SETTINGCHANGE: u32 = 0x001A;
pub const WM_PAINT: u32 = 0x000F;
pub const WM_ERASEBKGND: u32 = 0x0014;
pub const WM_TIMER: u32 = 0x0113;
pub const WM_MOUSEMOVE: u32 = 0x0200;
pub const WM_LBUTTONDOWN: u32 = 0x0201;
pub const WM_LBUTTONUP: u32 = 0x0202;
pub const WM_DPICHANGED: u32 = 0x02E0;
pub const WM_APP: u32 = 0x8000;

const WS_CAPTION: u32 = 0x00C0_0000;
const WS_SYSMENU: u32 = 0x0008_0000;
const WS_MINIMIZEBOX: u32 = 0x0002_0000;
const WS_THICKFRAME: u32 = 0x0004_0000;
const WS_MAXIMIZEBOX: u32 = 0x0001_0000;
/// Fenêtre ordinaire : redimensionnable, agrandissable en plein écran, réductible.
pub const WINDOW_STYLE: u32 = WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX | WS_THICKFRAME;
const CW_USEDEFAULT: i32 = 0x8000_0000u32 as i32;
const SRCCOPY: u32 = 0x00CC_0020;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct RECT {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[repr(C)]
struct WNDCLASSEXW {
    cbSize: u32,
    style: u32,
    lpfnWndProc: WNDPROC,
    cbClsExtra: i32,
    cbWndExtra: i32,
    hInstance: HANDLE,
    hIcon: HANDLE,
    hCursor: HANDLE,
    hbrBackground: HANDLE,
    lpszMenuName: *const u16,
    lpszClassName: *const u16,
    hIconSm: HANDLE,
}

#[repr(C)]
struct MSG {
    hwnd: HWND,
    message: u32,
    wParam: usize,
    lParam: isize,
    time: u32,
    pt: [i32; 2],
}

#[repr(C)]
struct PAINTSTRUCT {
    hdc: HDC,
    fErase: i32,
    rcPaint: RECT,
    fRestore: i32,
    fIncUpdate: i32,
    rgbReserved: [u8; 32],
}

#[repr(C)]
struct GdiplusStartupInput {
    GdiplusVersion: u32,
    DebugEventCallback: *const c_void,
    SuppressBackgroundThread: i32,
    SuppressExternalCodecs: i32,
}

#[repr(C)]
struct RectF {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

#[link(name = "user32")]
extern "system" {
    fn RegisterClassExW(class: *const WNDCLASSEXW) -> u16;
    fn CreateWindowExW(
        ex_style: u32,
        class: *const u16,
        title: *const u16,
        style: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        parent: HWND,
        menu: HANDLE,
        instance: HANDLE,
        param: *mut c_void,
    ) -> HWND;
    pub fn DefWindowProcW(hwnd: HWND, msg: u32, wparam: usize, lparam: isize) -> isize;
    fn GetMessageW(msg: *mut MSG, hwnd: HWND, min: u32, max: u32) -> i32;
    fn TranslateMessage(msg: *const MSG) -> i32;
    fn DispatchMessageW(msg: *const MSG) -> isize;
    pub fn PostQuitMessage(code: i32);
    fn PostMessageW(hwnd: HWND, msg: u32, wparam: usize, lparam: isize) -> i32;
    fn BeginPaint(hwnd: HWND, ps: *mut PAINTSTRUCT) -> HDC;
    fn EndPaint(hwnd: HWND, ps: *const PAINTSTRUCT) -> i32;
    fn InvalidateRect(hwnd: HWND, rect: *const RECT, erase: i32) -> i32;
    fn GetClientRect(hwnd: HWND, rect: *mut RECT) -> i32;
    fn SetCapture(hwnd: HWND) -> HWND;
    fn ReleaseCapture() -> i32;
    fn LoadCursorW(instance: HANDLE, name: *const u16) -> HANDLE;
    fn SetTimer(hwnd: HWND, id: usize, ms: u32, callback: *const c_void) -> usize;
    fn GetDpiForSystem() -> u32;
    fn GetDpiForWindow(hwnd: HWND) -> u32;
    fn SetProcessDpiAwarenessContext(context: isize) -> i32;
    fn AdjustWindowRectExForDpi(rect: *mut RECT, style: u32, menu: i32, ex_style: u32, dpi: u32) -> i32;
    fn ShowWindow(hwnd: HWND, cmd: i32) -> i32;
    fn SetWindowPos(hwnd: HWND, after: HWND, x: i32, y: i32, w: i32, h: i32, flags: u32) -> i32;
    fn MessageBoxW(hwnd: HWND, text: *const u16, caption: *const u16, kind: u32) -> i32;
}

#[link(name = "gdi32")]
extern "system" {
    fn CreateCompatibleDC(hdc: HDC) -> HDC;
    fn CreateCompatibleBitmap(hdc: HDC, w: i32, h: i32) -> HANDLE;
    fn SelectObject(hdc: HDC, object: HANDLE) -> HANDLE;
    fn BitBlt(dst: HDC, x: i32, y: i32, w: i32, h: i32, src: HDC, sx: i32, sy: i32, rop: u32) -> i32;
    fn DeleteObject(object: HANDLE) -> i32;
    fn DeleteDC(hdc: HDC) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(name: *const u16) -> HANDLE;
}

#[link(name = "dwmapi")]
extern "system" {
    fn DwmSetWindowAttribute(hwnd: HWND, attribute: u32, value: *const c_void, size: u32) -> i32;
}

#[link(name = "advapi32")]
extern "system" {
    fn RegGetValueW(
        key: HANDLE,
        subkey: *const u16,
        value: *const u16,
        flags: u32,
        kind: *mut u32,
        data: *mut c_void,
        size: *mut u32,
    ) -> i32;
}

#[link(name = "gdiplus")]
extern "system" {
    fn GdiplusStartup(token: *mut usize, input: *const GdiplusStartupInput, output: *mut c_void) -> i32;
    fn GdipCreateFromHDC(hdc: HDC, graphics: *mut *mut c_void) -> i32;
    fn GdipDeleteGraphics(graphics: *mut c_void) -> i32;
    fn GdipSetSmoothingMode(graphics: *mut c_void, mode: i32) -> i32;
    fn GdipSetTextRenderingHint(graphics: *mut c_void, hint: i32) -> i32;
    fn GdipGraphicsClear(graphics: *mut c_void, color: u32) -> i32;
    fn GdipCreateSolidFill(color: u32, brush: *mut *mut c_void) -> i32;
    fn GdipDeleteBrush(brush: *mut c_void) -> i32;
    fn GdipCreatePen1(color: u32, width: f32, unit: i32, pen: *mut *mut c_void) -> i32;
    fn GdipDeletePen(pen: *mut c_void) -> i32;
    fn GdipCreatePath(fill_mode: i32, path: *mut *mut c_void) -> i32;
    fn GdipDeletePath(path: *mut c_void) -> i32;
    fn GdipAddPathArc(path: *mut c_void, x: f32, y: f32, w: f32, h: f32, start: f32, sweep: f32) -> i32;
    fn GdipClosePathFigure(path: *mut c_void) -> i32;
    fn GdipFillPath(graphics: *mut c_void, brush: *mut c_void, path: *mut c_void) -> i32;
    fn GdipDrawPath(graphics: *mut c_void, pen: *mut c_void, path: *mut c_void) -> i32;
    fn GdipCreateLineBrushFromRect(
        rect: *const RectF,
        from: u32,
        to: u32,
        mode: i32,
        wrap: i32,
        brush: *mut *mut c_void,
    ) -> i32;
    fn GdipFillRectangle(graphics: *mut c_void, brush: *mut c_void, x: f32, y: f32, w: f32, h: f32) -> i32;
    fn GdipCreateFontFamilyFromName(name: *const u16, collection: *mut c_void, family: *mut *mut c_void) -> i32;
    fn GdipDeleteFontFamily(family: *mut c_void) -> i32;
    fn GdipCreateFont(family: *mut c_void, size: f32, style: i32, unit: i32, font: *mut *mut c_void) -> i32;
    fn GdipDeleteFont(font: *mut c_void) -> i32;
    fn GdipCreateStringFormat(flags: i32, language: u16, format: *mut *mut c_void) -> i32;
    fn GdipDeleteStringFormat(format: *mut c_void) -> i32;
    fn GdipSetStringFormatAlign(format: *mut c_void, align: i32) -> i32;
    fn GdipSetStringFormatLineAlign(format: *mut c_void, align: i32) -> i32;
    fn GdipSetStringFormatTrimming(format: *mut c_void, trimming: i32) -> i32;
    fn GdipMeasureString(
        graphics: *mut c_void,
        text: *const u16,
        length: i32,
        font: *mut c_void,
        layout: *const RectF,
        format: *mut c_void,
        bounds: *mut RectF,
        codepoints: *mut i32,
        lines: *mut i32,
    ) -> i32;
    fn GdipDrawString(
        graphics: *mut c_void,
        text: *const u16,
        length: i32,
        font: *mut c_void,
        layout: *const RectF,
        format: *mut c_void,
        brush: *mut c_void,
    ) -> i32;
}

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Rectangle en pixels logiques (avant mise à l'échelle DPI).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rc {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

pub const fn rc(x: f32, y: f32, w: f32, h: f32) -> Rc {
    Rc { x, y, w, h }
}

impl Rc {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// Surface de dessin lissée. Coordonnées logiques, mises à l'échelle et décalées au tracé
/// (voir [`view`]).
pub struct Canvas {
    g: *mut c_void,
    family: *mut c_void,
    /// Police d'icônes de Windows (Segoe Fluent Icons, ou MDL2 sur Windows 10).
    icons: *mut c_void,
    scale: f32,
    ox: f32,
    oy: f32,
}

impl Canvas {
    fn new(hdc: HDC, (scale, ox, oy): (f32, f32, f32)) -> Canvas {
        let mut g = null_mut();
        unsafe {
            GdipCreateFromHDC(hdc, &mut g);
            GdipSetSmoothingMode(g, 4); // AntiAlias
            // Lissage sans calage sur la grille : avec GDI+, le calage dérègle l'espacement des lettres.
            GdipSetTextRenderingHint(g, 4);
        }
        let family = first_family(&["Segoe UI Variable Text", "Segoe UI"]);
        let icons = first_family(&["Segoe Fluent Icons", "Segoe MDL2 Assets"]);
        Canvas { g, family, icons, scale, ox, oy }
    }

    pub fn clear(&self, color: u32) {
        unsafe { GdipGraphicsClear(self.g, color) };
    }

    fn with_brush(&self, color: u32, f: impl FnOnce(*mut c_void)) {
        let mut brush = null_mut();
        unsafe {
            GdipCreateSolidFill(color, &mut brush);
            f(brush);
            GdipDeleteBrush(brush);
        }
    }

    /// Chemin d'un rectangle arrondi, en pixels physiques.
    fn round_path(&self, r: Rc, radius: f32) -> *mut c_void {
        let s = self.scale;
        let (x, y, w, h) = (r.x * s + self.ox, r.y * s + self.oy, r.w * s, r.h * s);
        let d = (radius * 2.0 * s).min(w).min(h).max(0.01);
        let mut path = null_mut();
        unsafe {
            GdipCreatePath(0, &mut path);
            GdipAddPathArc(path, x, y, d, d, 180.0, 90.0);
            GdipAddPathArc(path, x + w - d, y, d, d, 270.0, 90.0);
            GdipAddPathArc(path, x + w - d, y + h - d, d, d, 0.0, 90.0);
            GdipAddPathArc(path, x, y + h - d, d, d, 90.0, 90.0);
            GdipClosePathFigure(path);
        }
        path
    }

    pub fn fill(&self, r: Rc, radius: f32, color: u32) {
        let path = self.round_path(r, radius);
        self.with_brush(color, |brush| unsafe {
            GdipFillPath(self.g, brush, path);
        });
        unsafe { GdipDeletePath(path) };
    }

    pub fn stroke(&self, r: Rc, radius: f32, color: u32, width: f32) {
        let path = self.round_path(r, radius);
        let mut pen = null_mut();
        unsafe {
            GdipCreatePen1(color, width * self.scale, 2, &mut pen); // UnitPixel
            GdipDrawPath(self.g, pen, path);
            GdipDeletePen(pen);
            GdipDeletePath(path);
        }
    }

    /// Rectangle plein à angles vifs (séparateurs, soulignés).
    pub fn bar(&self, r: Rc, color: u32) {
        let s = self.scale;
        self.with_brush(color, |brush| unsafe {
            GdipFillRectangle(self.g, brush, r.x * s + self.ox, r.y * s + self.oy, r.w * s, r.h * s);
        });
    }

    /// Dégradé linéaire entre deux couleurs, horizontal ou vertical.
    pub fn gradient(&self, r: Rc, from: u32, to: u32, vertical: bool) {
        let s = self.scale;
        let rect = RectF { x: r.x * s + self.ox, y: r.y * s + self.oy, w: r.w * s, h: r.h * s };
        let mut brush = null_mut();
        unsafe {
            // Mode « mosaïque en miroir » : pas de liseré d'une autre couleur sur les bords.
            GdipCreateLineBrushFromRect(&rect, from, to, vertical as i32, 3, &mut brush);
            GdipFillRectangle(self.g, brush, rect.x, rect.y, rect.w, rect.h);
            GdipDeleteBrush(brush);
        }
    }

    /// Largeur d'un texte en pixels logiques.
    pub fn text_width(&self, text: &str, size: f32, bold: bool) -> f32 {
        let s = self.scale;
        let w = wide(text);
        let layout = RectF { x: 0.0, y: 0.0, w: 10_000.0, h: 1_000.0 };
        let mut bounds = RectF { x: 0.0, y: 0.0, w: 0.0, h: 0.0 };
        let mut font = null_mut();
        let mut format = null_mut();
        unsafe {
            GdipCreateFont(self.family, size * s, bold as i32, 2, &mut font);
            GdipCreateStringFormat(0x1000, 0, &mut format);
            GdipMeasureString(
                self.g,
                w.as_ptr(),
                w.len() as i32 - 1,
                font,
                &layout,
                format,
                &mut bounds,
                null_mut(),
                null_mut(),
            );
            GdipDeleteStringFormat(format);
            GdipDeleteFont(font);
        }
        bounds.w / s
    }

    pub fn text(&self, text: &str, r: Rc, size: f32, bold: bool, color: u32, align: Align) {
        self.string(self.family, text, r, size, bold, color, align);
    }

    /// Dessine un glyphe de la police d'icônes, centré dans `r`.
    pub fn icon(&self, glyph: char, r: Rc, size: f32, color: u32) {
        let mut buf = [0u8; 4];
        self.string(self.icons, glyph.encode_utf8(&mut buf), r, size, false, color, Align::Center);
    }

    fn string(&self, family: *mut c_void, text: &str, r: Rc, size: f32, bold: bool, color: u32, align: Align) {
        let s = self.scale;
        let w = wide(text);
        let layout = RectF { x: r.x * s + self.ox, y: r.y * s + self.oy, w: r.w * s, h: r.h * s };
        let mut font = null_mut();
        let mut format = null_mut();
        unsafe {
            GdipCreateFont(family, size * s, bold as i32, 2, &mut font); // UnitPixel
            GdipCreateStringFormat(0x1000, 0, &mut format); // NoWrap
            GdipSetStringFormatAlign(
                format,
                match align {
                    Align::Left => 0,
                    Align::Center => 1,
                    Align::Right => 2,
                },
            );
            GdipSetStringFormatLineAlign(format, 1); // centré verticalement
            GdipSetStringFormatTrimming(format, 3); // points de suspension
        }
        self.with_brush(color, |brush| unsafe {
            GdipDrawString(self.g, w.as_ptr(), w.len() as i32 - 1, font, &layout, format, brush);
        });
        unsafe {
            GdipDeleteStringFormat(format);
            GdipDeleteFont(font);
        }
    }
}

impl Drop for Canvas {
    fn drop(&mut self) {
        unsafe {
            GdipDeleteFontFamily(self.family);
            GdipDeleteFontFamily(self.icons);
            GdipDeleteGraphics(self.g);
        }
    }
}

/// Première police installée parmi `names`.
fn first_family(names: &[&str]) -> *mut c_void {
    for name in names {
        let mut family = null_mut();
        let status = unsafe { GdipCreateFontFamilyFromName(wide(name).as_ptr(), null_mut(), &mut family) };
        if status == 0 && !family.is_null() {
            return family;
        }
    }
    null_mut()
}

/// Couleur d'accentuation choisie par l'utilisateur dans Windows (bleu par défaut).
pub fn accent() -> (u8, u8, u8) {
    let mut value = 0u32;
    let mut size = 4u32;
    let status = unsafe {
        RegGetValueW(
            0x8000_0001usize as HANDLE, // HKEY_CURRENT_USER
            wide("Software\\Microsoft\\Windows\\DWM").as_ptr(),
            wide("AccentColor").as_ptr(),
            0x10, // RRF_RT_REG_DWORD
            null_mut(),
            &mut value as *mut u32 as *mut c_void,
            &mut size,
        )
    };
    if status != 0 {
        return (0x00, 0x5F, 0xB8);
    }
    // Stockée en 0xAABBGGRR.
    (value as u8, (value >> 8) as u8, (value >> 16) as u8)
}

/// À appeler une fois au démarrage, avant toute création de fenêtre.
pub fn init() {
    unsafe {
        SetProcessDpiAwarenessContext(-4); // PER_MONITOR_AWARE_V2
        let input = GdiplusStartupInput {
            GdiplusVersion: 1,
            DebugEventCallback: null(),
            SuppressBackgroundThread: 0,
            SuppressExternalCodecs: 0,
        };
        let mut token = 0usize;
        GdiplusStartup(&mut token, &input, null_mut());
    }
}

/// Windows est-il réglé sur le thème sombre pour les applications ?
pub fn dark_mode() -> bool {
    let mut value = 1u32;
    let mut size = 4u32;
    let status = unsafe {
        RegGetValueW(
            0x8000_0001usize as HANDLE, // HKEY_CURRENT_USER
            wide("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize").as_ptr(),
            wide("AppsUseLightTheme").as_ptr(),
            0x10, // RRF_RT_REG_DWORD
            null_mut(),
            &mut value as *mut u32 as *mut c_void,
            &mut size,
        )
    };
    status == 0 && value == 0
}

/// Crée la fenêtre principale, de taille cliente `w` × `h` en pixels logiques.
pub fn create_window(title: &str, w: f32, h: f32, dark: bool, visible: bool, proc: WNDPROC) -> HWND {
    unsafe {
        let instance = GetModuleHandleW(null());
        let class = wide("omnictl");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: proc,
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: LoadIconW(instance, 1 as *const u16), // icône incorporée par build.rs
            hCursor: LoadCursorW(null_mut(), 32512 as *const u16), // IDC_ARROW
            hbrBackground: null_mut(),
            lpszMenuName: null(),
            lpszClassName: class.as_ptr(),
            hIconSm: LoadIconW(instance, 1 as *const u16),
        };
        RegisterClassExW(&wc);
        let dpi = GetDpiForSystem();
        let (cw, ch) = window_size(w, h, dpi);
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            wide(title).as_ptr(),
            WINDOW_STYLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            cw,
            ch,
            null_mut(),
            null_mut(),
            instance,
            null_mut(),
        );
        if hwnd.is_null() {
            return hwnd;
        }
        // L'écran d'arrivée peut avoir un autre DPI que l'écran principal.
        let actual = GetDpiForWindow(hwnd);
        if actual != dpi {
            let (cw, ch) = window_size(w, h, actual);
            SetWindowPos(hwnd, null_mut(), 0, 0, cw, ch, 0x0002 | 0x0004); // NOMOVE | NOZORDER
        }
        set_title_dark(hwnd, dark);
        if visible {
            ShowWindow(hwnd, 5);
        }
        hwnd
    }
}

fn window_size(w: f32, h: f32, dpi: u32) -> (i32, i32) {
    let s = dpi as f32 / 96.0;
    let mut r = RECT { left: 0, top: 0, right: (w * s).round() as i32, bottom: (h * s).round() as i32 };
    unsafe { AdjustWindowRectExForDpi(&mut r, WINDOW_STYLE, 0, 0, dpi) };
    (r.right - r.left, r.bottom - r.top)
}

/// Cadrage du contenu logique `w` × `h` dans la fenêtre : le plus grand possible sans
/// déformation, centré. Renvoie (échelle, décalage x, décalage y) en pixels physiques.
pub fn view(hwnd: HWND, w: f32, h: f32) -> (f32, f32, f32) {
    let mut client = RECT::default();
    unsafe { GetClientRect(hwnd, &mut client) };
    let (cw, ch) = (client.right.max(1) as f32, client.bottom.max(1) as f32);
    let scale = (cw / w).min(ch / h);
    (scale, ((cw - w * scale) / 2.0).round(), ((ch - h * scale) / 2.0).round())
}

/// Applique le thème clair ou sombre à la barre de titre.
pub fn set_title_dark(hwnd: HWND, dark: bool) {
    let flag = dark as i32;
    unsafe { DwmSetWindowAttribute(hwnd, 20, &flag as *const i32 as *const c_void, 4) };
}

/// Réponse à WM_DPICHANGED : adopte le rectangle proposé par Windows.
pub fn apply_suggested_rect(hwnd: HWND, lparam: isize) {
    unsafe {
        let r = *(lparam as *const RECT);
        SetWindowPos(hwnd, null_mut(), r.left, r.top, r.right - r.left, r.bottom - r.top, 0x0004);
    }
}

pub fn run_loop() {
    unsafe {
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

pub fn redraw(hwnd: HWND) {
    unsafe { InvalidateRect(hwnd, null(), 0) };
}

pub fn capture(hwnd: HWND) {
    unsafe { SetCapture(hwnd) };
}

pub fn release_capture() {
    unsafe { ReleaseCapture() };
}

pub fn set_timer(hwnd: HWND, id: usize, ms: u32) {
    unsafe { SetTimer(hwnd, id, ms, null()) };
}

/// Poste un message à la fenêtre depuis n'importe quel fil d'exécution.
pub fn post(hwnd_bits: usize, msg: u32) {
    unsafe { PostMessageW(hwnd_bits as HWND, msg, 0, 0) };
}

pub fn error_box(text: &str) {
    unsafe { MessageBoxW(null_mut(), wide(text).as_ptr(), wide("omnictl").as_ptr(), 0x10) };
}

/// Position du curseur d'un message souris, en pixels logiques du contenu `w` × `h`.
pub fn mouse_pos(hwnd: HWND, lparam: isize, w: f32, h: f32) -> (f32, f32) {
    let (s, ox, oy) = view(hwnd, w, h);
    let x = (lparam & 0xFFFF) as u16 as i16 as f32;
    let y = ((lparam >> 16) & 0xFFFF) as u16 as i16 as f32;
    ((x - ox) / s, (y - oy) / s)
}

/// Peint la fenêtre dans un tampon hors écran (pas de scintillement), puis le recopie.
/// `lw` × `lh` est la taille logique du contenu, cadré par [`view`].
pub fn paint(hwnd: HWND, lw: f32, lh: f32, draw: impl FnOnce(&Canvas)) {
    unsafe {
        let mut ps: PAINTSTRUCT = std::mem::zeroed();
        let hdc = BeginPaint(hwnd, &mut ps);
        let mut client = RECT::default();
        GetClientRect(hwnd, &mut client);
        let (w, h) = (client.right.max(1), client.bottom.max(1));
        let mem = CreateCompatibleDC(hdc);
        let bitmap = CreateCompatibleBitmap(hdc, w, h);
        let previous = SelectObject(mem, bitmap);
        {
            let canvas = Canvas::new(mem, view(hwnd, lw, lh));
            draw(&canvas);
        } // le Canvas est libéré avant le contexte qu'il dessine
        BitBlt(hdc, 0, 0, w, h, mem, 0, 0, SRCCOPY);
        SelectObject(mem, previous);
        DeleteObject(bitmap);
        DeleteDC(mem);
        EndPaint(hwnd, &ps);
    }
}

// ───────────── Arrière-plan : icône de notification, menu, démarrage avec Windows ─────────────

pub const WM_CLOSE: u32 = 0x0010;
/// Changement d'état de la session (verrouillage, déverrouillage…).
pub const WM_WTSSESSION_CHANGE: u32 = 0x02B1;
pub const WTS_SESSION_LOCK: usize = 7;
pub const WTS_SESSION_UNLOCK: usize = 8;
pub const WM_RBUTTONUP: u32 = 0x0205;

#[repr(C)]
struct NOTIFYICONDATAW {
    cbSize: u32,
    hWnd: HWND,
    uID: u32,
    uFlags: u32,
    uCallbackMessage: u32,
    hIcon: HANDLE,
    szTip: [u16; 128],
    dwState: u32,
    dwStateMask: u32,
    szInfo: [u16; 256],
    uVersion: u32,
    szInfoTitle: [u16; 64],
    dwInfoFlags: u32,
    guidItem: [u8; 16],
    hBalloonIcon: HANDLE,
}

#[repr(C)]
struct LASTINPUTINFO {
    cbSize: u32,
    dwTime: u32,
}

#[link(name = "user32")]
extern "system" {
    fn GetLastInputInfo(info: *mut LASTINPUTINFO) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetTickCount() -> u32;
}

#[link(name = "wtsapi32")]
extern "system" {
    fn WTSRegisterSessionNotification(hwnd: HWND, flags: u32) -> i32;
}

/// Secondes écoulées depuis la dernière action de l'utilisateur (clavier ou souris).
pub fn idle_seconds() -> u32 {
    let mut info = LASTINPUTINFO { cbSize: 8, dwTime: 0 };
    unsafe {
        if GetLastInputInfo(&mut info) == 0 {
            return 0;
        }
        GetTickCount().wrapping_sub(info.dwTime) / 1000
    }
}

/// Demande à recevoir `WM_WTSSESSION_CHANGE` quand la session est verrouillée ou rouverte.
pub fn watch_session(hwnd: HWND) {
    unsafe { WTSRegisterSessionNotification(hwnd, 0) }; // cette session seulement
}

#[link(name = "shell32")]
extern "system" {
    fn Shell_NotifyIconW(message: u32, data: *const NOTIFYICONDATAW) -> i32;
}

#[link(name = "user32")]
extern "system" {
    fn LoadIconW(instance: HANDLE, name: *const u16) -> HANDLE;
    fn CreatePopupMenu() -> HANDLE;
    fn AppendMenuW(menu: HANDLE, flags: u32, id: usize, text: *const u16) -> i32;
    fn TrackPopupMenu(menu: HANDLE, flags: u32, x: i32, y: i32, reserved: i32, hwnd: HWND, rect: *const RECT) -> i32;
    fn DestroyMenu(menu: HANDLE) -> i32;
    fn GetCursorPos(point: *mut [i32; 2]) -> i32;
    fn SetForegroundWindow(hwnd: HWND) -> i32;
    fn DestroyWindow(hwnd: HWND) -> i32;
}

#[link(name = "advapi32")]
extern "system" {
    fn RegSetKeyValueW(key: HANDLE, subkey: *const u16, value: *const u16, kind: u32, data: *const c_void, size: u32) -> i32;
    fn RegDeleteKeyValueW(key: HANDLE, subkey: *const u16, value: *const u16) -> i32;
}

fn tray_data(hwnd: HWND, callback: u32, tip: &str) -> NOTIFYICONDATAW {
    let mut data: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = hwnd;
    data.uID = 1;
    data.uFlags = 0x1 | 0x2 | 0x4; // MESSAGE | ICON | TIP
    data.uCallbackMessage = callback;
    data.hIcon = unsafe { LoadIconW(GetModuleHandleW(null()), 1 as *const u16) }; // icône de l'application
    for (slot, unit) in data.szTip.iter_mut().zip(tip.encode_utf16().take(127)) {
        *slot = unit;
    }
    data
}

/// Ajoute l'icône dans la zone de notification. Les clics arrivent à la fenêtre sous la
/// forme du message `callback`, avec le message souris d'origine dans `lparam`.
pub fn tray_add(hwnd: HWND, callback: u32, tip: &str) {
    unsafe { Shell_NotifyIconW(0, &tray_data(hwnd, callback, tip)) }; // NIM_ADD
}

pub fn tray_remove(hwnd: HWND) {
    unsafe { Shell_NotifyIconW(2, &tray_data(hwnd, 0, "")) }; // NIM_DELETE
}

/// Menu contextuel au niveau du curseur. Renvoie l'index de l'entrée choisie.
/// Boucle modale : ne pas appeler pendant un emprunt de l'état de l'application.
pub fn popup_menu(hwnd: HWND, items: &[&str]) -> Option<usize> {
    unsafe {
        let menu = CreatePopupMenu();
        for (i, item) in items.iter().enumerate() {
            AppendMenuW(menu, 0, i + 1, wide(item).as_ptr());
        }
        let mut pos = [0i32; 2];
        GetCursorPos(&mut pos);
        SetForegroundWindow(hwnd); // sans cela le menu ne se referme pas au clic ailleurs
        let choice = TrackPopupMenu(menu, 0x0002 | 0x0100, pos[0], pos[1], 0, hwnd, null()); // RIGHTBUTTON | RETURNCMD
        DestroyMenu(menu);
        (choice > 0).then(|| choice as usize - 1)
    }
}

pub fn show_window(hwnd: HWND, visible: bool) {
    unsafe {
        ShowWindow(hwnd, if visible { 5 } else { 0 });
        if visible {
            SetForegroundWindow(hwnd);
        }
    }
}

pub fn destroy_window(hwnd: HWND) {
    unsafe { DestroyWindow(hwnd) };
}

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// Inscrit (ou retire) le lancement de `command` à l'ouverture de session de l'utilisateur.
pub fn set_autostart(name: &str, command: Option<&str>) -> bool {
    let hkcu = 0x8000_0001usize as HANDLE;
    let status = unsafe {
        match command {
            Some(command) => {
                let data = wide(command);
                RegSetKeyValueW(
                    hkcu,
                    wide(RUN_KEY).as_ptr(),
                    wide(name).as_ptr(),
                    1, // REG_SZ
                    data.as_ptr() as *const c_void,
                    (data.len() * 2) as u32,
                )
            }
            None => match RegDeleteKeyValueW(hkcu, wide(RUN_KEY).as_ptr(), wide(name).as_ptr()) {
                2 => 0, // valeur déjà absente
                other => other,
            },
        }
    };
    status == 0
}

// ───────────── Application au premier plan ─────────────

#[link(name = "user32")]
extern "system" {
    fn GetForegroundWindow() -> HWND;
    fn GetWindowThreadProcessId(hwnd: HWND, pid: *mut u32) -> u32;
}

#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> HANDLE;
    fn QueryFullProcessImageNameW(process: HANDLE, flags: u32, name: *mut u16, size: *mut u32) -> i32;
    fn CloseHandle(handle: HANDLE) -> i32;
}

/// Nom de l'exécutable (en minuscules, sans chemin) de la fenêtre au premier plan.
/// `None` s'il n'y en a pas ou si Windows ne laisse pas l'interroger.
pub fn foreground_exe() -> Option<String> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        let process = OpenProcess(0x1000, 0, pid); // PROCESS_QUERY_LIMITED_INFORMATION
        if process.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(process, 0, buf.as_mut_ptr(), &mut len);
        CloseHandle(process);
        if ok == 0 {
            return None;
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        let name = path.rsplit(['\\', '/']).next()?.to_lowercase();
        (!name.is_empty()).then_some(name)
    }
}

// ───────────── Instance unique, téléchargement, empreinte ─────────────

#[link(name = "kernel32")]
extern "system" {
    fn CreateMutexW(security: *const c_void, owner: i32, name: *const u16) -> HANDLE;
    fn GetLastError() -> u32;
}

#[link(name = "user32")]
extern "system" {
    fn FindWindowW(class: *const u16, title: *const u16) -> HWND;
}

#[link(name = "urlmon")]
extern "system" {
    fn URLDownloadToFileW(caller: *mut c_void, url: *const u16, file: *const u16, reserved: u32, callback: *mut c_void) -> i32;
}

#[link(name = "wininet")]
extern "system" {
    fn DeleteUrlCacheEntryW(url: *const u16) -> i32;
}

#[link(name = "bcrypt")]
extern "system" {
    fn BCryptOpenAlgorithmProvider(handle: *mut HANDLE, algorithm: *const u16, implementation: *const u16, flags: u32) -> i32;
    fn BCryptHash(algorithm: HANDLE, secret: *mut u8, secret_len: u32, input: *const u8, input_len: u32, output: *mut u8, output_len: u32) -> i32;
    fn BCryptCloseAlgorithmProvider(handle: HANDLE, flags: u32) -> i32;
}

/// Vrai si omnictl tourne déjà dans cette session ; sa fenêtre reçoit alors `show_msg`
/// pour venir au premier plan. Le verrou vit aussi longtemps que le processus.
pub fn already_running(show_msg: u32) -> bool {
    unsafe {
        CreateMutexW(null(), 0, wide("Local\\omnictl-gui").as_ptr());
        if GetLastError() != 183 {
            return false; // ERROR_ALREADY_EXISTS
        }
        let existing = FindWindowW(wide("omnictl").as_ptr(), null());
        if !existing.is_null() {
            PostMessageW(existing, show_msg, 0, 0);
        }
        true
    }
}

/// Télécharge `url` dans le fichier `to` (HTTPS, réglages réseau de Windows).
pub fn download(url: &str, to: &std::path::Path) -> bool {
    let url = wide(url);
    let Some(path) = to.to_str() else { return false };
    unsafe {
        DeleteUrlCacheEntryW(url.as_ptr()); // toujours la version en ligne, pas une copie en cache
        URLDownloadToFileW(null_mut(), url.as_ptr(), wide(path).as_ptr(), 0, null_mut()) == 0
    }
}

/// Empreinte SHA-256, calculée par Windows.
pub fn sha256(data: &[u8]) -> Option<[u8; 32]> {
    let mut out = [0u8; 32];
    unsafe {
        let mut algorithm = null_mut();
        if BCryptOpenAlgorithmProvider(&mut algorithm, wide("SHA256").as_ptr(), null(), 0) != 0 {
            return None;
        }
        let status = BCryptHash(algorithm, null_mut(), 0, data.as_ptr(), data.len() as u32, out.as_mut_ptr(), 32);
        BCryptCloseAlgorithmProvider(algorithm, 0);
        (status == 0).then_some(out)
    }
}

// ───────────── Installation : raccourci du menu Démarrer, registre, confirmation ─────────────

#[repr(C)]
struct ComGuid {
    d1: u32,
    d2: u16,
    d3: u16,
    d4: [u8; 8],
}

const CLSID_SHELL_LINK: ComGuid = ComGuid { d1: 0x0002_1401, d2: 0, d3: 0, d4: [0xC0, 0, 0, 0, 0, 0, 0, 0x46] };
const IID_SHELL_LINK_W: ComGuid = ComGuid { d1: 0x0002_14F9, d2: 0, d3: 0, d4: [0xC0, 0, 0, 0, 0, 0, 0, 0x46] };
const IID_PERSIST_FILE: ComGuid = ComGuid { d1: 0x0000_010B, d2: 0, d3: 0, d4: [0xC0, 0, 0, 0, 0, 0, 0, 0x46] };

#[link(name = "ole32")]
extern "system" {
    fn CoInitializeEx(reserved: *mut c_void, model: u32) -> i32;
    fn CoCreateInstance(class: *const ComGuid, outer: *mut c_void, context: u32, iid: *const ComGuid, out: *mut *mut c_void) -> i32;
}

#[link(name = "advapi32")]
extern "system" {
    fn RegDeleteTreeW(key: HANDLE, subkey: *const u16) -> i32;
}

/// Appelle la méthode n° `index` d'un objet COM (table de fonctions en tête de l'objet).
unsafe fn com_method(object: *mut c_void, index: usize) -> *const c_void {
    *(*(object as *const *const *const c_void)).add(index)
}

/// Crée un raccourci `.lnk` vers `target` (interfaces IShellLinkW et IPersistFile).
pub fn create_shortcut(link: &std::path::Path, target: &std::path::Path, description: &str) -> bool {
    type SetText = unsafe extern "system" fn(*mut c_void, *const u16) -> i32;
    type Query = unsafe extern "system" fn(*mut c_void, *const ComGuid, *mut *mut c_void) -> i32;
    type Release = unsafe extern "system" fn(*mut c_void) -> u32;
    type Save = unsafe extern "system" fn(*mut c_void, *const u16, i32) -> i32;
    let (Some(link), Some(target)) = (link.to_str(), target.to_str()) else { return false };
    unsafe {
        CoInitializeEx(null_mut(), 2); // appartement à un seul fil ; sans effet si déjà fait
        let mut shell_link = null_mut();
        if CoCreateInstance(&CLSID_SHELL_LINK, null_mut(), 1, &IID_SHELL_LINK_W, &mut shell_link) != 0 {
            return false;
        }
        let release: Release = std::mem::transmute(com_method(shell_link, 2));
        let set_description: SetText = std::mem::transmute(com_method(shell_link, 7));
        let set_path: SetText = std::mem::transmute(com_method(shell_link, 20));
        set_path(shell_link, wide(target).as_ptr());
        set_description(shell_link, wide(description).as_ptr());
        let query: Query = std::mem::transmute(com_method(shell_link, 0));
        let mut file = null_mut();
        let mut saved = false;
        if query(shell_link, &IID_PERSIST_FILE, &mut file) == 0 {
            let save: Save = std::mem::transmute(com_method(file, 6));
            saved = save(file, wide(link).as_ptr(), 1) == 0;
            let release_file: Release = std::mem::transmute(com_method(file, 2));
            release_file(file);
        }
        release(shell_link);
        saved
    }
}

const HKCU: usize = 0x8000_0001;

/// Écrit une valeur texte dans la branche de registre de l'utilisateur.
pub fn reg_set_str(subkey: &str, name: &str, value: &str) -> bool {
    let data = wide(value);
    unsafe {
        RegSetKeyValueW(HKCU as HANDLE, wide(subkey).as_ptr(), wide(name).as_ptr(), 1, data.as_ptr() as *const c_void, (data.len() * 2) as u32) == 0
    }
}

/// Écrit une valeur numérique dans la branche de registre de l'utilisateur.
pub fn reg_set_u32(subkey: &str, name: &str, value: u32) -> bool {
    unsafe {
        RegSetKeyValueW(HKCU as HANDLE, wide(subkey).as_ptr(), wide(name).as_ptr(), 4, &value as *const u32 as *const c_void, 4) == 0
    }
}

/// Supprime une clé de la branche de registre de l'utilisateur, avec tout son contenu.
pub fn reg_delete_tree(subkey: &str) {
    unsafe { RegDeleteTreeW(HKCU as HANDLE, wide(subkey).as_ptr()) };
}

/// Question Oui / Non. Vrai si l'utilisateur répond Oui.
pub fn confirm(text: &str) -> bool {
    unsafe { MessageBoxW(null_mut(), wide(text).as_ptr(), wide("omnictl").as_ptr(), 0x24) == 6 }
}

/// Lance un programme sans fenêtre de console.
pub fn spawn_hidden(program: &str, args: &[&str]) {
    use std::os::windows::process::CommandExt;
    let _ = std::process::Command::new(program).args(args).creation_flags(0x0800_0000).spawn(); // CREATE_NO_WINDOW
}

/// Demande à l'instance déjà ouverte de se fermer complètement.
pub fn close_running(quit_msg: u32) {
    unsafe {
        let existing = FindWindowW(wide("omnictl").as_ptr(), null());
        if !existing.is_null() {
            PostMessageW(existing, quit_msg, 0, 0);
        }
    }
}
