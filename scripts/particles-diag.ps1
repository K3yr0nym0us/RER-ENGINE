# Diagnóstico F6-bis: captura la ventana del motor antes/después de activar la niebla.
# Deja dos PNG en %TEMP% para comparar el flip reportado por el usuario.

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Win32 {
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr hWnd, int x, int y, int w, int h, bool repaint);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hWnd, IntPtr after, int x, int y, int w, int h, uint flags);
    public struct RECT { public int Left, Top, Right, Bottom; }
}
"@

$engineRoot = Join-Path $PSScriptRoot "..\src\main\Engine"
$target = Join-Path $engineRoot "target\release\rer_engine_3d.exe"
if (-not (Test-Path $target)) { Write-Error "Falta el binario: $target" }

$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = $target
$psi.RedirectStandardInput = $true
$psi.RedirectStandardError = $true
$psi.RedirectStandardOutput = $true
$psi.UseShellExecute = $false
$psi.WorkingDirectory = $engineRoot
$env:RER_VK_SYNC_FRAMES = "0"
$env:RER_VK_VALIDATION = "0"
$process = [System.Diagnostics.Process]::Start($psi)
$stdoutTask = $process.StandardOutput.ReadToEndAsync()
$stderrTask = $process.StandardError.ReadToEndAsync()

function Send-Command($process, $json) {
    $process.StandardInput.WriteLine($json)
    $process.StandardInput.Flush()
}

function Capture-Window($process, $path) {
    # La ventana puede tardar en aparecer y titularse.
    $handle = [IntPtr]::Zero
    for ($i = 0; $i -lt 50 -and $handle -eq [IntPtr]::Zero; $i++) {
        $process.Refresh()
        $handle = $process.MainWindowHandle
        if ($handle -eq [IntPtr]::Zero) { Start-Sleep -Milliseconds 200 }
    }
    if ($handle -eq [IntPtr]::Zero) { Write-Host "[diag] ventana no encontrada"; return }
    # Se fuerza la posición (60, 60) y se sube a tope: el fondo de pantalla cambia
    # mientras tanto y la captura podría tomar otra ventana.
    [Win32]::MoveWindow($handle, 60, 60, 1200, 700, $true) | Out-Null
    [Win32]::SetWindowPos($handle, [IntPtr](-1), 0, 0, 0, 0, 0x0001 -bor 0x0002) | Out-Null  # HWND_TOPMOST
    Start-Sleep -Milliseconds 600
    $rect = New-Object Win32+RECT
    [Win32]::GetWindowRect($handle, [ref]$rect) | Out-Null
    $width = $rect.Right - $rect.Left
    $height = $rect.Bottom - $rect.Top
    if ($width -le 0 -or $height -le 0) { Write-Host "[diag] rect inválido"; return }
    $bitmap = New-Object System.Drawing.Bitmap($width, $height)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, (New-Object System.Drawing.Size($width, $height)))
    $bitmap.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    $graphics.Dispose()
    $bitmap.Dispose()
    # Vuelve a NO topmost para no estorbar.
    [Win32]::SetWindowPos($handle, [IntPtr](-2), 0, 0, 0, 0, 0x0001 -bor 0x0002) | Out-Null  # HWND_NOTOPMOST
    Write-Host "[diag] captura: $path ($width x $height)"
}

# Arranque + plantilla
Start-Sleep -Seconds 5
Send-Command $process '{"cmd":"set_target_fps","fps":60}'
Start-Sleep -Seconds 3
Capture-Window $process (Join-Path $env:TEMP "diag_antes.png")

# Activar niebla y capturar en varios momentos (aparece con retardo reportado).
Send-Command $process '{"cmd":"set_particle_environment","kind":"fog","intensity":1.0}'
Start-Sleep -Seconds 4
Capture-Window $process (Join-Path $env:TEMP "diag_fog_4s.png")
Start-Sleep -Seconds 8
Capture-Window $process (Join-Path $env:TEMP "diag_fog_12s.png")

Send-Command $process '{"cmd":"shutdown"}'
if (-not $process.WaitForExit(15000)) { $process.Kill() }
$null = $stdoutTask.Result
$stderr = $stderrTask.Result
$stderr | Select-String -Pattern "clima|emisor|error" | Select-Object -First 8 | ForEach-Object { Write-Host $_.Line }
