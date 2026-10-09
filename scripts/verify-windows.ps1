param(
    [Parameter(Mandatory=$true)][string]$Package,
    [string]$Output = (Join-Path $PWD 'artifacts\validation\windows'),
    [switch]$NoUi
)
$ErrorActionPreference = 'Stop'
Add-Type @'
using System.Runtime.InteropServices;
public static class FolioValidationDisplay {
    [DllImport("kernel32.dll")]
    public static extern uint SetThreadExecutionState(uint flags);
}
'@
# Display power saving suspends native rendering in a headless VirtualBox VM.
# Keep the interactive display active only for the duration of validation.
$null = [FolioValidationDisplay]::SetThreadExecutionState(2147483651)
try {
$Package = (Resolve-Path $Package).Path
$Output = [IO.Path]::GetFullPath($Output)
New-Item -ItemType Directory -Force -Path $Output | Out-Null
$binary = Join-Path $Package 'bin\folio.exe'
$python = Join-Path $Package 'python\python.exe'
$runtimeScript = Join-Path $PSScriptRoot 'verify-windows-runtime.py'
& $python $runtimeScript --package $Package --output (Join-Path $Output 'runtime.json')
if ($LASTEXITCODE) { throw 'Offline runtime verification failed.' }
$data = Join-Path $Output ('notes-' + [guid]::NewGuid().ToString())
$smoke = Start-Process $binary -ArgumentList @('--data-dir', "`"$data`"", '--smoke-test') -PassThru -RedirectStandardOutput (Join-Path $Output 'smoke.stdout') -RedirectStandardError (Join-Path $Output 'smoke.stderr')
$null = $smoke.Handle
if (!$smoke.WaitForExit(90000)) { $smoke.Kill(); throw 'Native smoke test timed out.' }
if ($smoke.ExitCode -ne 0 -or !(Select-String -Path (Join-Path $Output 'smoke.stdout') -Pattern 'FOLIO_SMOKE_OK' -Quiet)) { throw 'Native smoke test failed; inspect smoke.stdout and smoke.stderr.' }
if (!(Test-Path (Join-Path $data 'notes.sqlite3'))) { throw 'Smoke test did not save a database.' }
if ($NoUi) { Write-Output 'WINDOWS_SMOKE_OK'; return }
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes,System.Drawing
Add-Type -Path (Join-Path $PSScriptRoot 'windows-pen-replay.cs')
function Find-Control($window, [string]$name) {
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        $condition = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, $name)
        $element = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
        if ($element) { return $element }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "UI Automation control missing: $name"
}
function Invoke-Control($window, [string]$name) {
    $element = Find-Control $window $name
    $pattern = $element.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
    $pattern.Invoke()
    Start-Sleep -Milliseconds 300
}
function Capture-Window($window, [string]$name) {
    $rect = $window.Current.BoundingRectangle
    if ($rect.Width -lt 1 -or $rect.Height -lt 1) { throw 'Window has no visible bounds.' }
    $bitmap = New-Object Drawing.Bitmap([int]$rect.Width, [int]$rect.Height)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen([int]$rect.X, [int]$rect.Y, 0, 0, $bitmap.Size)
        $bitmap.Save((Join-Path $Output $name), [Drawing.Imaging.ImageFormat]::Png)
    } finally { $graphics.Dispose(); $bitmap.Dispose() }
}
$uiData = Join-Path $Output ('pen-notes-' + [guid]::NewGuid().ToString())
$app = Start-Process $binary -ArgumentList @('--data-dir', "`"$uiData`"", '--new-note') -PassThru -RedirectStandardOutput (Join-Path $Output 'ui.stdout') -RedirectStandardError (Join-Path $Output 'ui.stderr')
$null = $app.Handle
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    do {
        $app.Refresh()
        if ($app.HasExited) { throw 'App exited before opening a window.' }
        if ($app.MainWindowHandle -ne 0) { break }
        Start-Sleep -Milliseconds 200
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($app.MainWindowHandle -eq 0) { throw 'No Folio window appeared.' }
    $window = [System.Windows.Automation.AutomationElement]::FromHandle($app.MainWindowHandle)
    Find-Control $window 'Close window' | Out-Null
    $homeCondition = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, ('Library ' + [char]0xB7 + ' Ctrl+Shift+L'))
    if ($window.FindAll([System.Windows.Automation.TreeScope]::Descendants,$homeCondition).Count -ne 1) { throw 'Editor must have exactly one Library/Home button.' }
    [FolioPenReplay]::SetForegroundWindow($app.MainWindowHandle) | Out-Null
    $before = $window.Current.BoundingRectangle
    [FolioPenReplay]::DragMouse([int]($before.X + $before.Width * .5), [int]($before.Y + 20), 50, 20)
    $moved = $window.Current.BoundingRectangle
    if ([Math]::Abs($moved.X - $before.X) -lt 20 -or [Math]::Abs($moved.Width - $before.Width) -gt 5) { throw 'Custom title bar did not move the native window.' }
    [FolioPenReplay]::DragMouse([int]($moved.Right - 2), [int]($moved.Y + $moved.Height * .5), -40, 0)
    $resized = $window.Current.BoundingRectangle
    if ($moved.Width - $resized.Width -lt 20) { throw 'Custom edge did not resize the native window.' }
    $canvas = (Find-Control $window 'Page 1').Current.BoundingRectangle
    [FolioPenReplay]::Stroke([int]($canvas.X + $canvas.Width * .35), [int]($canvas.Y + $canvas.Height * .4))
    Start-Sleep -Milliseconds 500
    Capture-Window $window 'editor.png'
    $library = (Find-Control $window ('Library ' + [char]0xB7 + ' Ctrl+Shift+L')).Current.BoundingRectangle
    [FolioPenReplay]::Tap([int]($library.X + $library.Width / 2), [int]($library.Y + $library.Height / 2))
    Find-Control $window 'Documents' | Out-Null
    $optionsCondition = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, ('New document with options' + [char]0x2026))
    if ($window.FindAll([System.Windows.Automation.TreeScope]::Descendants,$optionsCondition).Count -ne 0) { throw 'Removed sliders button is still exposed.' }
    Capture-Window $window 'library.png'
    Invoke-Control $window 'New document'
    Find-Control $window 'Create document' | Out-Null
    Find-Control $window ('A5 ' + [char]0xB7 + ' 148 ' + [char]0xD7 + ' 210 mm') | Out-Null
    Capture-Window $window 'creation-options.png'
    Invoke-Control $window 'Cancel'
    Invoke-Control $window 'Settings'
    Capture-Window $window 'settings.png'
    Invoke-Control $window 'Close window'
    if (!$app.WaitForExit(20000)) { throw 'Window close did not flush and exit.' }
    & $python $runtimeScript --pen-database (Join-Path $uiData 'notes.sqlite3') --output (Join-Path $Output 'pen.json')
    if ($LASTEXITCODE) { throw 'Native pen verification failed.' }
    @{native_move_resize=$true; native_pen_controls=$true; native_smoke=$true; durable_save=$true; uia_controls=$true; uia_actions=$true; close_flush=$true; main_button_creation_options=$true; single_library_button=$true; no_creation_sliders=$true} | ConvertTo-Json | Set-Content (Join-Path $Output 'ui.json') -Encoding UTF8
    Write-Output 'WINDOWS_UI_OK'
} finally {
    if (!$app.HasExited) {
        $app.CloseMainWindow() | Out-Null
        if (!$app.WaitForExit(5000)) { $app.Kill() }
    }
}
} finally {
    $null = [FolioValidationDisplay]::SetThreadExecutionState(2147483648)
}
