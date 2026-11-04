param([switch]$Test)
$ErrorActionPreference='Stop'
$root=$PSScriptRoot
$toolchain=Join-Path $root 'tools/rustup/toolchains/stable-x86_64-pc-windows-gnu'
$env:RUSTUP_HOME=Join-Path $root 'tools/rustup'
$env:CARGO_HOME=Join-Path $root 'tools/cargo'
$env:RUSTC=Join-Path $toolchain 'bin/rustc.exe'
$env:PATH=(Join-Path $toolchain 'lib/rustlib/x86_64-pc-windows-gnu/bin')+';'+(Join-Path $toolchain 'bin')+';'+$env:PATH
$linker=Join-Path $toolchain 'lib/rustlib/x86_64-pc-windows-gnu/bin/rust-lld.exe'
$env:RUSTFLAGS='-C linker='+$linker+' -C linker-flavor=ld.lld -C link-self-contained=yes'
Set-Location -LiteralPath $root
if($Test){& (Join-Path $toolchain 'bin/cargo.exe') test --release}
else{& (Join-Path $toolchain 'bin/cargo.exe') build --release}
if($LASTEXITCODE -ne 0){throw 'Falha ao compilar MenuRust'}
