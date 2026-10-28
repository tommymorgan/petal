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
Optional -GeometryPhaseFile enables the same editor's fidelity fixture. An absent
file means initial; otherwise its entire trimmed value must be initial or changed.
Only this mode requests 960x560 / 800x650 and places four corner labels using the
native formatting rectangle and measured editor font. Each phase emits public
FIDELITY_SOURCE_METADATA JSON: width/height are measured DWM visible frame pixels,
not the outer window request or an assumption about invisible borders.
#>
param(
  [ValidateRange(30, 3600)] [int]$DurationSeconds = 600,
  [ValidateRange(500, 5000)] [int]$IntervalMilliseconds = 1000,
  [int]$ProcessId = 0,
  [long]$WindowHandle = 0,
  [string]$GeometryPhaseFile = ''
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
  [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
  [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
  [StructLayout(LayoutKind.Sequential)] public struct Size { public int Width, Height; }
  [StructLayout(LayoutKind.Sequential, CharSet=System.Runtime.InteropServices.CharSet.Unicode)]
  public struct TextMetric {
    public int Height, Ascent, Descent, InternalLeading, ExternalLeading, AveCharWidth, MaxCharWidth, Weight, Overhang, DigitizedAspectX, DigitizedAspectY;
    public char FirstChar, LastChar, DefaultChar, BreakChar;
    public byte Italic, Underlined, StruckOut, PitchAndFamily, CharSet;
  }
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr window, out Rect rect);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr window, ref Point point);
  [DllImport("user32.dll")] public static extern IntPtr GetDC(IntPtr window);
  [DllImport("user32.dll")] public static extern int ReleaseDC(IntPtr window, IntPtr dc);
  [DllImport("user32.dll", EntryPoint="SendMessageTimeoutW", SetLastError=true)]
  public static extern IntPtr NativeMessage(IntPtr window, uint message, IntPtr wparam, IntPtr lparam, uint flags, uint timeout, out IntPtr result);
  [DllImport("user32.dll", EntryPoint="SendMessageTimeoutW", SetLastError=true)]
  public static extern IntPtr RectMessage(IntPtr window, uint message, IntPtr wparam, ref Rect rect, uint flags, uint timeout, out IntPtr result);
  [DllImport("gdi32.dll")] public static extern IntPtr SelectObject(IntPtr dc, IntPtr obj);
  [DllImport("gdi32.dll", CharSet=CharSet.Unicode)] public static extern bool GetTextMetrics(IntPtr dc, out TextMetric metric);
  [DllImport("gdi32.dll", CharSet=CharSet.Unicode)] public static extern bool GetTextExtentPoint32(IntPtr dc, string text, int length, out Size size);
  [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr window, int attribute, out Rect rect, int size);
  [DllImport("kernel32.dll", SetLastError=true)] static extern IntPtr OpenProcess(uint access, bool inherit, uint processId);
  [DllImport("kernel32.dll", SetLastError=true)] static extern IntPtr VirtualAllocEx(IntPtr process, IntPtr address, UIntPtr size, uint allocation, uint protection);
  [DllImport("kernel32.dll", SetLastError=true)] static extern bool VirtualFreeEx(IntPtr process, IntPtr address, UIntPtr size, uint type);
  [DllImport("kernel32.dll", SetLastError=true)] static extern bool ReadProcessMemory(IntPtr process, IntPtr address, byte[] bytes, UIntPtr size, out UIntPtr read);
  [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
  public static Point RichPosition(IntPtr edit, uint expectedOwner, int index) {
    uint owner;
    GetWindowThreadProcessId(edit, out owner);
    if (!IsWindow(edit) || owner != expectedOwner)
      throw new Exception("Owned Notepad editor disappeared before position measurement");
    // EM_POSFROMCHAR (RichEdit 2+) writes a POINTL through wParam. Messages
    // above WM_USER are not automatically marshalled between processes.
    // Allocate only an eight-byte output buffer in the explicitly owned editor.
    IntPtr process = OpenProcess(0x0038, false, owner); // VM_OPERATION/READ/WRITE only
    if (process == IntPtr.Zero) throw new Exception("Cannot open owned editor for native position measurement");
    IntPtr remote = IntPtr.Zero;
    bool completed = false;
    try {
      remote = VirtualAllocEx(process, IntPtr.Zero, new UIntPtr(8), 0x3000, 0x04);
      if (remote == IntPtr.Zero) throw new Exception("Cannot allocate owned editor position buffer");
      IntPtr result;
      if (NativeMessage(edit, 0x00D6, remote, new IntPtr(index), 2, 2000, out result) == IntPtr.Zero)
        throw new Exception("Owned RichEdit position measurement timed out");
      completed = true;
      var bytes = new byte[8];
      UIntPtr count;
      if (!ReadProcessMemory(process, remote, bytes, new UIntPtr(8), out count) || count.ToUInt64() != 8)
        throw new Exception("Cannot read owned RichEdit position output");
      return new Point { X = BitConverter.ToInt32(bytes, 0), Y = BitConverter.ToInt32(bytes, 4) };
    } finally {
      // A timed-out receiver may still use its pointer: retain just those eight
      // bytes until Notepad exits, rather than freeing beneath its UI thread.
      if (completed && remote != IntPtr.Zero) VirtualFreeEx(process, remote, UIntPtr.Zero, 0x8000);
      CloseHandle(process);
    }
  }
  public static IntPtr Message(IntPtr window, uint message, IntPtr wparam, IntPtr lparam) {
    IntPtr result;
    if (NativeMessage(window, message, wparam, lparam, 2, 2000, out result) == IntPtr.Zero)
      throw new Exception("Notepad geometry message timed out");
    return result;
  }
  public sealed class Marker {
    public int number, x, y, width, height;
  }
  public sealed class Geometry {
    public int width, height, editorX, editorY, editorWidth, editorHeight;
    public int formatX, formatY, formatWidth, formatHeight, charWidth, lineHeight, columns, rows;
    public Marker[] markers;
    public string markerOrigin;
  }
  public static Geometry Measure(IntPtr window, IntPtr edit, uint processId, string phase, int counter) {
    Rect frame, client;
    if (DwmGetWindowAttribute(window, 9, out frame, Marshal.SizeOf(typeof(Rect))) != 0)
      throw new Exception("Cannot measure actual DWM visible source frame");
    if (!GetClientRect(edit, out client)) throw new Exception("Cannot measure native editor");
    Rect format = new Rect();
    IntPtr ignored;
    // EM_GETRECT is a system-marshalled message, including across processes.
    if (RectMessage(edit, 0x00B2, IntPtr.Zero, ref format, 2, 2000, out ignored) == IntPtr.Zero)
      throw new Exception("Cannot measure native editor formatting rectangle");
    Point origin = new Point();
    if (!ClientToScreen(edit, ref origin)) throw new Exception("Cannot locate native editor");
    IntPtr font = Message(edit, 0x0031, IntPtr.Zero, IntPtr.Zero); // WM_GETFONT
    int measuredCharWidth = 0, measuredLineHeight = 0;
    if (font == IntPtr.Zero) {
      var name = new StringBuilder(256);
      GetClassName(edit, name, name.Capacity);
      if (!name.ToString().StartsWith("RichEdit", StringComparison.OrdinalIgnoreCase))
        throw new Exception("Native editor did not expose its actual font");
      // Measure the actual rendered RichEdit layout, including its own font,
      // DPI and zoom, rather than selecting an unrelated default DC font.
      string known = phase == "changed" ? "Petal Linux 67890" : "Petal Linux 12345";
      string probe = known + "\r\nPETAL SAFE EDITOR\r\nPETAL RECEIVE COUNTER " + counter;
      Update(edit, processId, probe);
      Message(edit, 0x00B1, IntPtr.Zero, IntPtr.Zero);
      Message(edit, 0x00B7, IntPtr.Zero, IntPtr.Zero);
      int firstIndex = LineStart(edit, 0);
      Point first = RichPosition(edit, processId, firstIndex);
      Point nextLine = RichPosition(edit, processId, LineStart(edit, 1));
      measuredLineHeight = nextLine.Y - first.Y;
      for (int i = 1; i < known.Length; i++) {
        Point previousChar = RichPosition(edit, processId, firstIndex + i - 1);
        Point currentChar = RichPosition(edit, processId, firstIndex + i);
        int advance = currentChar.X - previousChar.X;
        if (i == 1) measuredCharWidth = advance;
        if (advance != measuredCharWidth || currentChar.Y != first.Y)
          throw new Exception("Actual RichEdit fixture font must have uniform unwrapped character advances");
      }
    }
    if (font != IntPtr.Zero) {
      IntPtr dc = GetDC(edit);
      if (dc == IntPtr.Zero) throw new Exception("Cannot measure actual editor font");
      IntPtr previous = SelectObject(dc, font);
      TextMetric metric;
      Size narrow, wide, space;
      try {
        if (!GetTextMetrics(dc, out metric) ||
            !GetTextExtentPoint32(dc, "iiiiiiii", 8, out narrow) ||
            !GetTextExtentPoint32(dc, "WWWWWWWW", 8, out wide) ||
            !GetTextExtentPoint32(dc, "        ", 8, out space))
          throw new Exception("Cannot read native font metrics");
        if (narrow.Width != wide.Width || space.Width != wide.Width || space.Width % 8 != 0)
          throw new Exception("Fidelity fixture requires the owned editor's actual font to be monospace");
        measuredCharWidth = space.Width / 8;
        measuredLineHeight = metric.Height;
      } finally {
        SelectObject(dc, previous);
        ReleaseDC(edit, dc);
      }
    }
    var g = new Geometry();
    g.width = frame.Right - frame.Left; g.height = frame.Bottom - frame.Top;
    g.editorX = origin.X - frame.Left; g.editorY = origin.Y - frame.Top;
    g.editorWidth = client.Right; g.editorHeight = client.Bottom;
    g.formatX = format.Left; g.formatY = format.Top;
    g.formatWidth = format.Right - format.Left; g.formatHeight = format.Bottom - format.Top;
    g.charWidth = measuredCharWidth; g.lineHeight = measuredLineHeight;
    if (g.charWidth <= 0 || g.lineHeight <= 0) throw new Exception("Invalid actual editor font metrics");
    // Leave one character and one line of safety against rounding, wrap and scroll.
    g.columns = g.formatWidth / g.charWidth - 1;
    g.rows = g.formatHeight / g.lineHeight - 1;
    if (g.width <= 0 || g.height <= 0 || g.columns < 40 || g.rows < 8)
      throw new Exception("Actual Notepad editor is too small for the four-corner fixture");
    return g;
  }
  public static string Fixture(Geometry g, string phase, int counter) {
    var lines = new string[g.rows];
    for (int i = 0; i < lines.Length; i++) lines[i] = "";
    lines[0] = "CORNER 1" + new string(' ', g.columns - 16) + "CORNER 2";
    lines[2] = "PETAL SAFE EDITOR";
    lines[3] = phase == "changed" ? "Petal Linux 67890" : "Petal Linux 12345";
    lines[5] = "PETAL RECEIVE COUNTER " + counter;
    lines[g.rows - 1] = "CORNER 3" + new string(' ', g.columns - 16) + "CORNER 4";
    return String.Join("\r\n", lines);
  }
  public static int LineStart(IntPtr edit, int line) {
    // Ask the actual control: do not infer its CR/LF indexing convention
    // from the string passed to WM_SETTEXT or returned by WM_GETTEXT.
    long index = Message(edit, 0x00BB, new IntPtr(line), IntPtr.Zero).ToInt64();
    if (index < 0 || index > Int32.MaxValue)
      throw new Exception("Native editor did not expose the requested fixture line");
    return (int)index;
  }
  public static Point CharacterPosition(IntPtr edit, uint processId, int index) {
    var name = new StringBuilder(256);
    GetClassName(edit, name, name.Capacity);
    if (name.ToString() != "Edit") return RichPosition(edit, processId, index);
    long packed = Message(edit, 0x00D6, new IntPtr(index), IntPtr.Zero).ToInt64();
    if (packed == -1) throw new Exception("Native editor cannot locate a corner marker");
    return new Point { X = (short)(packed & 0xffff), Y = (short)((packed >> 16) & 0xffff) };
  }
  public static void FitFixture(IntPtr edit, uint processId, Geometry g, string phase, int counter) {
    // Empty and nonempty RichEdit paragraphs may have different spacing.
    // Fit the actual final document, not a uniform-pitch extrapolation.
    DateTime deadline = DateTime.UtcNow.AddSeconds(10);
    int previousRows = -1;
    for (int attempt = 0; attempt < 128 && DateTime.UtcNow < deadline; attempt++) {
      Update(edit, processId, Fixture(g, phase, counter));
      Message(edit, 0x00B1, IntPtr.Zero, IntPtr.Zero);
      Message(edit, 0x00B7, IntPtr.Zero, IntPtr.Zero);
      Point bottom = CharacterPosition(edit, processId, LineStart(edit, g.rows - 1));
      int remaining = g.formatY + g.formatHeight - (bottom.Y + g.lineHeight);
      // Fit the complete marker cell within one measured line of the bottom.
      // The independent four-corner clipping/edge assertions remain unchanged.
      if (remaining >= 0 && remaining <= g.lineHeight) {
        LocateMarkers(edit, processId, g);
        return;
      }
      int nextRows = g.rows + (remaining < 0 ? -1 : 1);
      if (nextRows < 8 || nextRows > 512 || nextRows == previousRows)
        throw new Exception("Actual native editor cannot fit the complete four-corner fixture");
      previousRows = g.rows;
      g.rows = nextRows;
    }
    throw new Exception("Bounded actual native editor layout fitting did not converge");
  }
  public static void LocateMarkers(IntPtr edit, uint processId, Geometry g) {
    g.markerOrigin = "native-character-position";
    g.markers = new Marker[4];
    int top = LineStart(edit, 0);
    int bottom = LineStart(edit, g.rows - 1);
    int[] indices = { top, top + g.columns - 8, bottom, bottom + g.columns - 8 };
    for (int i = 0; i < 4; i++) {
      Point point = CharacterPosition(edit, processId, indices[i]);
      int x = point.X, y = point.Y;
      if (x < g.formatX || y < g.formatY ||
          x + 8 * g.charWidth > g.formatX + g.formatWidth ||
          y + g.lineHeight > g.formatY + g.formatHeight)
        throw new Exception("Native editor layout clips a corner marker");
      if (Math.Abs(x - (i % 2 == 0 ? g.formatX : g.formatX + g.formatWidth - 8 * g.charWidth)) > 3 * g.charWidth ||
          Math.Abs(y - (i < 2 ? g.formatY : g.formatY + g.formatHeight - g.lineHeight)) > 3 * g.lineHeight)
        throw new Exception(String.Format("Native editor corner {0} not near actual edges: cell={1},{2}; format={3},{4},{5},{6}; advance={7},{8}; grid={9},{10}", i + 1, x, y, g.formatX, g.formatY, g.formatWidth, g.formatHeight, g.charWidth, g.lineHeight, g.columns, g.rows));
      g.markers[i] = new Marker { number = i + 1, x = g.editorX + x, y = g.editorY + y, width = 8 * g.charWidth, height = g.lineHeight };
    }
  }
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
$fidelity = -not [string]::IsNullOrWhiteSpace($GeometryPhaseFile)
$lastPhase = ''
$geometry = $null
if ($fidelity) {
  # Only this helper thread changes DPI context; do not modify desktop settings.
  if ([PetalReceiveEditor]::SetThreadDpiAwarenessContext([IntPtr](-4)) -eq [IntPtr]::Zero) {
    throw 'Cannot measure fidelity geometry in physical pixels'
  }
}
while ([DateTime]::UtcNow -lt $until) {
  $counter++
  if ($fidelity) {
    $phase = 'initial'
    if (Test-Path -LiteralPath $GeometryPhaseFile -PathType Leaf) {
      $phase = [IO.File]::ReadAllText($GeometryPhaseFile).Trim()
    }
    if ($phase -cne 'initial' -and $phase -cne 'changed') {
      throw 'GeometryPhaseFile must contain exactly initial or changed'
    }
    if ($phase -ne $lastPhase) {
      [uint32]$ownerId = 0
      [void][PetalReceiveEditor]::GetWindowThreadProcessId($window, [ref]$ownerId)
      if ($ownerId -ne $editor.Id -or -not [PetalReceiveEditor]::IsWindowVisible($window)) {
        throw 'Owned safe Notepad source window disappeared'
      }
      $requestedWidth = if ($phase -eq 'changed') { 800 } else { 960 }
      $requestedHeight = if ($phase -eq 'changed') { 650 } else { 560 }
      if (-not [PetalReceiveEditor]::MoveWindow($window, 80, 80, $requestedWidth, $requestedHeight, $true)) {
        throw 'Unable to resize the actual Notepad source window'
      }
      # Let the real editor process its resize before querying its own layout.
      Start-Sleep -Milliseconds 200
      $geometry = [PetalReceiveEditor]::Measure($window, $edit, [uint32]$editor.Id, $phase, $counter)
      [PetalReceiveEditor]::FitFixture($edit, [uint32]$editor.Id, $geometry, $phase, $counter)
    }
    $text = [PetalReceiveEditor]::Fixture($geometry, $phase, $counter)
    [PetalReceiveEditor]::Update($edit, [uint32]$editor.Id, $text)
    # Keep the first line visible after WM_SETTEXT, without focusing or typing.
    [void][PetalReceiveEditor]::Message($edit, 0x00B1, [IntPtr]::Zero, [IntPtr]::Zero)
    [void][PetalReceiveEditor]::Message($edit, 0x00B7, [IntPtr]::Zero, [IntPtr]::Zero)
    if ($phase -ne $lastPhase) {
      [PetalReceiveEditor]::LocateMarkers($edit, [uint32]$editor.Id, $geometry)
      $metadata = [ordered]@{ phase = $phase }
      foreach ($field in $geometry.GetType().GetFields()) {
        $metadata[$field.Name] = $field.GetValue($geometry)
      }
      Write-Output ('FIDELITY_SOURCE_METADATA ' + (ConvertTo-Json -InputObject $metadata -Depth 5 -Compress))
      $lastPhase = $phase
    }
    Start-Sleep -Milliseconds 1000
  } else {
    $text = "PETAL SAFE EDITOR`r`n`r`nPETAL RECEIVE COUNTER $counter`r`n`r`nThis is a non-sensitive Windows Notepad acceptance document.`r`nThe counter changes in the real editor, not in generated video.`r`n"
    [PetalReceiveEditor]::Update($edit, [uint32]$editor.Id, $text)
    Start-Sleep -Milliseconds $IntervalMilliseconds
  }
}
Write-Output "Counter updates completed: $counter; safe editor remains open for normal parent cleanup"
