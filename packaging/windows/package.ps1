# Native MSVC build and verified, non-publishing experimental archive.
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not $IsWindows -or [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture -ne 'X64') {
    throw 'Run with PowerShell 7 on native x86-64 Windows.'
}
$root = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
Push-Location $root
try {
    foreach ($tool in @('cargo', 'rustc', 'rc.exe', 'cl.exe', 'fxc.exe', 'python', 'cargo-about')) {
        Get-Command $tool -ErrorAction Stop | Out-Null
    }
    $hostInfo = & rustc +stable -vV
    if ($LASTEXITCODE -ne 0 -or $hostInfo -notcontains 'host: x86_64-pc-windows-msvc') {
        throw 'Install stable x86_64-pc-windows-msvc Rust and use an x64 MSVC developer shell.'
    }
    & python -c 'import pefile; assert pefile.__version__ == "2024.8.26"'
    if ($LASTEXITCODE -ne 0) { throw 'Install packaging/windows/requirements.txt first.' }
    $aboutVersion = & cargo-about --version
    if ($LASTEXITCODE -ne 0 -or $aboutVersion -ne 'cargo-about 0.9.3') {
        throw 'Install cargo-about 0.9.3 (cargo +stable install cargo-about --locked --version 0.9.3 --features cli).'
    }
    & cargo +stable build --locked --release --target x86_64-pc-windows-msvc --bin kver
    if ($LASTEXITCODE -ne 0) { throw 'Native MSVC release build failed.' }
    $target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { 'target' }
    $release = Join-Path $target 'x86_64-pc-windows-msvc/release'
    $licenses = Join-Path $release 'THIRD-PARTY-LICENSES.html'
    & cargo +stable about generate --locked --fail -c packaging/windows/about.toml `
        -o $licenses packaging/common/licenses.hbs
    if ($LASTEXITCODE -ne 0) { throw 'Third-party license generation failed.' }
    & python packaging/windows/package.py --release-dir $release --licenses $licenses
    if ($LASTEXITCODE -ne 0) { throw 'Archive verification failed.' }
} finally {
    Pop-Location
}
