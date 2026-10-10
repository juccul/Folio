param(
 [Parameter(Mandatory=$true)][string]$Package,
 [string]$Output=(Join-Path (Split-Path -Parent $PSScriptRoot) ('artifacts\validation\windows-titlebar-'+[guid]::NewGuid().ToString())),
 [switch]$ConfirmDisposableVm,
 [string[]]$Locations=@('center','upper','lower','left','right'),
 [string[]]$WindowStates=@('restored','maximized'),
 [int[]]$HoverMilliseconds=@(0,100),
 [int[]]$PressMilliseconds=@(20,100),
 [string]$FixtureNote='Untitled document'
)
$ErrorActionPreference='Stop'
if(!$ConfirmDisposableVm){throw 'Run only in the owned Windows VM with -ConfirmDisposableVm.'}
if($env:OS -ne 'Windows_NT' -or (Get-CimInstance Win32_ComputerSystem).Model -notmatch 'VirtualBox'){throw 'This physical pointer harness requires the owned VirtualBox Windows guest.'}
# --smoke-titlebar disables Updater before app construction and requires a fresh
# explicit library. It is safe alongside another Folio process: every operation
# below targets only the process started by this script. No updater-cache access,
# installation, registry writes, Restart click, or termination of other apps.
$packagePath=(Resolve-Path -LiteralPath $Package).Path
$binary=Join-Path $packagePath 'bin\folio.exe'
if(!(Test-Path -LiteralPath $binary)){throw 'Package must contain bin\folio.exe.'}
$binaryHash=(Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash.ToLowerInvariant()
$manifestPath=Join-Path $packagePath 'manifest.json'
if(Test-Path -LiteralPath $manifestPath){
 $packageManifest=Get-Content -LiteralPath $manifestPath -Raw|ConvertFrom-Json
 if($packageManifest.binary_sha256 -and $packageManifest.binary_sha256 -ne $binaryHash){throw 'Package manifest does not identify the executable being tested.'}
}
$Output=[IO.Path]::GetFullPath($Output)
if(Test-Path -LiteralPath $Output){throw 'Use a fresh evidence directory.'}
New-Item -ItemType Directory -Path $Output -Force|Out-Null
$runRoot=Join-Path ([IO.Path]::GetTempPath()) ('FolioTitlebarPointerV2-'+[guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $runRoot|Out-Null
$data=Join-Path $runRoot 'fixture-library'
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes,System.Drawing
if(-not ('FolioTitlebarPointerV2' -as [type])){
Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Threading;
public static class FolioTitlebarPointerV2 {
 [DllImport("kernel32.dll")] public static extern uint SetThreadExecutionState(uint flags);
 [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
 [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
 [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hwnd,int command);
 [DllImport("user32.dll")] public static extern bool IsZoomed(IntPtr hwnd);
 [DllImport("user32.dll",SetLastError=true)] public static extern bool SetWindowPos(IntPtr hwnd,IntPtr after,int x,int y,int width,int height,uint flags);
 [DllImport("user32.dll")] static extern bool SetCursorPos(int x,int y);
 [DllImport("user32.dll")] static extern void mouse_event(uint flags,uint x,uint y,uint data,UIntPtr extra);
 [DllImport("user32.dll")] static extern void keybd_event(byte key,byte scan,uint flags,UIntPtr extra);
 [DllImport("user32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern IntPtr SendMessageTimeout(IntPtr hwnd,uint message,UIntPtr wparam,IntPtr lparam,uint flags,uint timeout,out UIntPtr result);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern IntPtr FindWindowEx(IntPtr parent,IntPtr after,string classname,string name);
 [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr hwnd);
 [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hwnd,out uint process);
 [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hwnd);
 public static void Move(int x,int y){if(!SetCursorPos(x,y))throw new Exception("SetCursorPos failed");}
 public static void Click(int x,int y,int hover,int press,bool right){
  Move(x,y); if(hover>0)Thread.Sleep(hover);
  mouse_event(right?8u:2u,0,0,0,UIntPtr.Zero);
  Thread.Sleep(press); mouse_event(right?16u:4u,0,0,0,UIntPtr.Zero);
 }
 public static void Drag(int x,int y,int dx,int dy){
  Move(x,y);Thread.Sleep(100);mouse_event(2,0,0,0,UIntPtr.Zero);
  try{Thread.Sleep(100);for(int step=1;step<=6;step++){Move(x+dx*step/6,y+dy*step/6);Thread.Sleep(40);}}
  finally{mouse_event(4,0,0,0,UIntPtr.Zero);}Thread.Sleep(300);
 }
 public static void DoubleClick(int x,int y){
  Click(x,y,100,20,false);Thread.Sleep(60);Click(x,y,0,20,false);Thread.Sleep(400);
 }
 public static int HitTest(IntPtr hwnd,int x,int y){
  long packed=unchecked((int)(((uint)(ushort)y<<16)|(uint)(ushort)x));
  UIntPtr result; if(SendMessageTimeout(hwnd,0x84,UIntPtr.Zero,new IntPtr(packed),2,2000,out result)==IntPtr.Zero)throw new Exception("WM_NCHITTEST timeout/error "+Marshal.GetLastWin32Error());
  return unchecked((int)result.ToUInt64());
 }
 public static bool HasNativeMenu(uint process){
  IntPtr h=IntPtr.Zero;
  while((h=FindWindowEx(IntPtr.Zero,h,"#32768",null))!=IntPtr.Zero){uint owner;GetWindowThreadProcessId(h,out owner);if(owner==process&&IsWindowVisible(h))return true;}
  return false;
 }
 public static void ResetUpdate(){
  keybd_event(0x11,0,0,UIntPtr.Zero);keybd_event(0x12,0,0,UIntPtr.Zero);keybd_event(0x55,0,0,UIntPtr.Zero);
  Thread.Sleep(30);keybd_event(0x55,0,2,UIntPtr.Zero);keybd_event(0x12,0,2,UIntPtr.Zero);keybd_event(0x11,0,2,UIntPtr.Zero);
 }
 public static void Escape(){keybd_event(0x1b,0,0,UIntPtr.Zero);Thread.Sleep(30);keybd_event(0x1b,0,2,UIntPtr.Zero);}
}
'@
}
function Node($window,[string]$name,[int]$seconds=5){
 $deadline=[DateTime]::UtcNow.AddSeconds($seconds)
 $condition=New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty,$name)
 do{try{$node=$window.FindFirst([System.Windows.Automation.TreeScope]::Descendants,$condition);if($node -and !$node.Current.IsOffscreen){return $node}}catch{}
 Start-Sleep -Milliseconds 40}while([DateTime]::UtcNow -lt $deadline)
 throw ('Missing visible owned control: '+$name)
}
function Owned-Window($process){
 $deadline=[DateTime]::UtcNow.AddSeconds(45)
 do{$process.Refresh();if($process.HasExited){throw 'Fixture exited at startup.'}
 if($process.MainWindowHandle -ne [IntPtr]::Zero){try{$window=[System.Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle);if($window.Current.ProcessId -eq $process.Id){$null=Node $window 'Close window' 1;return $window}}catch{}}
 Start-Sleep -Milliseconds 100}while([DateTime]::UtcNow -lt $deadline)
 throw 'No ready window owned by fixture process within45s.'
}
function Focus-Owned($window,$process){
 if($window.Current.ProcessId -ne $process.Id){throw 'Window ownership changed.'}
 $handle=[IntPtr]$window.Current.NativeWindowHandle
 [FolioTitlebarPointerV2]::SetForegroundWindow($handle)|Out-Null
 Start-Sleep -Milliseconds 100
 if([FolioTitlebarPointerV2]::GetForegroundWindow() -ne $handle){throw 'Owned fixture could not gain foreground focus; do not send input.'}
}
function Bounds($window){$r=$window.Current.BoundingRectangle;return @{X=$r.X;Y=$r.Y;Width=$r.Width;Height=$r.Height}}
function Check-Bounds($before,$after){foreach($key in @('X','Y','Width','Height')){if([Math]::Abs($before[$key]-$after[$key]) -gt 2){throw ('Control press unexpectedly moved/resized/maximized fixture: '+$key)}}}
function Point-For($node,[string]$location){
 $r=$node.Current.BoundingRectangle
 if($r.Width -lt 8 -or $r.Height -lt 8 -or $node.Current.IsOffscreen){throw 'Pointer target is not visible or too small.'}
 $x=[int][Math]::Floor($r.X+$r.Width/2);$y=[int][Math]::Floor($r.Y+$r.Height/2)
 switch($location){'center'{};'upper'{$y=[int][Math]::Ceiling($r.Y+2)};'lower'{$y=[int][Math]::Floor($r.Bottom-2)};'left'{$x=[int][Math]::Ceiling($r.X+2)};'right'{$x=[int][Math]::Floor($r.Right-2)};default{throw ('Unknown target location: '+$location)}}
 return @{X=$x;Y=$y;control_bounds=@{X=$r.X;Y=$r.Y;Width=$r.Width;Height=$r.Height}}
}
function Capture($window,[string]$name){
 $r=$window.Current.BoundingRectangle
 $screen=[System.Windows.Forms.Screen]::PrimaryScreen.Bounds
 # Maximized windows can have hidden8px borders with negative bounds. Capture
 # only the visible intersection; record whole window bounds separately.
 $left=[int][Math]::Max($screen.Left,$r.Left);$top=[int][Math]::Max($screen.Top,$r.Top)
 $width=[int]([Math]::Min($screen.Right,$r.Right)-$left);$height=[int]([Math]::Min($screen.Bottom,$r.Bottom)-$top)
 if($width -le 0 -or $height -le 0){throw 'Fixture is outside visible screen.'}
 $bitmap=New-Object Drawing.Bitmap($width,$height);$graphics=[Drawing.Graphics]::FromImage($bitmap)
 try{$graphics.CopyFromScreen($left,$top,0,0,$bitmap.Size);$bitmap.Save((Join-Path $Output $name),[Drawing.Imaging.ImageFormat]::Png)}finally{$graphics.Dispose();$bitmap.Dispose()}
}
function Names($window){return @($window.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)|ForEach-Object{$_.Current.Name})}
function Click-Target($window,$process,$node,[string]$location,[int]$hover,[int]$press,[bool]$right=$false){
 Focus-Owned $window $process
 $r=$window.Current.BoundingRectangle
 # Start outside the titlebar before every sweep, without a preparatory hover
 # at the actual target or WM_NCHITTEST query that might prime a stale hit map.
 [FolioTitlebarPointerV2]::Move([int]($r.X+$r.Width/2),[int]($r.Y+$r.Height/2))
 Start-Sleep -Milliseconds 100
 $point=Point-For $node $location
 [FolioTitlebarPointerV2]::Click($point.X,$point.Y,$hover,$press,$right)
 Start-Sleep -Milliseconds 150
 # Diagnostic queried AFTER physical press: it cannot improve pre-click state.
 $point.hit_test_after_press=[FolioTitlebarPointerV2]::HitTest([IntPtr]$window.Current.NativeWindowHandle,$point.X,$point.Y)
 $point.native_system_menu=[FolioTitlebarPointerV2]::HasNativeMenu([uint32]$process.Id)
 return $point
}
function Blank-Point($window,[string]$availableLabel){
 $plus=(Node $window ('Open or create a document '+[char]0xB7+' Ctrl+T')).Current.BoundingRectangle
 $update=(Node $window $availableLabel).Current.BoundingRectangle
 # Both adjacent rendered controls bound the explicit flex spacer. Stay12px
 # away from each edge and vertically within their shared band, avoiding the
 # native4px top resize strip and any tab/control hitbox.
 $left=[Math]::Ceiling($plus.Right+12);$right=[Math]::Floor($update.Left-12)
 $top=[Math]::Max($plus.Top,$update.Top);$bottom=[Math]::Min($plus.Bottom,$update.Bottom)
 if($right-$left -lt 24 -or $bottom-$top -lt 12){throw 'No unambiguous blank titlebar spacer between New tab and Update; cannot safely test drag.'}
 return @{X=[int][Math]::Floor(($left+$right)/2);Y=[int][Math]::Floor(($top+$bottom)/2);spacer_left=$left;spacer_right=$right;shared_top=$top;shared_bottom=$bottom}
}
function Wait-Zoomed($window,$process,[bool]$expected,[string]$action){
 $clock=[Diagnostics.Stopwatch]::StartNew();$deadline=[DateTime]::UtcNow.AddSeconds(5)
 $hwnd=[IntPtr]$window.Current.NativeWindowHandle
 do{
  $process.Refresh();if($process.HasExited){throw ('Owned fixture exited while waiting for '+$action)}
  $actual=[FolioTitlebarPointerV2]::IsZoomed($hwnd)
  if($actual -eq $expected){return @{expected_zoomed=$expected;actual_zoomed=$actual;elapsed_ms=$clock.ElapsedMilliseconds;one_pointer_action_only=$true}}
  Start-Sleep -Milliseconds 50
 }while([DateTime]::UtcNow -lt $deadline)
 throw ($action+' did not reach expected native IsZoomed='+$expected+' within5s. No click retry was issued.')
}
function Restore-Owned-Bounds($window,$process,$before){
 Focus-Owned $window $process;$hwnd=[IntPtr]$window.Current.NativeWindowHandle
 [FolioTitlebarPointerV2]::ShowWindow($hwnd,9)|Out-Null
 Start-Sleep -Milliseconds 250
 if(![FolioTitlebarPointerV2]::SetWindowPos($hwnd,[IntPtr]::Zero,[int]$before.X,[int]$before.Y,[int]$before.Width,[int]$before.Height,0x14)){throw 'Cannot restore owned fixture bounds after blank titlebar test.'}
 Start-Sleep -Milliseconds 350
 Check-Bounds $before (Bounds $window)
}
Add-Type -AssemblyName System.Windows.Forms
$app=$null;$results=@();$failed=0;$blankPassed=$false;$windowControlsPassed=$false
$null=[FolioTitlebarPointerV2]::SetThreadExecutionState(2147483651)
try{
 $app=Start-Process -FilePath $binary -ArgumentList @('--data-dir',('"'+$data+'"'),'--smoke-titlebar') -PassThru -RedirectStandardOutput (Join-Path $Output 'fixture.stdout') -RedirectStandardError (Join-Path $Output 'fixture.stderr')
 $null=$app.Handle;$window=Owned-Window $app
 $handle=[IntPtr]$window.Current.NativeWindowHandle
 $environment=@{binary_sha256=$binaryHash;owned_pid=$app.Id;owned_hwnd=$handle.ToInt64();dpi=[FolioTitlebarPointerV2]::GetDpiForWindow($handle);screen_width=[System.Windows.Forms.Screen]::PrimaryScreen.Bounds.Width;screen_height=[System.Windows.Forms.Screen]::PrimaryScreen.Bounds.Height;fixture_data_dir=$data;locations=$Locations;window_states=$WindowStates;hover_ms=$HoverMilliseconds;press_ms=$PressMilliseconds;network_or_install_actions=$false;updater_cache_access=$false;preexisting_folio_processes_allowed=$true}
 $environment|ConvertTo-Json -Depth 5|Set-Content (Join-Path $Output 'environment.json') -Encoding UTF8
 $available='Update '+[char]0xB7+' 0.1.4';$homeLabel='Library '+[char]0xB7+' Ctrl+Shift+L'
 $null=Node $window $available 10
 # Regression gate: empty chrome must still drag and double-click maximize/
 # restore after removing broad titlebar handlers. Only the owned fixture moves.
 $blank=@{drag_passed=$false;double_click_maximize_passed=$false;double_click_restore_passed=$false}
 $blankOriginal=$null
 try{
  Focus-Owned $window $app
  [FolioTitlebarPointerV2]::ShowWindow($handle,9)|Out-Null;Start-Sleep -Milliseconds 500
  $blankOriginal=Bounds $window;$point=Blank-Point $window $available
  $blank.initial_bounds=$blankOriginal;$blank.drag_pointer=$point
  Capture $window 'blank-titlebar-before-drag.png'
  [FolioTitlebarPointerV2]::Drag($point.X,$point.Y,24,18)
  $dragAfter=Bounds $window;$blank.drag_after_bounds=$dragAfter
  $dx=$dragAfter.X-$blankOriginal.X;$dy=$dragAfter.Y-$blankOriginal.Y
  if([Math]::Abs($dx-24) -gt 4 -or [Math]::Abs($dy-18) -gt 4){throw ('Blank titlebar drag did not move the fixture by24,18px: actual '+$dx+','+$dy)}
  if([Math]::Abs($dragAfter.Width-$blankOriginal.Width) -gt 2 -or [Math]::Abs($dragAfter.Height-$blankOriginal.Height) -gt 2 -or [FolioTitlebarPointerV2]::IsZoomed($handle)){throw 'Blank titlebar drag resized or maximized the fixture.'}
  $blank.drag_passed=$true;Capture $window 'blank-titlebar-dragged.png'
  Restore-Owned-Bounds $window $app $blankOriginal
  $point=Blank-Point $window $available;$blank.double_click_restored_pointer=$point
  [FolioTitlebarPointerV2]::DoubleClick($point.X,$point.Y)
  $blank.maximize_wait=Wait-Zoomed $window $app $true 'Blank titlebar double-click maximize'
  $null=Node $window 'Restore window' 5
  $blank.double_click_maximize_passed=$true;Capture $window 'blank-titlebar-double-click-maximized.png'
  $point=Blank-Point $window $available;$blank.double_click_maximized_pointer=$point
  [FolioTitlebarPointerV2]::DoubleClick($point.X,$point.Y)
  $blank.restore_wait=Wait-Zoomed $window $app $false 'Blank titlebar double-click restore'
  $null=Node $window 'Maximize window' 5
  $blank.double_click_restore_passed=$true;Capture $window 'blank-titlebar-double-click-restored.png'
  $blankPassed=$true
 }catch{
  $failed++;$blank.error=$_.Exception.Message
  try{Capture $window 'blank-titlebar-failure.png'}catch{$blank.diagnostic_error=$_.Exception.Message}
 }finally{
  if($blankOriginal){Restore-Owned-Bounds $window $app $blankOriginal}
  $blank|ConvertTo-Json -Depth 6|Set-Content (Join-Path $Output 'blank-titlebar-results.json') -Encoding UTF8
 }
 # Separate actual custom-control gate; test even if a blank-titlebar gate
 # failed. Poll native asynchronous state, never issue a second/retry click.
 $customControls=@{maximize_passed=$false;restore_passed=$false};$customOriginal=$null
 try{
  Focus-Owned $window $app
  [FolioTitlebarPointerV2]::ShowWindow($handle,9)|Out-Null
  $null=Wait-Zoomed $window $app $false 'Custom-control gate setup';Start-Sleep -Milliseconds 300
  $customOriginal=Bounds $window
  $customControls.initial_bounds=$customOriginal
  $customControls.maximize_pointer=Click-Target $window $app (Node $window 'Maximize window' 5) 'center' 0 20
  $customControls.maximize_wait=Wait-Zoomed $window $app $true 'Physical Maximize window click'
  $restoreControl=Node $window 'Restore window' 5
  $customControls.maximize_passed=$true
  Capture $window 'custom-control-maximized.png'
  $customControls.restore_pointer=Click-Target $window $app $restoreControl 'center' 0 20
  $customControls.restore_wait=Wait-Zoomed $window $app $false 'Physical Restore window click'
  $null=Node $window 'Maximize window' 5
  $customControls.restore_passed=$true
  $customControls.restored_bounds=Bounds $window
  Check-Bounds $customOriginal (Bounds $window)
  Capture $window 'custom-control-restored.png'
  $windowControlsPassed=$true
 }catch{
  $failed++;$customControls.error=$_.Exception.Message
  try{Capture $window 'custom-control-failure.png'}catch{$customControls.diagnostic_error=$_.Exception.Message}
 }finally{
  if($customOriginal){Restore-Owned-Bounds $window $app $customOriginal}
  $customControls|ConvertTo-Json -Depth 7|Set-Content (Join-Path $Output 'custom-window-controls-results.json') -Encoding UTF8
 }
 foreach($state in $WindowStates){
  Focus-Owned $window $app
  switch($state){'restored'{[FolioTitlebarPointerV2]::ShowWindow($handle,9)|Out-Null};'maximized'{[FolioTitlebarPointerV2]::ShowWindow($handle,3)|Out-Null};default{throw ('Unknown window state: '+$state)}}
  Start-Sleep -Milliseconds 700
  if([FolioTitlebarPointerV2]::IsZoomed($handle) -ne ($state -eq 'maximized')){throw 'Requested window state was not applied.'}
  Capture $window ($state+'-available.png')
  foreach($location in $Locations){foreach($hover in $HoverMilliseconds){foreach($press in $PressMilliseconds){
   $caseName=$state+'-'+$location+'-hover'+$hover+'-press'+$press
   $case=@{case=$caseName;window_state=$state;target_location=$location;hover_ms=$hover;press_ms=$press;home_passed=$false;update_passed=$false;disabled_progress_passed=$false}
   try{
    Focus-Owned $window $app
    [FolioTitlebarPointerV2]::ResetUpdate();$null=Node $window $available 5
    # Select the already-created fixture tab through the real pointer; don't
    # create notes or invoke callbacks as a navigation shortcut.
    $tab=Node $window ('Open '+$FixtureNote) 5
    $null=Click-Target $window $app $tab 'center' 100 100
    $homeControl=Node $window $homeLabel 5;$before=Bounds $window
    $case.home_pointer=Click-Target $window $app $homeControl $location $hover $press
    $null=Node $window 'Documents' 3
    if($case.home_pointer.native_system_menu){throw 'Home click opened native system menu.'}
    if($case.home_pointer.hit_test_after_press -ne 1){throw ('Visible Home button point was not HTCLIENT: '+$case.home_pointer.hit_test_after_press)}
    Check-Bounds $before (Bounds $window)
    $case.home_passed=$true
    $case.home_right_pointer=Click-Target $window $app (Node $window $homeLabel) $location $hover $press $true
    if($case.home_right_pointer.native_system_menu){throw 'Home right press opened native system menu.'}
    Check-Bounds $before (Bounds $window)
    # Return through the real note tab and verify the editor before ending Home.
    $null=Click-Target $window $app (Node $window ('Open '+$FixtureNote)) 'center' 100 100
   }catch{
    $case.home_error=$_.Exception.Message
    try{Capture $window ($caseName+'-home-failure.png');Names $window|ConvertTo-Json -Depth 3|Set-Content (Join-Path $Output ($caseName+'-home-accessible-names.json')) -Encoding UTF8}catch{$case.home_diagnostic_error=$_.Exception.Message}
    try{Focus-Owned $window $app;[FolioTitlebarPointerV2]::Escape()}catch{}
   }
   # Update is an independent gate even when Home failed. Reset is fixture-only;
   # the failed Home result remains retained and can never become a pass.
   try{
    Focus-Owned $window $app;[FolioTitlebarPointerV2]::ResetUpdate()
    $null=Node $window $available 5;$before=Bounds $window
    $case.update_pointer=Click-Target $window $app (Node $window $available) $location $hover $press
    $progress=Node $window 'Downloading 42%' 3
    if($case.update_pointer.native_system_menu){throw 'Update click opened native system menu.'}
    if($case.update_pointer.hit_test_after_press -ne 1){throw ('Visible Update button point was not HTCLIENT: '+$case.update_pointer.hit_test_after_press)}
    Check-Bounds $before (Bounds $window)
    $case.update_passed=$true
    $case.disabled_progress_left=Click-Target $window $app $progress $location $hover $press
    $case.disabled_progress_right=Click-Target $window $app (Node $window 'Downloading 42%') $location $hover $press $true
    if($case.disabled_progress_left.native_system_menu -or $case.disabled_progress_right.native_system_menu){throw 'Disabled progress press opened native system menu.'}
    $null=Node $window 'Downloading 42%' 3
    Check-Bounds $before (Bounds $window)
    $case.disabled_progress_passed=$true
    $case.bounds_after=Bounds $window
    if($location -eq 'center' -and $hover -eq 0 -and $press -eq 20){Capture $window ($caseName+'-downloading.png')}
   }catch{
    $case.update_error=$_.Exception.Message
    try{Capture $window ($caseName+'-update-failure.png');Names $window|ConvertTo-Json -Depth 3|Set-Content (Join-Path $Output ($caseName+'-update-accessible-names.json')) -Encoding UTF8}catch{$case.update_diagnostic_error=$_.Exception.Message}
    # Release a native menu only in the focused owned window. No retry can turn
    # this failed case into a pass; every failure remains in results.
    try{Focus-Owned $window $app;[FolioTitlebarPointerV2]::Escape()}catch{}
   }
   if($case.home_error -or $case.update_error){$failed++}
   $results+=$case
   $results|ConvertTo-Json -Depth 10|Set-Content (Join-Path $Output 'pointer-results.json') -Encoding UTF8
   $caseName+': home='+$case.home_passed+', update='+$case.update_passed+', disabled='+$case.disabled_progress_passed|Add-Content (Join-Path $Output 'progress.txt')
  }}}
 }
 $app.CloseMainWindow()|Out-Null
 if(!$app.WaitForExit(20000)){throw 'Owned fixture did not close and flush.'}
 $app=$null
 if(!(Test-Path -LiteralPath (Join-Path $data 'notes.sqlite3'))){throw 'Fixture library did not save.'}
 Copy-Item -LiteralPath (Join-Path $data 'notes.sqlite3') -Destination (Join-Path $Output 'fixture-notes.sqlite3')
 if((Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash.ToLowerInvariant() -ne $binaryHash){throw 'Running package executable changed during pointer testing.'}
 @{blank_titlebar_passed=$blankPassed;custom_window_controls_passed=$windowControlsPassed;cases=$results.Count;home_activations=@($results|Where-Object{$_.home_passed}).Count;update_activations=@($results|Where-Object{$_.update_passed}).Count;disabled_progress_cases=@($results|Where-Object{$_.disabled_progress_passed}).Count;failed_cases=$failed;binary_sha256=$binaryHash;native_installer_or_restart_invoked=$false}|ConvertTo-Json|Set-Content (Join-Path $Output 'summary.json') -Encoding UTF8
 if($failed){throw ($failed.ToString()+' physical pointer cases failed; retained all diagnostics.')}
 'WINDOWS_TITLEBAR_POINTER_MATRIX_OK'|Set-Content (Join-Path $Output 'done')
}catch{$_|Out-String|Set-Content (Join-Path $Output 'error.txt');throw}
finally{
 if($app -and !$app.HasExited){$app.CloseMainWindow()|Out-Null;if(!$app.WaitForExit(10000)){$app.Kill();$app.WaitForExit()}}
 @{only_owned_process_targeted=$true;updater_cache_never_accessed=$true;installed_application_never_modified=$true;disposable_library_only=$true}|ConvertTo-Json|Set-Content (Join-Path $Output 'cleanup.json') -Encoding UTF8
 $null=[FolioTitlebarPointerV2]::SetThreadExecutionState(2147483648)
 if($runRoot.StartsWith([IO.Path]::GetTempPath())){Remove-Item -LiteralPath $runRoot -Recurse -Force}
}
