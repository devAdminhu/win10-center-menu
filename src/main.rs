#![windows_subsystem = "windows"]
use std::{
    ffi::c_void,
    mem::{size_of, zeroed},
    path::PathBuf,
    ptr::{null, null_mut},
    sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, AtomicU32, Ordering::Relaxed},
};
use windows_sys::{
    core::GUID,
    Win32::{
        Foundation::*,
        Graphics::{Dwm::*, Gdi::*, GdiPlus as gp},
        System::{Com::*, LibraryLoader::*, Power::SetSuspendState, Shutdown::{ExitWindowsEx, LockWorkStation, EWX_LOGOFF}, Threading::{AttachThreadInput, GetCurrentThreadId}},
        UI::{Accessibility::AccessibleObjectFromWindow, Controls::{ImageList_GetIcon, HIMAGELIST, ILD_TRANSPARENT, MARGINS, WM_MOUSELEAVE}, Input::KeyboardAndMouse::*, Shell::*, WindowsAndMessaging::*},
    },
};

const W: i32 = 640;
const H: i32 = 472;
const CLASS: &str = "MenuRustNative";
const TOGGLE: u32 = WM_APP + 1;
const TRAY: u32 = WM_APP + 2;
const CATALOG_READY: u32 = WM_APP + 3;
const CATALOG_SCRIPT: &str = include_str!("../catalogo.ps1");
const UNINSTALL_SCRIPT: &str = include_str!("../desinstalar.ps1");
const NOT_FOUND: i32 = 2;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const POLL_START: usize = 1;
const POLL_BAR: usize = 2;
const PRELOAD: usize = 3;
const ICONS_PER_TICK: usize = 4;
const ICON: i32 = 32;
const PINNED: usize = 18;
const COLS: usize = 6;
const GRID_X: i32 = 32;
const GRID_TOP: i32 = 124;
const TILE_W: i32 = 96;
const TILE_H: i32 = 84;
const ROW_H: i32 = 36;
const VISIBLE_ROWS: usize = 7;
const FOOTER: i32 = 408;
const PROFILE_RECT: RECT = RECT { left: 32, top: FOOTER + 8, right: 300, bottom: H - 8 };
const POWER_RECT: RECT = RECT { left: W - 80, top: FOOTER + 8, right: W - 32, bottom: H - 8 };
const TOGGLE_RECT: RECT = RECT { left: W - 132, top: 84, right: W - 32, bottom: 112 };
const PARGB: i32 = 0xE200B;
const ARGB: i32 = 0x26200A;
const TEXT: u32 = 0xFFF3F4F6;
const DIM: u32 = 0xFFAEB3BE;
const HOVER: u32 = 0x24FFFFFF;
const SELECTED: u32 = 0x33FFFFFF;
const FIELD: u32 = 0x2EFFFFFF;
const BAND: u32 = 0x33000000;
const BORDER: u32 = 0x40FFFFFF;
const IID_IACCESSIBLE: GUID = GUID::from_u128(0x618736e0_3c3d_11cf_810c_00aa00389b71);

static MENU: AtomicIsize = AtomicIsize::new(0);
static HOOK: AtomicIsize = AtomicIsize::new(0);
static PRESSED: AtomicBool = AtomicBool::new(false);
static START: [AtomicI32; 4] = [const { AtomicI32::new(0) }; 4];
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn root() -> PathBuf {
    std::env::current_exe().unwrap().parent().unwrap().to_path_buf()
}

