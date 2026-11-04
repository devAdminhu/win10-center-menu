$ErrorActionPreference='Stop'
$root=$PSScriptRoot
New-Item -ItemType Directory -Path (Join-Path $root 'tools') -Force | Out-Null
$env:RUSTUP_HOME=Join-Path $root 'tools/rustup'
$env:CARGO_HOME=Join-Path $root 'tools/cargo'
$installer=Join-Path $root 'tools/rustup-init.exe'
if(-not (Test-Path $installer)){
 & curl.exe --fail --location --retry 2 --output $installer 'https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-gnu/rustup-init.exe'
 if($LASTEXITCODE -ne 0){throw 'Falha ao baixar rustup oficial'}
 & curl.exe --fail --location --output ($installer+'.sha256') 'https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-gnu/rustup-init.exe.sha256'
 if($LASTEXITCODE -ne 0){throw 'Falha ao obter checksum oficial'}
}
$expected=((Get-Content ($installer+'.sha256') -Raw).Trim() -split '\s+')[0]
if((Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLower() -ne $expected.ToLower()){throw 'Checksum do instalador incorreto'}
& $installer -y --profile minimal --default-host x86_64-pc-windows-gnu --default-toolchain stable --no-modify-path
if($LASTEXITCODE -ne 0){throw 'Falha ao preparar Rust'}
Write-Output 'Rust preparado em tools, sem alterar o PATH do Windows.'
