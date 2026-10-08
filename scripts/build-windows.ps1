param(
    [switch]$DebugBuild,
    [switch]$Check,
    [switch]$Test
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
if (!(Test-Path $cargo)) { $cargo = (Get-Command cargo -ErrorAction Stop).Source }
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
if (!(Test-Path $vswhere)) { throw 'Install Visual Studio Build Tools with Desktop development with C++ and a Windows SDK.' }
$installation = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (!$installation) { throw 'No x64 MSVC toolchain found. Install the C++ Build Tools workload.' }
$devcmd = Join-Path $installation 'Common7\Tools\VsDevCmd.bat'
# Pass cmd's nested quotes verbatim; PowerShell 5.1 rewrites native arguments.
$setup = New-Object System.Diagnostics.ProcessStartInfo
$setup.FileName = $env:ComSpec
$setup.Arguments = '/d /s /c ""' + $devcmd + '" -no_logo -arch=x64 -host_arch=x64 >nul && set"'
$setup.UseShellExecute = $false
$setup.CreateNoWindow = $true
$setup.RedirectStandardOutput = $true
$setup.RedirectStandardError = $true
$loader = [Diagnostics.Process]::Start($setup)
$environment = $loader.StandardOutput.ReadToEnd()
$errors = $loader.StandardError.ReadToEnd()
$loader.WaitForExit()
if ($loader.ExitCode -ne 0) { throw "Visual Studio environment setup failed: $errors" }
$environment -split "`r?`n" | ForEach-Object {
    if ($_ -match '^([^=]+)=(.*)$') { [Environment]::SetEnvironmentVariable($matches[1], $matches[2], 'Process') }
}
$loader.Dispose()
if (!$env:GPUI_FXC_PATH) {
    $sdk = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
    $fxc = Get-ChildItem $sdk -Filter fxc.exe -Recurse | Where-Object { $_.Directory.Name -eq 'x64' } | Sort-Object FullName -Descending | Select-Object -First 1
    if (!$fxc) { throw 'Windows SDK fxc.exe was not found.' }
    $env:GPUI_FXC_PATH = $fxc.FullName
}
function Invoke-Cargo {
    # PowerShell 5.1 turns redirected native stderr into ErrorRecord objects.
    # Cargo's progress belongs in the log; its exit status determines failure.
    $ErrorActionPreference = 'Continue'
    & $cargo @args 2>&1 | ForEach-Object { $_.ToString() }
    if ($LASTEXITCODE -ne 0) { throw "Cargo failed with exit code $LASTEXITCODE" }
}
Push-Location $root
try {
    if ($Check) { Invoke-Cargo check --locked --workspace --all-targets --target x86_64-pc-windows-msvc }
    elseif ($Test) { Invoke-Cargo test --locked --workspace --target x86_64-pc-windows-msvc }
    elseif ($DebugBuild) { Invoke-Cargo build --locked -p folio --target x86_64-pc-windows-msvc }
    else { Invoke-Cargo build --locked --release -p folio --target x86_64-pc-windows-msvc }
} finally { Pop-Location }
