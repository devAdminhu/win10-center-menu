# win10-center-menu

Centered taskbar and a light Start panel for Windows 10, in one ~300 KB Rust/Win32 exe.

- Groups the Start button and taskbar icons in the center
- Start (click or Win key) opens an acrylic panel: pinned grid, search, all apps
- ~20 MB RAM, no .NET, no browser engine
- Tray icon → **Sair** restores the original taskbar

## Build

```powershell
powershell -File preparar-rust.ps1   # local Rust toolchain in tools/, PATH untouched
powershell -File compilar.ps1 -Test
powershell -File compilar.ps1
copy target\release\menu-rust.exe MenuRust.exe
```

## App catalog

```powershell
powershell -File atualizar-catalogo.ps1   # apps.json from Get-StartApps
python catalogo.py                        # apps.tsv next to the exe; first 18 are pinned
```

Edit the `names` list in `catalogo.py` to choose the pinned apps.

## Notes

- Uses `SetWindowCompositionAttribute` (undocumented) for acrylic; falls back to opaque.
- If the process is killed, restarting Explorer restores the taskbar.
- Tested on Windows 10 22H2, primary horizontal taskbar.
