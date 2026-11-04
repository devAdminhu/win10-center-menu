param([Parameter(Mandatory)][string]$Saida)
$ErrorActionPreference = 'Stop'

$fixados = @('Google Chrome', 'Microsoft Edge', 'Explorador de Arquivos', 'File Explorer', 'Terminal', 'Windows Terminal',
  'WhatsApp', 'Telegram', 'Claude', 'Fotos', 'Photos', 'Calculadora', 'Calculator', 'Paint', 'Configurações', 'Settings',
  'Bloco de Notas', 'Notepad', 'Captura e Esboço', 'Snip & Sketch', 'Microsoft Store', '7-Zip File Manager')
$alternativos = @{
  'Explorador de Arquivos' = "$env:WINDIR\explorer.exe"; 'File Explorer' = "$env:WINDIR\explorer.exe"
  'Paint' = "$env:WINDIR\System32\mspaint.exe"; 'Bloco de Notas' = "$env:WINDIR\System32\notepad.exe"; 'Notepad' = "$env:WINDIR\System32\notepad.exe"
}

$atalhos = @{}
foreach ($base in @($env:APPDATA, $env:ProgramData)) {
  $pasta = Join-Path $base 'Microsoft\Windows\Start Menu\Programs'
  if (Test-Path $pasta) {
    Get-ChildItem $pasta -Recurse -Filter *.lnk | ForEach-Object { if (-not $atalhos.ContainsKey($_.BaseName)) { $atalhos[$_.BaseName] = $_.FullName } }
  }
}

$pacotes = @{}
Get-AppxPackage | ForEach-Object { $pacotes[$_.PackageFamilyName] = $_ }

# Store apps only expose a logo through the package manifest; scale-200/targetsize variants are resolved by name.
function Logo($appId) {
  $familia, $id = $appId.Split('!')
  $pacote = $pacotes[$familia]
  if (-not $id -or -not $pacote -or -not $pacote.InstallLocation) { return $null }
  try {
    [xml]$manifesto = Get-Content -LiteralPath (Join-Path $pacote.InstallLocation 'AppxManifest.xml')
    $app = $manifesto.SelectNodes("//*[local-name()='Application']") | Where-Object Id -EQ $id | Select-Object -First 1
    $logo = $app.SelectSingleNode("./*[local-name()='VisualElements']").GetAttribute('Square44x44Logo')
    $caminho = Join-Path $pacote.InstallLocation $logo
    if (Test-Path -LiteralPath $caminho) { return $caminho }
    $base = [IO.Path]::GetFileNameWithoutExtension($caminho)
    Get-ChildItem -LiteralPath (Split-Path $caminho) -Filter "$base*.png" -ErrorAction SilentlyContinue |
      Where-Object Name -notmatch 'contrast' |
      Sort-Object { if ($_.Name -match 'targetsize-48.*unplated') { 0 } elseif ($_.Name -match 'scale-200') { 1 } else { 2 } } |
      Select-Object -First 1 -ExpandProperty FullName
  } catch { $null }
}

$apps = @(Get-StartApps)
$ordem = @($fixados | ForEach-Object { $nome = $_; $apps | Where-Object Name -EQ $nome | Select-Object -First 1 }) +
  @($apps | Where-Object { $fixados -notcontains $_.Name })

$linhas = foreach ($a in $ordem) {
  $icone = Logo $a.AppID
  if (-not $icone) { $icone = $atalhos[$a.Name] }
  if (-not $icone) { $icone = $alternativos[$a.Name] }
  (@($a.Name, $a.AppID, $icone) | ForEach-Object { "$_" -replace "[`t`r`n]", ' ' }) -join "`t"
}
[IO.File]::WriteAllLines($Saida, [string[]]$linhas, (New-Object Text.UTF8Encoding $false))