#[derive(Clone, Debug)]
struct App {
    name: String,
    id: String,
    icon: String,
}
fn parse(s: &str) -> Vec<App> {
    s.lines()
        .filter_map(|l| {
            let mut p = l.trim_start_matches('\u{feff}').splitn(3, '\t');
            let name = p.next()?.trim();
            let id = p.next()?.trim();
            let icon = p.next().unwrap_or("").trim();
            if name.is_empty() || id.is_empty() {
                return None;
            }
            Some(App { name: name.into(), id: id.into(), icon: icon.into() })
        })
        .collect()
}
fn fold(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            _ => c,
        })
        .collect()
}
fn search(apps: &[App], q: &str) -> Vec<usize> {
    let q = fold(q.trim());
    apps.iter().enumerate().filter(|(_, a)| fold(&a.name).contains(&q)).map(|(i, _)| i).collect()
}
fn alphabetical(apps: &[App]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..apps.len()).collect();
    order.sort_by_key(|&i| fold(&apps[i].name));
    order
}
fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, &b)| n | (b as u32) << (16 - 8 * i));
        for i in 0..4 {
            out.push(if i <= chunk.len() { ABC[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

fn quoted(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

// Embedded scripts run through -EncodedCommand so the exe ships alone, without .ps1 files next to it.
fn run_script(script: &str, args: &[(&str, &str)]) -> Option<i32> {
    use std::os::windows::process::CommandExt;
    let params: String = args.iter().map(|(name, value)| format!(" -{name} {}", quoted(value))).collect();
    let command = format!("& {{\n{script}\n}}{params}");
    let encoded = base64(&command.encode_utf16().flat_map(u16::to_le_bytes).collect::<Vec<u8>>());
    std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-EncodedCommand", &encoded])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .ok()?
        .code()
}

// Get-StartApps plus Appx manifests take ~15 s, so the catalog is rebuilt off the UI thread.
fn rebuild_catalog(hwnd: HWND) {
    let hwnd = hwnd as isize;
    std::thread::spawn(move || {
        let output = root().join("apps.tsv").to_string_lossy().into_owned();
        if run_script(CATALOG_SCRIPT, &[("Saida", &output)]) == Some(0) {
            unsafe { PostMessageW(hwnd as HWND, CATALOG_READY, 0, 0) };
        }
    });
}

unsafe fn uninstall(hwnd: HWND, app: &App) {
    ShowWindow(hwnd, SW_HIDE);
    let question = w(&format!("Desinstalar {}?", app.name));
    if MessageBoxW(null_mut(), question.as_ptr(), w("Desinstalar").as_ptr(), MB_YESNO | MB_ICONQUESTION | MB_TOPMOST) != IDYES {
        return;
    }
    let (name, id, icon, hwnd) = (app.name.clone(), app.id.clone(), app.icon.clone(), hwnd as isize);
    std::thread::spawn(move || match run_script(UNINSTALL_SCRIPT, &[("Nome", &name), ("AppId", &id), ("Atalho", &icon)]) {
        Some(NOT_FOUND) => unsafe {
            ShellExecuteW(null_mut(), w("open").as_ptr(), w("ms-settings:appsfeatures").as_ptr(), null(), null(), SW_SHOWNORMAL);
        },
        _ => rebuild_catalog(hwnd as HWND),
    });
}

fn load_catalog() -> Vec<App> {
    parse(&std::fs::read_to_string(root().join("apps.tsv")).unwrap_or_default())
}

fn short(s: &str) -> &str {
    match s {
        "Explorador de Arquivos" => "Arquivos",
        "Google Chrome" => "Chrome",
        "Microsoft Edge" => "Edge",
        "Microsoft Store" => "Store",
        "7-Zip File Manager" => "7-Zip",
        "Captura e Esboço" => "Captura",
        "Configurações" => "Ajustes",
        _ => s,
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Hit {
    Tile(usize),
    Row(usize),
    Toggle,
    Profile,
    Power,
}

#[derive(Default)]
struct Bar {
    tray: isize,
    list: isize,
    list_origin: (i32, i32),
    start: isize,
    start_origin: (i32, i32),
}

struct Ui {
    apps: Vec<App>,
    rows: Vec<usize>,
    query: String,
    all: bool,
    hover: Option<Hit>,
    selected: usize,
    scroll: usize,
    images: Vec<Option<*mut gp::GpImage>>,
    font: *mut gp::GpFont,
    small: *mut gp::GpFont,
    bold: *mut gp::GpFont,
    glyphs: *mut gp::GpFont,
    brush: *mut gp::GpSolidFill,
    format: *mut gp::GpStringFormat,
    bar: Bar,
}

impl Ui {
    unsafe fn new(apps: Vec<App>) -> Self {
        let font = |family: &str, size: f32, style: i32| {
            let mut f = null_mut();
            let mut font = null_mut();
            gp::GdipCreateFontFamilyFromName(w(family).as_ptr(), null_mut(), &mut f);
            gp::GdipCreateFont(f, size, style, 2, &mut font);
            font
        };
        let mut brush = null_mut();
        gp::GdipCreateSolidFill(TEXT, &mut brush);
        let mut format = null_mut();
        gp::GdipCreateStringFormat(0, 0, &mut format);
        gp::GdipSetStringFormatTrimming(format, 3);
        let images = vec![None; apps.len()];
        Self {
            apps,
            rows: vec![],
            query: String::new(),
            all: false,
            hover: None,
            selected: 0,
            scroll: 0,
            images,
            font: font("Segoe UI", 15.0, 0),
            small: font("Segoe UI", 12.5, 0),
            bold: font("Segoe UI Semibold", 14.0, 0),
            glyphs: font("Segoe MDL2 Assets", 15.0, 0),
            brush,
            format,
            bar: Bar::default(),
        }
    }
    fn listing(&self) -> bool {
        self.all || !self.query.trim().is_empty()
    }
    fn refresh(&mut self) {
        self.rows = if !self.query.trim().is_empty() {
            search(&self.apps, &self.query)
        } else if self.all {
            alphabetical(&self.apps)
        } else {
            vec![]
        };
        self.selected = 0;
        self.scroll = 0;
    }
    fn select(&mut self, index: usize) {
        self.selected = index.min(self.rows.len().saturating_sub(1));
        if self.selected < self.scroll {
            self.scroll = self.selected;
        }
        if self.selected >= self.scroll + VISIBLE_ROWS {
            self.scroll = self.selected + 1 - VISIBLE_ROWS;
        }
    }
    fn hit(&self, x: i32, y: i32) -> Option<Hit> {
        let inside = |r: RECT| x >= r.left && x < r.right && y >= r.top && y < r.bottom;
        if self.query.trim().is_empty() && inside(TOGGLE_RECT) {
            return Some(Hit::Toggle);
        }
        if inside(PROFILE_RECT) {
            return Some(Hit::Profile);
        }
        if inside(POWER_RECT) {
            return Some(Hit::Power);
        }
        if y < GRID_TOP || !(24..W - 24).contains(&x) {
            return None;
        }
        if self.listing() {
            let index = self.scroll + ((y - GRID_TOP) / ROW_H) as usize;
            let in_list = y < GRID_TOP + VISIBLE_ROWS as i32 * ROW_H && index < self.rows.len();
            return in_list.then_some(Hit::Row(index));
        }
        if x < GRID_X || x >= GRID_X + COLS as i32 * TILE_W {
            return None;
        }
        let index = ((y - GRID_TOP) / TILE_H) as usize * COLS + ((x - GRID_X) / TILE_W) as usize;
        (index < PINNED.min(self.apps.len())).then_some(Hit::Tile(index))
    }
    unsafe fn image(&mut self, index: usize) -> *mut gp::GpImage {
        if self.images[index].is_none() {
            self.images[index] = Some(prescaled(load_icon(&self.apps[index].icon)));
        }
        self.images[index].unwrap_or(null_mut())
    }
}

unsafe fn load_icon(path: &str) -> *mut gp::GpImage {
    if path.is_empty() {
        return null_mut();
    }
    if path.to_lowercase().ends_with(".png") {
        let mut image = null_mut();
        gp::GdipLoadImageFromFile(w(path).as_ptr(), &mut image);
        return image;
    }
    let mut info: SHFILEINFOW = zeroed();
    let path = w(&path.replace('/', "\\"));
    // SHGFI_ICON paints the shortcut arrow on .lnk icons; the system image list entry has no overlay.
    let list = SHGetFileInfoW(path.as_ptr(), 0, &mut info, size_of::<SHFILEINFOW>() as u32, SHGFI_SYSICONINDEX);
    if list == 0 {
        return null_mut();
    }
    let icon = ImageList_GetIcon(list as HIMAGELIST, info.iIcon, ILD_TRANSPARENT);
    if icon.is_null() {
        return null_mut();
    }
    let image = icon_bitmap(icon);
    DestroyIcon(icon);
    image
}

// Source logos go up to ~200 px; scaling once to the drawn size keeps every repaint cheap.
unsafe fn prescaled(image: *mut gp::GpImage) -> *mut gp::GpImage {
    if image.is_null() {
        return image;
    }
    let mut bitmap = null_mut();
    let mut g = null_mut();
    if gp::GdipCreateBitmapFromScan0(ICON, ICON, 0, PARGB, null(), &mut bitmap) != 0 {
        return image;
    }
    gp::GdipGetImageGraphicsContext(bitmap as *mut gp::GpImage, &mut g);
    gp::GdipSetInterpolationMode(g, 7);
    gp::GdipDrawImageRectI(g, image, 0, 0, ICON, ICON);
    gp::GdipDeleteGraphics(g);
    gp::GdipDisposeImage(image);
    bitmap as *mut gp::GpImage
}

// GdipCreateBitmapFromHICON drops the alpha channel, so the color bitmap is copied by hand.
unsafe fn icon_bitmap(icon: HICON) -> *mut gp::GpImage {
    let mut ii: ICONINFO = zeroed();
    if GetIconInfo(icon, &mut ii) == 0 {
        return null_mut();
    }
    if !ii.hbmMask.is_null() {
        DeleteObject(ii.hbmMask as HGDIOBJ);
    }
    if ii.hbmColor.is_null() {
        return null_mut();
    }
    let mut bm: BITMAP = zeroed();
    GetObjectW(ii.hbmColor as HGDIOBJ, size_of::<BITMAP>() as i32, &mut bm as *mut _ as *mut c_void);
    let (width, height) = (bm.bmWidth, bm.bmHeight);
    let mut pixels = vec![0u8; (width * height * 4).max(0) as usize];
    let mut info: BITMAPINFO = zeroed();
    info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
    info.bmiHeader.biWidth = width;
    info.bmiHeader.biHeight = -height;
    info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = 32;
    let dc = GetDC(null_mut());
    let copied = GetDIBits(dc, ii.hbmColor, 0, height as u32, pixels.as_mut_ptr() as *mut c_void, &mut info, DIB_RGB_COLORS) != 0;
    ReleaseDC(null_mut(), dc);
    DeleteObject(ii.hbmColor as HGDIOBJ);
    if !copied || pixels.is_empty() {
        return null_mut();
    }
    if pixels.chunks(4).all(|p| p[3] == 0) {
        pixels.chunks_mut(4).for_each(|p| p[3] = 255);
    }
    let mut bitmap = null_mut();
    if gp::GdipCreateBitmapFromScan0(width, height, 0, ARGB, null(), &mut bitmap) != 0 {
        return null_mut();
    }
    let rect = gp::Rect { X: 0, Y: 0, Width: width, Height: height };
    let mut data: gp::BitmapData = zeroed();
    if gp::GdipBitmapLockBits(bitmap, &rect, 2, ARGB, &mut data) == 0 {
        let row = width as usize * 4;
        for y in 0..height as usize {
            std::ptr::copy_nonoverlapping(pixels.as_ptr().add(y * row), (data.Scan0 as *mut u8).offset(y as isize * data.Stride as isize), row);
        }
        gp::GdipBitmapUnlockBits(bitmap, &mut data);
    }
    bitmap as *mut gp::GpImage
}

#[repr(C)]
struct Accent {
    state: u32,
    flags: u32,
    color: u32,
    animation: u32,
}
#[repr(C)]
struct Composition {
    attribute: u32,
    data: *mut c_void,
    size: usize,
}
unsafe fn acrylic(hwnd: HWND) {
    let Some(p) = GetProcAddress(GetModuleHandleW(w("user32.dll").as_ptr()), c"SetWindowCompositionAttribute".as_ptr().cast()) else { return };
    let set: unsafe extern "system" fn(HWND, *mut Composition) -> i32 = std::mem::transmute(p);
    for (state, flags) in [(4, 0), (3, 2)] {
        let mut a = Accent { state, flags, color: 0xbb25201b, animation: 0 };
        let mut d = Composition { attribute: 19, data: &mut a as *mut _ as _, size: size_of::<Accent>() };
        if set(hwnd, &mut d) != 0 {
            return;
        }
    }
}

unsafe fn fill(g: *mut gp::GpGraphics, ui: &Ui, x: i32, y: i32, width: i32, height: i32, color: u32) {
    gp::GdipSetSolidFillColor(ui.brush, color);
    gp::GdipFillRectangleI(g, ui.brush as *mut gp::GpBrush, x, y, width, height);
}

fn layout(x: i32, y: i32, width: i32, height: i32) -> gp::RectF {
    gp::RectF { X: x as f32, Y: y as f32, Width: width as f32, Height: height as f32 }
}

#[derive(Clone, Copy, PartialEq)]
enum Flow {
    Left,
    Center,
    Wrap,
}

unsafe fn text(g: *mut gp::GpGraphics, ui: &Ui, s: &str, font: *mut gp::GpFont, r: gp::RectF, color: u32, flow: Flow) {
    let center = flow != Flow::Left;
    let wrap = flow == Flow::Wrap;
    gp::GdipSetSolidFillColor(ui.brush, color);
    gp::GdipSetStringFormatAlign(ui.format, if center { 1 } else { 0 });
    gp::GdipSetStringFormatLineAlign(ui.format, if wrap { 0 } else { 1 });
    gp::GdipSetStringFormatFlags(ui.format, if wrap { 0x2000 } else { 0x1000 });
    let t: Vec<u16> = s.encode_utf16().collect();
    gp::GdipDrawString(g, t.as_ptr(), t.len() as i32, font, &r, ui.format, ui.brush as *const gp::GpBrush);
}

unsafe fn text_width(g: *mut gp::GpGraphics, ui: &Ui, s: &str, font: *mut gp::GpFont) -> i32 {
    if s.is_empty() {
        return 0;
    }
    gp::GdipSetStringFormatFlags(ui.format, 0x1000 | 0x800);
    let t: Vec<u16> = s.encode_utf16().collect();
    let mut bounds: gp::RectF = zeroed();
    gp::GdipMeasureString(g, t.as_ptr(), t.len() as i32, font, &layout(0, 0, 4000, 40), ui.format, &mut bounds, null_mut(), null_mut());
    bounds.Width as i32
}

unsafe fn draw(g: *mut gp::GpGraphics, ui: &mut Ui) {
    let mut pen = null_mut();
    gp::GdipCreatePen1(BORDER, 1.0, 2, &mut pen);
    gp::GdipDrawRectangleI(g, pen, 0, 0, W - 1, H - 1);
    gp::GdipDeletePen(pen);

    fill(g, ui, 32, 26, W - 64, 40, FIELD);
    text(g, ui, "\u{E721}", ui.glyphs, layout(48, 26, 24, 40), DIM, Flow::Left);
    if ui.query.is_empty() {
        text(g, ui, "Pesquisar apps", ui.font, layout(76, 26, W - 140, 40), DIM, Flow::Left);
    } else {
        text(g, ui, &ui.query, ui.font, layout(76, 26, W - 140, 40), TEXT, Flow::Left);
    }
    let caret = 74 + text_width(g, ui, &ui.query, ui.font).min(W - 150);
    gp::GdipSetSolidFillColor(ui.brush, TEXT);
    gp::GdipFillRectangleI(g, ui.brush as *mut gp::GpBrush, caret, 36, 1, 20);

    let title = if !ui.query.trim().is_empty() {
        "Resultados"
    } else if ui.all {
        "Todos os apps"
    } else {
        "Fixados"
    };
    text(g, ui, title, ui.bold, layout(40, 84, 300, 28), TEXT, Flow::Left);
    if ui.query.trim().is_empty() {
        let r = TOGGLE_RECT;
        let color = if ui.hover == Some(Hit::Toggle) { SELECTED } else { HOVER };
        fill(g, ui, r.left, r.top, r.right - r.left, r.bottom - r.top, color);
        let label = if ui.all { "‹  Voltar" } else { "Todos  ›" };
        text(g, ui, label, ui.small, layout(r.left, r.top, r.right - r.left, r.bottom - r.top), TEXT, Flow::Center);
    }

    if ui.listing() {
        draw_list(g, ui);
    } else {
        draw_grid(g, ui);
    }

    gp::GdipSetSolidFillColor(ui.brush, BAND);
    gp::GdipFillRectangleI(g, ui.brush as *mut gp::GpBrush, 1, FOOTER, W - 2, H - FOOTER - 1);
    for (hit, r) in [(Hit::Profile, PROFILE_RECT), (Hit::Power, POWER_RECT)] {
        if ui.hover == Some(hit) {
            fill(g, ui, r.left, r.top, r.right - r.left, r.bottom - r.top, HOVER);
        }
    }
    let p = POWER_RECT;
    text(g, ui, "\u{E7E8}", ui.glyphs, layout(p.left, p.top, p.right - p.left, p.bottom - p.top), TEXT, Flow::Center);
    gp::GdipSetSolidFillColor(ui.brush, FIELD);
    gp::GdipFillEllipseI(g, ui.brush as *mut gp::GpBrush, 40, FOOTER + 16, 32, 32);
    text(g, ui, "\u{E77B}", ui.glyphs, layout(40, FOOTER + 16, 32, 32), TEXT, Flow::Center);
    let user = std::env::var("USERNAME").unwrap_or_default();
    text(g, ui, &user, ui.font, layout(84, FOOTER, 300, H - FOOTER), TEXT, Flow::Left);
}

unsafe fn draw_grid(g: *mut gp::GpGraphics, ui: &mut Ui) {
    if ui.apps.is_empty() {
        text(g, ui, "Carregando apps…", ui.font, layout(40, GRID_TOP, 400, ROW_H), DIM, Flow::Left);
        return;
    }
    for i in 0..PINNED.min(ui.apps.len()) {
        let x = GRID_X + (i % COLS) as i32 * TILE_W;
        let y = GRID_TOP + (i / COLS) as i32 * TILE_H;
        if ui.hover == Some(Hit::Tile(i)) {
            fill(g, ui, x + 4, y + 2, TILE_W - 8, TILE_H - 4, HOVER);
        }
        let image = ui.image(i);
        if image.is_null() {
            let initial = ui.apps[i].name.chars().next().unwrap_or('?').to_string();
            fill(g, ui, x + 32, y + 12, 32, 32, FIELD);
            text(g, ui, &initial, ui.font, layout(x + 32, y + 12, 32, 32), TEXT, Flow::Center);
        } else {
            gp::GdipDrawImageRectI(g, image, x + 32, y + 12, 32, 32);
        }
        let name = short(&ui.apps[i].name).to_string();
        text(g, ui, &name, ui.small, layout(x + 4, y + 50, TILE_W - 8, 32), TEXT, Flow::Wrap);
    }
}

unsafe fn draw_list(g: *mut gp::GpGraphics, ui: &mut Ui) {
    if ui.rows.is_empty() {
        text(g, ui, "Nada encontrado", ui.font, layout(40, GRID_TOP, 400, ROW_H), DIM, Flow::Left);
        return;
    }
    for k in 0..VISIBLE_ROWS {
        let index = ui.scroll + k;
        let Some(&app) = ui.rows.get(index) else { break };
        let y = GRID_TOP + k as i32 * ROW_H;
        if index == ui.selected {
            fill(g, ui, 24, y + 2, W - 56, ROW_H - 4, SELECTED);
        } else if ui.hover == Some(Hit::Row(index)) {
            fill(g, ui, 24, y + 2, W - 56, ROW_H - 4, HOVER);
        }
        let image = ui.image(app);
        if !image.is_null() {
            gp::GdipDrawImageRectI(g, image, 36, y + 8, 20, 20);
        }
        let name = ui.apps[app].name.clone();
        text(g, ui, &name, ui.font, layout(68, y, W - 120, ROW_H), TEXT, Flow::Left);
    }
    if ui.rows.len() > VISIBLE_ROWS {
        let track = VISIBLE_ROWS as i32 * ROW_H;
        let thumb = (track * VISIBLE_ROWS as i32 / ui.rows.len() as i32).max(24);
        let max_scroll = (ui.rows.len() - VISIBLE_ROWS) as i32;
        let top = GRID_TOP + (track - thumb) * ui.scroll as i32 / max_scroll;
        fill(g, ui, W - 26, top, 3, thumb, 0x60FFFFFF);
    }
}

// GDI writes zero alpha, so the frame is composed in a PARGB buffer to keep the acrylic visible behind it.
unsafe fn paint(hdc: HDC, ui: &mut Ui) {
    let mut bitmap = null_mut();
    if gp::GdipCreateBitmapFromScan0(W, H, 0, PARGB, null(), &mut bitmap) != 0 {
        return;
    }
    let mut g = null_mut();
    gp::GdipGetImageGraphicsContext(bitmap as *mut gp::GpImage, &mut g);
    gp::GdipSetSmoothingMode(g, 4);
    gp::GdipSetTextRenderingHint(g, 4);
    gp::GdipGraphicsClear(g, 0);
    draw(g, ui);
    gp::GdipDeleteGraphics(g);
    let rect = gp::Rect { X: 0, Y: 0, Width: W, Height: H };
    let mut data: gp::BitmapData = zeroed();
    if gp::GdipBitmapLockBits(bitmap, &rect, 1, PARGB, &mut data) == 0 {
        let mut info: BITMAPINFO = zeroed();
        info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = W;
        info.bmiHeader.biHeight = -H;
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        SetDIBitsToDevice(hdc, 0, 0, W as u32, H as u32, 0, 0, 0, H as u32, data.Scan0, &info, DIB_RGB_COLORS);
        gp::GdipBitmapUnlockBits(bitmap, &mut data);
    }
    gp::GdipDisposeImage(bitmap as *mut gp::GpImage);
}

unsafe fn rect(h: HWND) -> Option<RECT> {
    let mut r: RECT = zeroed();
    (!h.is_null() && GetWindowRect(h, &mut r) != 0).then_some(r)
}
unsafe fn class_name(h: HWND) -> String {
    let mut c = [0u16; 128];
    let n = GetClassNameW(h, c.as_mut_ptr(), 128);
    String::from_utf16_lossy(&c[..n.max(0) as usize])
}
unsafe extern "system" fn find_list(h: HWND, l: LPARAM) -> BOOL {
    if class_name(h) == "MSTaskListWClass" {
        *(l as *mut HWND) = h;
        return 0;
    }
    1
}

#[repr(C)]
struct Variant {
    vt: u16,
    reserved: [u16; 3],
    value: i64,
    extra: i64,
}

// Taskbar buttons are not windows on Windows 10; their bounds only come from MSAA (IAccessible vtable).
unsafe fn button_span(list: HWND) -> Option<(i32, i32)> {
    let mut acc: *mut c_void = null_mut();
    if AccessibleObjectFromWindow(list, OBJID_CLIENT as u32, &IID_IACCESSIBLE, &mut acc) < 0 || acc.is_null() {
        return None;
    }
    let vtable = *(acc as *const *const usize);
    let release: unsafe extern "system" fn(*mut c_void) -> u32 = std::mem::transmute(*vtable.add(2));
    let count: unsafe extern "system" fn(*mut c_void, *mut i32) -> i32 = std::mem::transmute(*vtable.add(8));
    let location: unsafe extern "system" fn(*mut c_void, *mut i32, *mut i32, *mut i32, *mut i32, Variant) -> i32 = std::mem::transmute(*vtable.add(22));
    let mut n = 0;
    count(acc, &mut n);
    let (mut left, mut right) = (i32::MAX, i32::MIN);
    for i in 1..=n.min(200) {
        let (mut x, mut y, mut cx, mut cy) = (0, 0, 0, 0);
        let child = Variant { vt: 3, reserved: [0; 3], value: i as i64, extra: 0 };
        if location(acc, &mut x, &mut y, &mut cx, &mut cy, child) >= 0 && cx > 0 && cy > 0 {
            left = left.min(x);
            right = right.max(x + cx);
        }
    }
    release(acc);
    (left < right).then_some((left, right))
}

unsafe fn center_taskbar(bar: &mut Bar) {
    let tray = FindWindowW(w("Shell_TrayWnd").as_ptr(), null());
    let mut list: HWND = null_mut();
    if tray.is_null() {
        return;
    }
    EnumChildWindows(tray, Some(find_list), &mut list as *mut HWND as LPARAM);
    let (Some(b), Some(r), Some(p)) = (rect(tray), rect(list), rect(GetParent(list))) else { return };
    if b.right - b.left < b.bottom - b.top {
        return;
    }
    if tray as isize != bar.tray {
        *bar = Bar { tray: tray as isize, ..Bar::default() };
        let start = FindWindowExW(tray, null_mut(), w("Start").as_ptr(), null());
        if let Some(s) = rect(start) {
            bar.start = start as isize;
            bar.start_origin = (s.left - b.left, s.top - b.top);
        }
    }
    if list as isize != bar.list {
        bar.list = list as isize;
        bar.list_origin = (r.left - p.left, r.top - p.top);
    }
    let Some((left, right)) = button_span(list) else { return };
    let start = rect(bar.start as HWND);
    let start_width = start.map_or(0, |s| s.right - s.left);
    let span = right - left;
    let offset = left - r.left;
    let group = b.left + (b.right - b.left - span - start_width) / 2;
    let desired = (group + start_width - offset).min(p.right - span - offset - 8).max(p.left);
    if (r.left - desired).abs() > 1 {
        SetWindowPos(list, null_mut(), desired - p.left, r.top - p.top, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS);
    }
    if let Some(s) = start {
        if s.left != group {
            SetWindowPos(bar.start as HWND, null_mut(), group - b.left, bar.start_origin.1, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS);
        }
        for (slot, value) in START.iter().zip([group, s.top, group + start_width, s.bottom]) {
            slot.store(value, Relaxed);
        }
    }
}

unsafe fn restore_taskbar(bar: &Bar) {
    if bar.list != 0 {
        SetWindowPos(bar.list as HWND, null_mut(), bar.list_origin.0, bar.list_origin.1, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
    }
    if bar.start != 0 {
        SetWindowPos(bar.start as HWND, null_mut(), bar.start_origin.0, bar.start_origin.1, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
    }
}

unsafe extern "system" fn mouse(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let message = wp as u32;
    if code >= 0 && (message == WM_LBUTTONDOWN || message == WM_LBUTTONUP) {
        let pt = (*(lp as *const MSLLHOOKSTRUCT)).pt;
        let [l, t, r, b] = [0, 1, 2, 3].map(|i| START[i].load(Relaxed));
        let on_start = r > l && pt.x >= l && pt.x < r && pt.y >= t && pt.y < b;
        if message == WM_LBUTTONDOWN && on_start {
            PRESSED.store(true, Relaxed);
            return 1;
        }
        if message == WM_LBUTTONUP && PRESSED.swap(false, Relaxed) {
            PostMessageW(MENU.load(Relaxed) as HWND, TOGGLE, 0, 0);
            return 1;
        }
    }
    CallNextHookEx(null_mut(), code, wp, lp)
}

unsafe fn tray_icon(hwnd: HWND, action: u32) {
    let mut nid: NOTIFYICONDATAW = zeroed();
    nid.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    nid.uCallbackMessage = TRAY;
    nid.hIcon = LoadIconW(null_mut(), IDI_APPLICATION);
    for (slot, c) in nid.szTip.iter_mut().zip("MenuRust".encode_utf16()) {
        *slot = c;
    }
    Shell_NotifyIconW(action, &nid);
}

unsafe fn ui(hwnd: HWND) -> Option<&'static mut Ui> {
    (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Ui).as_mut()
}

// A background process may not take focus from the Start menu unless it shares that thread's input queue.
unsafe fn focus(hwnd: HWND) {
    let foreground = GetForegroundWindow();
    let other = GetWindowThreadProcessId(foreground, null_mut());
    let current = GetCurrentThreadId();
    let attached = other != 0 && other != current && AttachThreadInput(current, other, 1) != 0;
    SetForegroundWindow(hwnd);
    SetFocus(hwnd);
    if attached {
        AttachThreadInput(current, other, 0);
    }
}

unsafe fn show(hwnd: HWND, ui: &mut Ui) {
    ui.query.clear();
    ui.all = false;
    ui.hover = None;
    ui.refresh();
    let mut area: RECT = zeroed();
    SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut area as *mut _ as *mut c_void, 0);
    let x = area.left + (area.right - area.left - W) / 2;
    let y = (area.bottom - H - 12).max(area.top);
    SetWindowPos(hwnd, HWND_TOPMOST, x, y, W, H, SWP_SHOWWINDOW);
    InvalidateRect(hwnd, null(), 0);
    focus(hwnd);
}

unsafe fn launch(hwnd: HWND, ui: &Ui, index: usize) {
    let Some(app) = ui.apps.get(index) else { return };
    ShowWindow(hwnd, SW_HIDE);
    let target = w(&format!("shell:AppsFolder\\{}", app.id));
    ShellExecuteW(hwnd, w("open").as_ptr(), w("explorer.exe").as_ptr(), target.as_ptr(), null(), SW_SHOWNORMAL);
}

fn uninstallable(app: &App, windows_dir: &str) -> bool {
    let path = app.icon.replace('/', "\\").to_lowercase();
    let system = !windows_dir.is_empty() && path.starts_with(&windows_dir.to_lowercase());
    app.id.contains('!') || (!path.is_empty() && !system)
}

fn file_target(app: &App) -> Option<String> {
    let path = app.icon.replace('/', "\\");
    (!path.to_lowercase().ends_with(".png") && std::path::Path::new(&path).is_file()).then_some(path)
}

unsafe fn app_menu(hwnd: HWND, ui: &Ui, index: usize) {
    let Some(app) = ui.apps.get(index) else { return };
    let target = file_target(app);
    let menu = CreatePopupMenu();
    AppendMenuW(menu, MF_STRING, 1, w("Abrir").as_ptr());
    if target.is_some() {
        AppendMenuW(menu, MF_STRING, 2, w("Executar como administrador").as_ptr());
        AppendMenuW(menu, MF_STRING, 3, w("Abrir local do arquivo").as_ptr());
    }
    if uninstallable(app, &std::env::var("WINDIR").unwrap_or_default()) {
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        AppendMenuW(menu, MF_STRING, 4, w("Desinstalar").as_ptr());
    }
    let mut pt: POINT = zeroed();
    GetCursorPos(&mut pt);
    let choice = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, pt.x, pt.y, 0, hwnd, null());
    DestroyMenu(menu);
    let path = target.unwrap_or_default();
    match choice {
        1 => launch(hwnd, ui, index),
        2 => {
            ShowWindow(hwnd, SW_HIDE);
            ShellExecuteW(hwnd, w("runas").as_ptr(), w(&path).as_ptr(), null(), null(), SW_SHOWNORMAL);
        }
        3 => {
            ShowWindow(hwnd, SW_HIDE);
            let select = w(&format!("/select,\"{path}\""));
            ShellExecuteW(hwnd, w("open").as_ptr(), w("explorer.exe").as_ptr(), select.as_ptr(), null(), SW_SHOWNORMAL);
        }
        4 => uninstall(hwnd, app),
        _ => {}
    }
}

unsafe fn popup(hwnd: HWND, items: &[&str]) -> usize {
    let menu = CreatePopupMenu();
    for (i, item) in items.iter().enumerate() {
        AppendMenuW(menu, MF_STRING, i + 1, w(item).as_ptr());
    }
    let mut pt: POINT = zeroed();
    GetCursorPos(&mut pt);
    let choice = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN, pt.x, pt.y, 0, hwnd, null());
    DestroyMenu(menu);
    choice as usize
}

unsafe fn open(target: &str, args: &str, show: SHOW_WINDOW_CMD) {
    let args = w(args);
    let args = if args.len() > 1 { args.as_ptr() } else { null() };
    ShellExecuteW(null_mut(), w("open").as_ptr(), w(target).as_ptr(), args, null(), show);
}

unsafe fn profile_menu(hwnd: HWND) {
    let choice = popup(hwnd, &["Alterar configurações da conta", "Bloquear", "Sair"]);
    if choice != 0 {
        ShowWindow(hwnd, SW_HIDE);
    }
    match choice {
        1 => open("ms-settings:yourinfo", "", SW_SHOWNORMAL),
        2 => {
            LockWorkStation();
        }
        3 => {
            ExitWindowsEx(EWX_LOGOFF, 0);
        }
        _ => {}
    }
}

unsafe fn power_menu(hwnd: HWND) {
    let choice = popup(hwnd, &["Suspender", "Desligar", "Reiniciar"]);
    if choice != 0 {
        ShowWindow(hwnd, SW_HIDE);
    }
    match choice {
        1 => {
            SetSuspendState(0, 0, 0);
        }
        2 => open("shutdown.exe", "/s /t 0", SW_HIDE),
        3 => open("shutdown.exe", "/r /t 0", SW_HIDE),
        _ => {}
    }
}

unsafe fn activate(hwnd: HWND, ui: &mut Ui, hit: Hit) {
    match hit {
        Hit::Toggle => {
            ui.all = !ui.all;
            ui.refresh();
        }
        Hit::Tile(i) => launch(hwnd, ui, i),
        Hit::Profile => profile_menu(hwnd),
        Hit::Power => power_menu(hwnd),
        Hit::Row(i) => {
            if let Some(&app) = ui.rows.get(i) {
                launch(hwnd, ui, app);
            }
        }
    }
}

unsafe fn key(hwnd: HWND, ui: &mut Ui, vk: u16) {
    match vk {
        VK_ESCAPE => {
            ShowWindow(hwnd, SW_HIDE);
        }
        VK_DOWN if ui.listing() => ui.select(ui.selected + 1),
        VK_UP if ui.listing() => ui.select(ui.selected.saturating_sub(1)),
        VK_RETURN if ui.listing() => {
            if let Some(&app) = ui.rows.get(ui.selected) {
                launch(hwnd, ui, app);
            }
        }
        _ => {}
    }
}

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == TASKBAR_CREATED.load(Relaxed) && msg != 0 {
        tray_icon(hwnd, NIM_ADD);
        return 0;
    }
    let Some(ui) = ui(hwnd) else { return DefWindowProcW(hwnd, msg, wp, lp) };
    match msg {
        WM_ERASEBKGND => return 1,
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            paint(hdc, ui);
            EndPaint(hwnd, &ps);
            return 0;
        }
        WM_TIMER if wp == POLL_START => {
            let foreground = GetForegroundWindow();
            let mut title = [0u16; 32];
            let n = GetWindowTextW(foreground, title.as_mut_ptr(), 32);
            let title = String::from_utf16_lossy(&title[..n.max(0) as usize]);
            let native_start = class_name(foreground) == "Windows.UI.Core.CoreWindow" && (title == "Iniciar" || title == "Start");
            if native_start && IsWindowVisible(hwnd) == 0 {
                show(hwnd, ui);
            }
            return 0;
        }
        CATALOG_READY => {
            let apps = load_catalog();
            if !apps.is_empty() {
                ui.images.iter().flatten().filter(|i| !i.is_null()).for_each(|&i| {
                    gp::GdipDisposeImage(i);
                });
                ui.images = vec![None; apps.len()];
                ui.apps = apps;
                ui.refresh();
                SetTimer(hwnd, PRELOAD, 15, None);
                InvalidateRect(hwnd, null(), 0);
            }
            return 0;
        }
        WM_TIMER if wp == PRELOAD => {
            match ui.images.iter().position(Option::is_none) {
                Some(first) => (first..ui.apps.len().min(first + ICONS_PER_TICK)).for_each(|i| {
                    ui.image(i);
                }),
                None => {
                    KillTimer(hwnd, PRELOAD);
                }
            }
            return 0;
        }
        WM_TIMER if wp == POLL_BAR => {
            center_taskbar(&mut ui.bar);
            return 0;
        }
        TOGGLE => {
            if IsWindowVisible(hwnd) != 0 {
                ShowWindow(hwnd, SW_HIDE);
            } else {
                show(hwnd, ui);
            }
            return 0;
        }
        TRAY if (lp & 0xffff) as u32 == WM_RBUTTONUP => {
            let menu = CreatePopupMenu();
            AppendMenuW(menu, MF_STRING, 2, w("Atualizar apps").as_ptr());
            AppendMenuW(menu, MF_STRING, 1, w("Sair").as_ptr());
            let mut pt: POINT = zeroed();
            GetCursorPos(&mut pt);
            SetForegroundWindow(hwnd);
            match TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, pt.x, pt.y, 0, hwnd, null()) {
                1 => {
                    DestroyWindow(hwnd);
                }
                2 => rebuild_catalog(hwnd),
                _ => {}
            }
            DestroyMenu(menu);
            return 0;
        }
        TRAY if (lp & 0xffff) as u32 == WM_LBUTTONUP => {
            show(hwnd, ui);
            return 0;
        }
        WM_ACTIVATE if wp & 0xffff == 0 => {
            ShowWindow(hwnd, SW_HIDE);
            return 0;
        }
        WM_CHAR => {
            match wp as u32 {
                0x08 => {
                    ui.query.pop();
                }
                0x7f => ui.query.clear(),
                c if c >= 0x20 => ui.query.extend(char::from_u32(c)),
                _ => return 0,
            }
            ui.refresh();
            InvalidateRect(hwnd, null(), 0);
            return 0;
        }
        WM_KEYDOWN => {
            key(hwnd, ui, wp as u16);
            InvalidateRect(hwnd, null(), 0);
            return 0;
        }
        WM_MOUSEMOVE => {
            let hit = ui.hit((lp & 0xffff) as i16 as i32, ((lp >> 16) & 0xffff) as i16 as i32);
            if hit != ui.hover {
                ui.hover = hit;
                InvalidateRect(hwnd, null(), 0);
            }
            let mut track = TRACKMOUSEEVENT { cbSize: size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
            TrackMouseEvent(&mut track);
            return 0;
        }
        WM_MOUSELEAVE => {
            ui.hover = None;
            InvalidateRect(hwnd, null(), 0);
            return 0;
        }
        WM_LBUTTONUP => {
            if let Some(hit) = ui.hit((lp & 0xffff) as i16 as i32, ((lp >> 16) & 0xffff) as i16 as i32) {
                activate(hwnd, ui, hit);
                InvalidateRect(hwnd, null(), 0);
            }
            return 0;
        }
        WM_RBUTTONUP => {
            match ui.hit((lp & 0xffff) as i16 as i32, ((lp >> 16) & 0xffff) as i16 as i32) {
                Some(Hit::Tile(i)) => app_menu(hwnd, ui, i),
                Some(Hit::Row(i)) => {
                    if let Some(&app) = ui.rows.get(i) {
                        app_menu(hwnd, ui, app);
                    }
                }
                _ => {}
            }
            return 0;
        }
        WM_MOUSEWHEEL if ui.listing() => {
            let delta = ((wp >> 16) & 0xffff) as i16;
            let max = ui.rows.len().saturating_sub(VISIBLE_ROWS);
            ui.scroll = if delta > 0 { ui.scroll.saturating_sub(3) } else { (ui.scroll + 3).min(max) };
            InvalidateRect(hwnd, null(), 0);
            return 0;
        }
        WM_DESTROY => {
            KillTimer(hwnd, POLL_START);
            KillTimer(hwnd, POLL_BAR);
            UnhookWindowsHookEx(HOOK.load(Relaxed) as HHOOK);
            restore_taskbar(&ui.bar);
            tray_icon(hwnd, NIM_DELETE);
            PostQuitMessage(0);
            return 0;
        }
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

fn main() {
    unsafe {
        let existing = FindWindowW(w(CLASS).as_ptr(), null());
        if !existing.is_null() {
            PostMessageW(existing, TOGGLE, 0, 0);
            return;
        }
        let apps = load_catalog();
        let _ = CoInitializeEx(null(), COINIT_APARTMENTTHREADED as u32);
        let mut token = 0;
        let input = gp::GdiplusStartupInput { GdiplusVersion: 1, DebugEventCallback: 0, SuppressBackgroundThread: 0, SuppressExternalCodecs: 0 };
        gp::GdiplusStartup(&mut token, &input, null_mut());
        TASKBAR_CREATED.store(RegisterWindowMessageW(w("TaskbarCreated").as_ptr()), Relaxed);

        let instance = GetModuleHandleW(null());
        let class = w(CLASS);
        let wc = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            lpszClassName: class.as_ptr(),
            ..zeroed()
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(WS_EX_TOOLWINDOW | WS_EX_TOPMOST, class.as_ptr(), w("MenuRust").as_ptr(), WS_POPUP, 0, 0, W, H, null_mut(), null_mut(), instance, null());
        if hwnd.is_null() {
            return;
        }
        let state = Box::into_raw(Box::new(Ui::new(apps)));
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize);
        for i in 0..PINNED.min((*state).apps.len()) {
            (*state).image(i);
        }
        let margins = MARGINS { cxLeftWidth: -1, cxRightWidth: -1, cyTopHeight: -1, cyBottomHeight: -1 };
        DwmExtendFrameIntoClientArea(hwnd, &margins);
        acrylic(hwnd);

        MENU.store(hwnd as isize, Relaxed);
        HOOK.store(SetWindowsHookExW(WH_MOUSE_LL, Some(mouse), instance, 0) as isize, Relaxed);
        tray_icon(hwnd, NIM_ADD);
        center_taskbar(&mut (*state).bar);
        SetTimer(hwnd, POLL_START, 150, None);
        SetTimer(hwnd, POLL_BAR, 750, None);
        SetTimer(hwnd, PRELOAD, 15, None);
        rebuild_catalog(hwnd);
        show(hwnd, &mut *state);

        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        CoUninitialize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_bad_rows() {
        assert_eq!(parse("bad\n\tmissing\nGood\tapp!id\timage.png").len(), 1);
    }
    #[test]
    fn search_ignores_case_and_accents() {
        let a = parse("Configurações\tsettings!id\nCalculadora\tcalc!id\nJoão\tjoao!id");
        assert_eq!(search(&a, "CONFIGURACOES"), vec![0]);
        assert_eq!(search(&a, "calc"), vec![1]);
        assert_eq!(search(&a, "joao"), vec![2]);
        assert!(search(&a, "nada").is_empty());
    }
    #[test]
    fn search_stays_fast_on_large_catalogs() {
        let catalog: String = (0..20_000).map(|i| format!("Aplicação {i}\tapp{i}!id\n")).collect();
        let apps = parse(&catalog);
        let start = std::time::Instant::now();
        for q in ["aplicacao 19999", "APLICAÇÃO 5", "nada"] {
            search(&apps, q);
        }
        assert!(start.elapsed().as_millis() < 300, "busca lenta: {:?}", start.elapsed());
    }
    #[test]
    fn file_target_skips_store_logos() {
        let a = parse("Loja\tstore!id\tC:/x/logo.png\nNotas\tnotes\tC:/Windows/System32/notepad.exe");
        assert_eq!(file_target(&a[0]), None);
        assert_eq!(file_target(&a[1]).as_deref(), Some(r"C:\Windows\System32\notepad.exe"));
    }
    #[test]
    fn windows_tools_are_not_uninstallable() {
        let a = parse("Paint\tpaint\tC:/Windows/System32/mspaint.exe\nLoja\tstore!id\nChrome\tChrome\tC:/ProgramData/Chrome.lnk\nPC\tpc");
        let found: Vec<bool> = a.iter().map(|app| uninstallable(app, r"C:\Windows")).collect();
        assert_eq!(found, vec![false, true, true, false]);
    }
    #[test]
    fn quoted_escapes_single_quotes() {
        assert_eq!(quoted("D'Água"), "'D''Água'");
    }
    #[test]
    fn base64_matches_rfc4648() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
    #[test]
    fn alphabetical_uses_folded_names() {
        let a = parse("Zeta\tz\nÁgua\ta\nbeta\tb");
        assert_eq!(alphabetical(&a), vec![1, 2, 0]);
    }
}
