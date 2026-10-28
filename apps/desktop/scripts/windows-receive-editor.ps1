<#
.SYNOPSIS
Open a real, non-sensitive Notepad editor and update a visible counter by its
owned native Edit control, without SendKeys, clipboard or desktop focus loops.

.DESCRIPTION
Run in the authorized Windows desktop, then share only the window titled
"Petal Safe Editor - Notepad" using the released native Petal client's normal
share picker. This helper does not join rooms, inject video or control Petal.
Targets classic Edit or modern RichEdit native editor controls. Supply both
-ProcessId and -WindowHandle to attach to an explicitly authorized safe editor.
Otherwise launches its own Notepad; never discovers an unrelated editor.
The editor stays open when the bounded update interval ends; parent owns its
normal desktop cleanup. All text is the fixed public acceptance fixture.
#>
param(
  [ValidateRange(30, 3600)] [int]$DurationSeconds = 600,
  [ValidateRange(500, 5000)] [int]$IntervalMilliseconds = 1000,
  [int]$ProcessId = 0,
  [long]$WindowHandle = 0
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class PetalReceiveEditor {
  public delegate bool EnumProc(IntPtr window, IntPtr parameter);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc callback, IntPtr parameter);
  [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr parent, EnumProc callback, IntPtr parameter);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr window, StringBuilder name, int count);
  [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr window);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern bool SetWindowText(IntPtr window, string text);
  [DllImport("user32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
  public static extern IntPtr SendMessageTimeout(IntPtr window, uint message, IntPtr wparam, string lparam, uint flags, uint timeout, out IntPtr result);
  [DllImport("user32.dll", EntryPoint="SendMessageTimeoutW", CharSet=CharSet.Unicode, SetLastError=true)]
  public static extern IntPtr ReadMessageTimeout(IntPtr window, uint message, IntPtr wparam, StringBuilder lparam, uint flags, uint timeout, out IntPtr result);
  [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr window, int x, int y, int width, int height, bool repaint);
  public static IntPtr FindWindow(uint processId) {
    IntPtr result = IntPtr.Zero;
    EnumWindows((window, parameter) => {
      uint owner;
      GetWindowThreadProcessId(window, out owner);
      if (owner == processId && IsWindowVisible(window)) { result = window; return false; }
      return true;
    }, IntPtr.Zero);
    return result;
  }
  public static IntPtr FindEdit(IntPtr parent) {
    IntPtr result = IntPtr.Zero;
    EnumChildWindows(parent, (window, parameter) => {
      var name = new StringBuilder(256);
      GetClassName(window, name, name.Capacity);
      if (name.ToString() == "Edit" || name.ToString().StartsWith("RichEdit", StringComparison.OrdinalIgnoreCase)) {
        if (result != IntPtr.Zero) throw new Exception("Multiple native editor controls; select a single safe Notepad tab");
        result = window;
      }
      return true;
    }, IntPtr.Zero);
    return result;
  }
  public static void Update(IntPtr window, uint processId, string text) {
    uint owner;
    GetWindowThreadProcessId(window, out owner);
    if (!IsWindow(window) || owner != processId) throw new Exception("Owned Notepad Edit control disappeared");
    IntPtr ignored;
    // SMTO_ABORTIFHUNG bounds the real native UI update; no focus-dependent keys.
    if (SendMessageTimeout(window, 0x000C, IntPtr.Zero, text, 2, 2000, out ignored) == IntPtr.Zero)
      throw new Exception("Owned Notepad Edit control rejected WM_SETTEXT");
    var actual = new StringBuilder(text.Length + 1);
    if (ReadMessageTimeout(window, 0x000D, new IntPtr(actual.Capacity), actual, 2, 2000, out ignored) == IntPtr.Zero)
      throw new Exception("Notepad native editor readback timed out");
    if (actual.ToString() != text) throw new Exception("Notepad did not retain the safe editor fixture");
  }
}
'@

if (($ProcessId -eq 0) -ne ($WindowHandle -eq 0)) {
  throw 'Attach requires both -ProcessId and -WindowHandle for the authorized safe Notepad window'
}
$attaching = $ProcessId -ne 0
$editor = if ($attaching) { Get-Process -Id $ProcessId } else {
  Start-Process -FilePath "$env:WINDIR\System32\notepad.exe" -PassThru
}
if ($editor.ProcessName -ne 'Notepad') { throw 'Fixture requires an actual Notepad process' }
$window = [IntPtr]$WindowHandle
if ($attaching) {
  [uint32]$ownerId = 0
  [void][PetalReceiveEditor]::GetWindowThreadProcessId($window, [ref]$ownerId)
  if ($ownerId -ne $editor.Id -or -not [PetalReceiveEditor]::IsWindowVisible($window)) {
    throw 'Explicit safe Notepad window is not visible or not owned by supplied process'
  }
}
$edit = [IntPtr]::Zero
$deadline = [DateTime]::UtcNow.AddSeconds(15)
while ([DateTime]::UtcNow -lt $deadline) {
  $editor.Refresh()
  if ($editor.HasExited) {
    throw 'Launched Notepad exited or delegated; attach using the authorized safe editor ProcessId and WindowHandle'
  }
  if (-not $attaching) { $window = [PetalReceiveEditor]::FindWindow([uint32]$editor.Id) }
  if ($window -ne [IntPtr]::Zero) {
    $edit = [PetalReceiveEditor]::FindEdit($window)
    if ($edit -ne [IntPtr]::Zero) { break }
  }
  Start-Sleep -Milliseconds 200
}
if ($edit -eq [IntPtr]::Zero) { throw 'Owned Notepad has no native Edit/RichEdit control; no fixture was substituted' }
if (-not [PetalReceiveEditor]::MoveWindow($window, 80, 80, 960, 560, $true)) {
  throw 'Unable to size the actual Notepad editor window'
}
if (-not [PetalReceiveEditor]::SetWindowText($window, 'Petal Safe Editor - Notepad')) {
  throw 'Unable to name the actual Notepad editor window'
}
Write-Output 'Share only: Petal Safe Editor - Notepad'
Write-Output 'Source: actual Notepad Edit control; fixed non-sensitive text; no clipboard or SendKeys'
$until = [DateTime]::UtcNow.AddSeconds($DurationSeconds)
$counter = 0
while ([DateTime]::UtcNow -lt $until) {
  $counter++
  $text = "PETAL SAFE EDITOR`r`n`r`nPETAL RECEIVE COUNTER $counter`r`n`r`nThis is a non-sensitive Windows Notepad acceptance document.`r`nThe counter changes in the real editor, not in generated video.`r`n"
  [PetalReceiveEditor]::Update($edit, [uint32]$editor.Id, $text)
  Start-Sleep -Milliseconds $IntervalMilliseconds
}
Write-Output "Counter updates completed: $counter; safe editor remains open for normal parent cleanup"
