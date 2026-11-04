param([Parameter(Mandatory)][string]$Nome, [Parameter(Mandatory)][string]$AppId, [string]$Atalho = '', [switch]$Simular)
$ErrorActionPreference = 'Stop'

if ($AppId.Contains('!')) {
  $pacote = Get-AppxPackage | Where-Object PackageFamilyName -EQ $AppId.Split('!')[0] | Select-Object -First 1
  if (-not $pacote) { exit 2 }
  if ($Simular) { $pacote.PackageFullName; exit 0 }
  $pacote | Remove-AppxPackage
  exit 0
}

$chaves = @(
  'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*',
  'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*',
  'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*'
)
$programas = @(Get-ItemProperty $chaves -ErrorAction SilentlyContinue | Where-Object { $_.DisplayName -and $_.UninstallString })

# Shortcut names rarely match DisplayName ("7-Zip File Manager" vs "7-Zip 24.08"), so the install folder decides first.
$pasta = ''
if ($Atalho -like '*.lnk' -and (Test-Path -LiteralPath $Atalho)) { $Atalho = (New-Object -ComObject WScript.Shell).CreateShortcut($Atalho).TargetPath }
if ($Atalho -like '*.exe') { $pasta = (Split-Path $Atalho).TrimEnd('\') + '\' }
function NaPasta($p) {
  $local = "$($p.InstallLocation)".Trim('"').TrimEnd('\')
  $icone = "$($p.DisplayIcon)".Split(',')[0].Trim('"')
  ($local -and $pasta.StartsWith($local + '\', 'OrdinalIgnoreCase')) -or ($icone -and $pasta -eq (Split-Path $icone) + '\')
}

$alvo = $programas | Where-Object DisplayName -EQ $Nome | Select-Object -First 1
if (-not $alvo -and $pasta) { $alvo = $programas | Where-Object { NaPasta $_ } | Select-Object -First 1 }
if (-not $alvo) { $alvo = $programas | Where-Object { $_.DisplayName -like "$Nome*" -or $Nome -like "$($_.DisplayName)*" } | Select-Object -First 1 }
# MSI entries carry no folder; compare names without version numbers, then a first word only one program has.
function SemVersao($t) { ($t -replace '\b\d+(\.\d+)+\b', '' -replace '\s+', ' ').Trim() }
if (-not $alvo) { $alvo = $programas | Where-Object { (SemVersao $_.DisplayName) -eq (SemVersao $Nome) } | Select-Object -First 1 }
if (-not $alvo) {
  $palavra = $Nome.Split(' ')[0]
  $mesmas = @($programas | Where-Object { $_.DisplayName.Split(' ')[0] -eq $palavra })
  if ($mesmas.Count -eq 1) { $alvo = $mesmas[0] }
}
if (-not $alvo) { exit 2 }
if ($Simular) { $alvo.DisplayName; exit 0 }

# UninstallString is a raw command line: quoted exe + args, bare path ending in .exe, or msiexec /X{GUID}.
$linha = $alvo.UninstallString.Trim()
if ($linha.StartsWith('"')) {
  $fim = $linha.IndexOf('"', 1)
  $exe, $argumentos = $linha.Substring(1, $fim - 1), $linha.Substring($fim + 1).Trim()
} else {
  $fim = $linha.ToLower().IndexOf('.exe') + 4
  $exe, $argumentos = $linha.Substring(0, $fim), $linha.Substring($fim).Trim()
}
if ($argumentos) { Start-Process $exe -ArgumentList $argumentos -Wait } else { Start-Process $exe -Wait }
