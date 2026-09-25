# Gera os arquivos de uma versão do Swoop em dist/:
#   Swoop-Installer-<versão>.exe   instalador personalizado (janela própria, pede administrador)
#   Swoop_<versão>_x64-setup.exe   setup NSIS padrão (Arquivos de Programas; usado pela atualização)
#   SHA256SUMS.txt                 hashes conferidos pela atualização automática
#
# Uso (Windows, na raiz do repositório): pwsh scripts/build-release.ps1
# Requer Rust estável e Node 22. O Tauri baixa o NSIS sozinho na primeira vez.
#
# Pasta Música: builds dentro de C:\Users\...\Music podem falhar no rename com
# EPERM (o WMPNetworkSvc trava pastas novas). Rode o script num clone fora dela.

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches.Groups[1].Value
if (-not $version) { throw 'versão não encontrada no Cargo.toml' }
Write-Host "Swoop $version"

if (-not (Test-Path ui/node_modules)) {
    Push-Location ui
    npm ci
    Pop-Location
}

# 1. Setup NSIS do app (o beforeBuildCommand gera a UI).
Push-Location apps/swoop
& ../../ui/node_modules/.bin/tauri.cmd build --bundles nsis
if ($LASTEXITCODE -ne 0) { throw "tauri build falhou ($LASTEXITCODE)" }
Pop-Location

$setupName = "Swoop_${version}_x64-setup.exe"
$setup = Join-Path $root "target/release/bundle/nsis/$setupName"
if (-not (Test-Path $setup)) { throw "setup não encontrado: $setup" }

# 2. Instalador personalizado com o setup embutido.
$env:SWOOP_SETUP_PAYLOAD = $setup
cargo build --release -p swoop-installer
if ($LASTEXITCODE -ne 0) { throw "build do instalador falhou ($LASTEXITCODE)" }
Remove-Item Env:SWOOP_SETUP_PAYLOAD

# 3. dist/ + hashes.
$dist = Join-Path $root 'dist'
Remove-Item $dist -Recurse -Force -ErrorAction SilentlyContinue
New-Item $dist -ItemType Directory | Out-Null
Copy-Item $setup (Join-Path $dist $setupName)
Copy-Item (Join-Path $root 'target/release/swoop-installer.exe') (Join-Path $dist "Swoop-Installer-$version.exe")

Get-ChildItem $dist -File -Filter *.exe | Sort-Object Name | ForEach-Object {
    '{0}  {1}' -f (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower(), $_.Name
} | Set-Content (Join-Path $dist 'SHA256SUMS.txt') -Encoding ascii

Get-ChildItem $dist | Format-Table Name, Length
Get-Content (Join-Path $dist 'SHA256SUMS.txt')
