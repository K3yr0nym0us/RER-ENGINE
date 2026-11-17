# Perfil de rendimiento del sistema de partículas GPU-driven (F6).
#
# Ejecuta `rer_engine_3d` en RELEASE con el límite de FPS subido a 240 (para medir el
# headroom real en vez del cap) y carga completa: clima lluvia→nieve→niebla + un emisor
# de cada material de escena. Parsea los eventos `debug_metrics` de stdout y reporta
# FPS medio/mínimo y uso de GPU.
#
# Uso:  powershell -File scripts/particles-perf.ps1 [-SecondsPerStep 4]

param(
    [double]$SecondsPerStep = 4.0
)

$ErrorActionPreference = "Stop"

$engineRoot = Join-Path $PSScriptRoot "..\src\main\Engine"
$target = Join-Path $engineRoot "target\release\rer_engine_3d.exe"
if (-not (Test-Path $target)) {
    Write-Host "[perf] compilando binario release…"
    Push-Location $engineRoot
    cargo build -p rer-engine --bin rer_engine_3d --no-default-features --features engine3d --release
    Pop-Location
    if ($LASTEXITCODE -ne 0) { Write-Error "La compilación release falló." }
}

$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = $target
$psi.RedirectStandardInput = $true
$psi.RedirectStandardError = $true
$psi.RedirectStandardOutput = $true
$psi.UseShellExecute = $false
# El motor resuelve plantillas/activos relativos a cwd: debe correr desde la raíz del motor.
$psi.WorkingDirectory = $engineRoot
# Perfil realista: el editor corre con frames en vuelo y sin capas de validación.
# Por defecto el motor arranca serializado (device_wait_idle por frame, modo diagnóstico).
$env:RER_VK_SYNC_FRAMES = "0"
$env:RER_VK_VALIDATION = "0"
# Perfil realista (se heredan al proceso hijo):
# - RER_VK_SYNC_FRAMES=0: el modo serializado (device_wait_idle por frame) plasta el fps.
# - RER_VK_VALIDATION=0: las capas de validación suman overhead por frame.
$process = [System.Diagnostics.Process]::Start($psi)

# Drena stdout/stderr en segundo plano (deadlock de pipes si no).
$stdoutTask = $process.StandardOutput.ReadToEndAsync()
$stderrTask = $process.StandardError.ReadToEndAsync()

$steps = @(
    @{ delay = $SecondsPerStep; lines = @(
        '{"cmd":"set_target_fps","fps":240}'
        '{"cmd":"set_particle_environment","kind":"rain","intensity":1.0}'
    ) },
    @{ delay = $SecondsPerStep; lines = @(
        '{"cmd":"set_particle_environment","kind":"snow","intensity":1.0}'
    ) },
    @{ delay = $SecondsPerStep; lines = @(
        '{"cmd":"set_particle_environment","kind":"fog","intensity":1.0}'
    ) },
    @{ delay = 0.2; lines = @(
        '{"cmd":"set_particle_environment","kind":"none"}'
        '{"cmd":"add_particle_emitter","material":"fire","position":[0,0.4,0]}'
        '{"cmd":"add_particle_emitter","material":"smoke","position":[2,0.4,0]}'
        '{"cmd":"add_particle_emitter","material":"sand","position":[-2,2,0]}'
        '{"cmd":"add_particle_emitter","material":"water","position":[3,2,0]}'
    ) },
    @{ delay = $SecondsPerStep; lines = @() },
    @{ delay = 0.5; lines = @(
        '{"cmd":"shutdown"}'
    ) }
)

try {
    foreach ($step in $steps) {
        foreach ($line in $step.lines) {
            $process.StandardInput.WriteLine($line)
            $process.StandardInput.Flush()
        }
        Start-Sleep -Seconds $step.delay
    }
} finally {
    if (-not $process.WaitForExit(20000)) {
        Write-Host "[perf] El motor no respondió a shutdown; forzando cierre."
        $process.Kill()
    }
}

$stdout = $stdoutTask.Result
$stderr = $stderrTask.Result
Set-Content -Path (Join-Path $env:TEMP "rer_particles_perf.log") -Value ($stdout + "`n=== STDERR ===`n" + $stderr)
# ¿Llegaron los comandos? ¿Qué dice el motor? (los log::info van a stderr)
Get-Content (Join-Path $env:TEMP "rer_particles_perf.log") |
    Select-String -Pattern "clima|emisor|parse error|standalone|adaptador" |
    Select-Object -First 12 | ForEach-Object { Write-Host ("[perf] log: {0}" -f $_.Line) }

# Parseo de los eventos debug_metrics (uno por segundo de stdout).
$samples = @()
foreach ($line in ($stdout -split "`n")) {
    if ($line -match '"fps":([0-9.]+)') {
        $fps = [double]::Parse($Matches[1], [System.Globalization.CultureInfo]::InvariantCulture)
        $gpu = $null
        if ($line -match '"gpu_percent":([0-9.]+)') {
            $gpu = [double]::Parse($Matches[1], [System.Globalization.CultureInfo]::InvariantCulture)
        }
        $samples += @{ fps = $fps; gpu = $gpu }
    }
}

if ($samples.Count -lt 3) {
    Write-Host "[perf] ❌ Se esperaban >= 3 muestras de métricas y llegaron $($samples.Count)."
    exit 1
}

# Descarta las primeras muestras (carga de plantilla + compilación de shaders).
$steady = @($samples | Select-Object -Skip 2)
$avg = ($steady | ForEach-Object { $_.fps } | Measure-Object -Average).Average
$min = ($steady | ForEach-Object { $_.fps } | Measure-Object -Minimum).Minimum
$gpus = $steady | Where-Object { $_.gpu -ne $null } | ForEach-Object { $_.gpu }
$gpuAvg = if ($gpus) { ($gpus | Measure-Object -Average).Average } else { $null }

# Línea de tiempo completa para diagnóstico (una muestra por segundo).
$timeline = $samples | ForEach-Object { [math]::Round($_.fps, 0) }
Write-Host ("[perf] Timeline fps: {0}" -f ($timeline -join " · "))
Set-Content -Path (Join-Path $env:TEMP "rer_particles_perf.log") -Value $stdout
# ¿Llegaron los comandos? ¿Qué dice el motor?
Get-Content (Join-Path $env:TEMP "rer_particles_perf.log") |
    Select-String -Pattern "clima|emisor|parse error|standalone|adaptador" |
    Select-Object -First 12 | ForEach-Object { Write-Host ("[perf] log: {0}" -f $_.Line) }

Write-Host ("[perf] Muestras: {0} (estables: {1})" -f $samples.Count, $steady.Count)
Write-Host ("[perf] FPS  medio: {0:N1} · mínimo: {1:N1} (límite pedido: 240)" -f $avg, $min)
if ($gpuAvg) {
    Write-Host ("[perf] GPU  media: {0:N1}%" -f $gpuAvg)
}
if ($avg -lt 60) {
    Write-Host "[perf] ❌ La carga completa de partículas cae por debajo de 60 FPS."
    exit 1
}
Write-Host "[perf] ✅ La carga completa sostiene ≥ 60 FPS."
exit 0
