# Publicación del artefacto del plugin world-gen (Windows).
#
# Construye `rer_worldgen.dll` en release, lo empaqueta en un ZIP con el nombre
# que espera el catálogo del editor y calcula el SHA-256 para pegarlo en
# `pluginCatalog.ts`.
#
# Uso:
#   powershell -File scripts\publish-worldgen.ps1 [-Version "1.0.0"]
#
# Después: subir el ZIP a un release de GitHub (tag `worldgen-v<Version>`) y
# copiar el SHA-256 al campo `artifact.sha256` del catálogo.

param(
    [string]$Version = "1.0.0"
)

$ErrorActionPreference = "Stop"

$engineDir = Join-Path $PSScriptRoot "..\src\main\Engine"
$dllPath = Join-Path $engineDir "target\release\rer_worldgen.dll"
$outDir = Join-Path $engineDir "dist\worldgen"
$zipName = "rer-worldgen-$Version-win-x64.zip"
$zipPath = Join-Path $outDir $zipName

Write-Host "== Build release del crate worldgen =="
Push-Location $engineDir
cargo build -p rer-worldgen --release
if ($LASTEXITCODE -ne 0) { Pop-Location; throw "cargo build falló" }
Pop-Location

if (-not (Test-Path $dllPath)) { throw "No se encontró la DLL: $dllPath" }

New-Item -ItemType Directory -Force -Path $outDir | Out-Null
if (Test-Path $zipPath) { Remove-Item $zipPath -Force }

# El catálogo espera el binario como `rer_worldgen.dll` dentro del ZIP.
$stage = Join-Path $outDir "stage"
if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
New-Item -ItemType Directory -Force -Path $stage | Out-Null
Copy-Item $dllPath (Join-Path $stage "rer_worldgen.dll")
Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zipPath
Remove-Item $stage -Recurse -Force

$sha = (Get-FileHash $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
$size = (Get-Item $zipPath).Length

Write-Host ""
Write-Host "Artefacto listo:"
Write-Host "  ZIP      : $zipPath"
Write-Host "  Tamaño   : $size bytes"
Write-Host "  SHA-256  : $sha"
Write-Host ""
Write-Host "Pasos siguientes:"
Write-Host "  1. Crea el release en GitHub con tag 'worldgen-v$Version'."
Write-Host "  2. Sube $zipName como asset."
Write-Host "  3. Actualiza en src/main/plugins/pluginCatalog.ts:"
Write-Host "     downloadUrl -> URL del asset"
Write-Host "     sizeBytes   -> $size"
Write-Host "     sha256      -> `"$sha`""
