# Smoke test de partículas GPU-driven (F6).
#
# Arranca `rer_engine_3d` con las capas de validación de Vulkan activadas
# (RER_VK_VALIDATION=1), ejercita los 7 materiales vía IPC por stdin y apaga el motor
# con `shutdown`. Al final escanea el log en busca de errores de validación.
#
# Uso:  powershell -File scripts/particles-smoke.ps1 [-Binary <ruta>] [-SecondsPerStep 2]

param(
    [string]$Binary = "",
    [double]$SecondsPerStep = 2.0
)

$ErrorActionPreference = "Stop"

$engineRoot = Join-Path $PSScriptRoot "..\src\main\Engine"
$target = Join-Path $engineRoot "target\debug\rer_engine_3d.exe"
if ($Binary -ne "") {
    $target = $Binary
}
if (-not (Test-Path $target)) {
    Write-Error "No existe el binario del motor: $target (compila antes con cargo build)."
}

$log = Join-Path $env:TEMP "rer_particles_smoke.log"
Remove-Item $log -ErrorAction SilentlyContinue

# Secuencia: clima en cascada + un emisor de cada material de escena + apagado.
$steps = @(
    @{ delay = 2.5; lines = @(
        '{"cmd":"set_particle_environment","kind":"rain","intensity":1.0}'
    ) },
    @{ delay = 2.5; lines = @(
        '{"cmd":"set_particle_environment","kind":"snow","intensity":1.0}'
    ) },
    @{ delay = 2.5; lines = @(
        '{"cmd":"set_particle_environment","kind":"fog","intensity":0.8}'
    ) },
    @{ delay = 2.5; lines = @(
        '{"cmd":"add_particle_emitter","material":"fire","position":[0,0.4,0]}'
        '{"cmd":"add_particle_emitter","material":"smoke","position":[2,0.4,0]}'
        '{"cmd":"add_particle_emitter","material":"sand","position":[-2,2,0]}'
        '{"cmd":"add_particle_emitter","material":"water","position":[3,2,0]}'
    ) },
    @{ delay = 4.0; lines = @() },
    @{ delay = 0.5; lines = @(
        '{"cmd":"shutdown"}'
    ) }
)

Write-Host "[smoke] Arrancando $target (validación Vulkan activa)…"

$env:RER_VK_VALIDATION = "1"

# Secuencia: clima en cascada + un emisor de cada material de escena + apagado.
$steps = @(
    @{ delay = 2.5; lines = @(
        '{"cmd":"set_particle_environment","kind":"rain","intensity":1.0}'
    ) },
    @{ delay = 2.5; lines = @(
        '{"cmd":"set_particle_environment","kind":"snow","intensity":1.0}'
    ) },
    @{ delay = 2.5; lines = @(
        '{"cmd":"set_particle_environment","kind":"fog","intensity":0.8}'
    ) },
    @{ delay = 2.5; lines = @(
        '{"cmd":"add_particle_emitter","material":"fire","position":[0,0.4,0]}'
        '{"cmd":"add_particle_emitter","material":"smoke","position":[2,0.4,0]}'
        '{"cmd":"add_particle_emitter","material":"sand","position":[-2,2,0]}'
        '{"cmd":"add_particle_emitter","material":"water","position":[3,2,0]}'
    ) },
    @{ delay = 4.0; lines = @() },
    @{ delay = 0.5; lines = @(
        '{"cmd":"shutdown"}'
    ) }
)

# PowerShell: escribir líneas con retardos a un proceso interactivo.
$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = $target
$psi.RedirectStandardInput = $true
$psi.RedirectStandardError = $true
$psi.RedirectStandardOutput = $true
$psi.UseShellExecute = $false
$psi.WorkingDirectory = (Get-Location).Path
$process = [System.Diagnostics.Process]::Start($psi)

# Lecturas asíncronas: si nadie drena stdout/stderr mientras el motor escribe los
# eventos de la plantilla, el buffer del pipe se llena, el motor se bloquea y deja
# de consumir stdin (deadlock clásico). Así se drena en paralelo.
$stdoutTask = $process.StandardOutput.ReadToEndAsync()
$stderrTask = $process.StandardError.ReadToEndAsync()

try {
    foreach ($step in $steps) {
        foreach ($line in $step.lines) {
            $process.StandardInput.WriteLine($line)
            $process.StandardInput.Flush()
        }
        Start-Sleep -Seconds $step.delay
    }
    $process.StandardInput.Flush()
} finally {
    if (-not $process.WaitForExit(20000)) {
        Write-Host "[smoke] El motor no respondió a shutdown; forzando cierre."
        $process.Kill()
    }
}

$stdout = $stdoutTask.Result
$stderr = $stderrTask.Result
Set-Content -Path $log -Value ($stdout + $stderr)

# Escaneo del log: errores de validación, avisos de fuga al destruir el device.
$failures = @()
foreach ($line in (Get-Content $log)) {
    if (
        $line -match "VUID-" -or
        $line -match "Validation Error" -or
        $line -match "leaked objects" -or
        $line -match "Vulkan error" -or
        $line -match "panicked"
    ) {
        $failures += $line
    }
}

if ($failures.Count -gt 0) {
    Write-Host "[smoke] ❌ $(($failures | Select-Object -Unique).Count) líneas sospechosas:"
    $failures | Select-Object -Unique | Select-Object -First 20 | ForEach-Object { Write-Host "    $_" }
    exit 1
}

$frames = (Select-String -Path $log -Pattern "60|fps" -SimpleMatch:$false | Measure-Object).Count
Write-Host "[smoke] ✅ Sin errores de validación. Log completo: $log"
exit 0
