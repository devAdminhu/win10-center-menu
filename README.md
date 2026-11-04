# win10-center-menu

Centered taskbar and a light Start panel for Windows 10, in one ~300 KB Rust/Win32 exe.

- Groups the Start button and taskbar icons in the center
- Start (click or Win key) opens an acrylic panel: pinned grid, search, all apps
- ~20 MB RAM, ~370 KB exe, no .NET, no browser engine
- Tray icon → **Sair** restores the original taskbar

## Use

Download `MenuRust.exe` from [Releases](../../releases) and run it. The app list is built on first run (and refreshed on every start or via tray → **Atualizar apps**).

- Right-click an app: open, run as admin, open file location, uninstall
- Pinned apps: the `$fixados` list in `catalogo.ps1`

## Build

```powershell
powershell -File preparar-rust.ps1   # local Rust toolchain in tools/, PATH untouched
powershell -File compilar.ps1 -Test
powershell -File compilar.ps1
```

CI (`.github/workflows/build.yml`) runs clippy, tests (incl. a search speed check), enforces a 600 KB size budget and publishes `MenuRust.exe` on `v*` tags.

## Notes

- Uses `SetWindowCompositionAttribute` (undocumented) for acrylic; falls back to opaque.
- If the process is killed, restarting Explorer restores the taskbar.
- Tested on Windows 10 22H2, primary horizontal taskbar.
