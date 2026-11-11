//! Liaisons Windows minimales pour l'interface : fenêtre (user32), tampon hors écran (gdi32),
//! dessin lissé (GDI+), sélecteur de couleur (comdlg32), thème (dwmapi, registre).
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
/// Fenêtre de taille fixe : barre de titre, fermeture, réduction.
pub const WINDOW_STYLE: u32 = WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
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
struct CHOOSECOLORW {
    lStructSize: u32,
    hwndOwner: HWND,
    hInstance: HWND,
    rgbResult: u32,
    lpCustColors: *mut u32,
    Flags: u32,
    lCustData: isize,
    lpfnHook: *const c_void,
    lpTemplateName: *const u16,
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

#[link(name = "comdlg32")]
extern "system" {
    fn ChooseColorW(cc: *mut CHOOSECOLORW) -> i32;
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

/// Surface de dessin lissée. Coordonnées logiques, multipliées par l'échelle DPI au tracé.
pub struct Canvas {
    g: *mut c_void,
    family: *mut c_void,
    scale: f32,
}

impl Canvas {
    fn new(hdc: HDC, scale: f32) -> Canvas {
        let mut g = null_mut();
        let mut family = null_mut();
        unsafe {
            GdipCreateFromHDC(hdc, &mut g);
            GdipSetSmoothingMode(g, 4); // AntiAlias
            // Lissage sans calage sur la grille : avec GDI+, le calage dérègle l'espacement des lettres.
            GdipSetTextRenderingHint(g, 4);
            GdipCreateFontFamilyFromName(wide("Segoe UI").as_ptr(), null_mut(), &mut family);
        }
        Canvas { g, family, scale }
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
        let (x, y, w, h) = (r.x * s, r.y * s, r.w * s, r.h * s);
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
            GdipFillRectangle(self.g, brush, r.x * s, r.y * s, r.w * s, r.h * s);
        });
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
        let s = self.scale;
        let w = wide(text);
        let layout = RectF { x: r.x * s, y: r.y * s, w: r.w * s, h: r.h * s };
        let mut font = null_mut();
        let mut format = null_mut();
        unsafe {
            GdipCreateFont(self.family, size * s, bold as i32, 2, &mut font); // UnitPixel
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
            GdipDeleteGraphics(self.g);
        }
    }
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
pub fn create_window(title: &str, w: f32, h: f32, dark: bool, proc: WNDPROC) -> HWND {
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
            hIcon: null_mut(),
            hCursor: LoadCursorW(null_mut(), 32512 as *const u16), // IDC_ARROW
            hbrBackground: null_mut(),
            lpszMenuName: null(),
            lpszClassName: class.as_ptr(),
            hIconSm: null_mut(),
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
        let flag = dark as i32;
        DwmSetWindowAttribute(hwnd, 20, &flag as *const i32 as *const c_void, 4); // barre de titre sombre
        ShowWindow(hwnd, 5);
        hwnd
    }
}

fn window_size(w: f32, h: f32, dpi: u32) -> (i32, i32) {
    let s = dpi as f32 / 96.0;
    let mut r = RECT { left: 0, top: 0, right: (w * s).round() as i32, bottom: (h * s).round() as i32 };
    unsafe { AdjustWindowRectExForDpi(&mut r, WINDOW_STYLE, 0, 0, dpi) };
    (r.right - r.left, r.bottom - r.top)
}

pub fn scale(hwnd: HWND) -> f32 {
    unsafe { GetDpiForWindow(hwnd) as f32 / 96.0 }
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

/// Position du curseur d'un message souris, en pixels logiques.
pub fn mouse_pos(hwnd: HWND, lparam: isize) -> (f32, f32) {
    let s = scale(hwnd);
    let x = (lparam & 0xFFFF) as u16 as i16 as f32;
    let y = ((lparam >> 16) & 0xFFFF) as u16 as i16 as f32;
    (x / s, y / s)
}

/// Peint la fenêtre dans un tampon hors écran (pas de scintillement), puis le recopie.
pub fn paint(hwnd: HWND, draw: impl FnOnce(&Canvas)) {
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
            let canvas = Canvas::new(mem, scale(hwnd));
            draw(&canvas);
        } // le Canvas est libéré avant le contexte qu'il dessine
        BitBlt(hdc, 0, 0, w, h, mem, 0, 0, SRCCOPY);
        SelectObject(mem, previous);
        DeleteObject(bitmap);
        DeleteDC(mem);
        EndPaint(hwnd, &ps);
    }
}

/// Ouvre le sélecteur de couleur de Windows. `None` si l'utilisateur annule.
/// Boucle modale : ne pas appeler pendant qu'un emprunt de l'état de l'application est en cours.
pub fn choose_color(hwnd: HWND, initial: (u8, u8, u8), custom: &mut [u32; 16]) -> Option<(u8, u8, u8)> {
    let mut cc = CHOOSECOLORW {
        lStructSize: std::mem::size_of::<CHOOSECOLORW>() as u32,
        hwndOwner: hwnd,
        hInstance: null_mut(),
        rgbResult: initial.0 as u32 | (initial.1 as u32) << 8 | (initial.2 as u32) << 16,
        lpCustColors: custom.as_mut_ptr(),
        Flags: 0x1 | 0x2 | 0x100, // RGBINIT | FULLOPEN | ANYCOLOR
        lCustData: 0,
        lpfnHook: null(),
        lpTemplateName: null(),
    };
    if unsafe { ChooseColorW(&mut cc) } == 0 {
        return None;
    }
    let c = cc.rgbResult;
    Some((c as u8, (c >> 8) as u8, (c >> 16) as u8))
}
