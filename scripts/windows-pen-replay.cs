// Exercise the actual WM_POINTER adapter with Windows' synthetic pen API.
// This tests event routing; physical digitizer latency still needs hardware.
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Threading;
public static class FolioPenReplay {
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
    [StructLayout(LayoutKind.Sequential)] public struct PointerInfo {
        public uint Type, Id, Frame, Flags;
        public IntPtr Device, Window;
        public Point Pixel, Himetric, PixelRaw, HimetricRaw;
        public uint Time, History;
        public int InputData;
        public uint KeyStates;
        public ulong PerformanceCount;
        public uint ButtonChange;
    }
    [StructLayout(LayoutKind.Sequential)] public struct PenInfo {
        public PointerInfo Pointer;
        public uint Flags, Mask, Pressure, Rotation;
        public int TiltX, TiltY;
    }
    // The union also contains a 144-byte POINTER_TOUCH_INFO on x64.
    [StructLayout(LayoutKind.Explicit, Size=152)] public struct PointerTypeInfo {
        [FieldOffset(0)] public uint Type;
        [FieldOffset(8)] public PenInfo Pen;
    }
    [DllImport("user32.dll", SetLastError=true)] static extern IntPtr CreateSyntheticPointerDevice(uint type, uint count, uint feedback);
    [DllImport("user32.dll", SetLastError=true)] static extern bool InjectSyntheticPointerInput(IntPtr device, [In] PointerTypeInfo[] pointers, uint count);
    [DllImport("user32.dll")] static extern void DestroySyntheticPointerDevice(IntPtr device);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
    [DllImport("user32.dll")] static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] static extern void mouse_event(uint flags, uint x, uint y, uint data, UIntPtr extra);
    public static void DragMouse(int x, int y, int dx, int dy) {
        SetCursorPos(x, y);
        Thread.Sleep(100);
        mouse_event(2, 0, 0, 0, UIntPtr.Zero);
        Thread.Sleep(150);
        for (int i=1; i<=10; i++) {
            SetCursorPos(x+dx*i/10, y+dy*i/10);
            Thread.Sleep(20);
        }
        mouse_event(4, 0, 0, 0, UIntPtr.Zero);
        Thread.Sleep(300);
    }
    static void Frame(IntPtr device, int x, int y, uint flags, uint pressure) {
        PointerTypeInfo input = new PointerTypeInfo();
        input.Type = 3; // PT_PEN
        input.Pen.Pointer.Type = 3;
        input.Pen.Pointer.Id = 1;
        input.Pen.Pointer.Flags = flags;
        input.Pen.Pointer.Pixel = new Point { X=x, Y=y };
        input.Pen.Mask = 1 | 4 | 8; // pressure and both tilt axes
        input.Pen.Pressure = pressure;
        input.Pen.TiltX = 20;
        input.Pen.TiltY = -10;
        if (!InjectSyntheticPointerInput(device, new[] { input }, 1))
            throw new Win32Exception(Marshal.GetLastWin32Error(), "InjectSyntheticPointerInput");
        Thread.Sleep(12);
    }
    public static void Tap(int x, int y) {
        IntPtr device = CreateSyntheticPointerDevice(3, 1, 3);
        if (device == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
        try {
            Frame(device, x, y, 0x00020003, 0);
            Frame(device, x, y, 0x00010016, 512);
            Frame(device, x, y, 0x00040002, 0);
            Frame(device, x, y, 0x00020000, 0);
        } finally { DestroySyntheticPointerDevice(device); }
    }
    public static void Stroke(int x, int y) {
        if (IntPtr.Size != 8) throw new InvalidOperationException("Run the x64 PowerShell host.");
        IntPtr device = CreateSyntheticPointerDevice(3, 1, 3); // feedback off
        if (device == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error(), "CreateSyntheticPointerDevice");
        try {
            Frame(device, x, y, 0x00020003, 0); // NEW, INRANGE, UPDATE: hover
            Frame(device, x, y, 0x00010016, 100); // INRANGE, INCONTACT, FIRSTBUTTON, DOWN
            for (int i=1; i<=24; i++)
                Frame(device, x+i*5, y+i*2, 0x00020016, (uint)(100+i*30));
            Frame(device, x+120, y+48, 0x00040002, 0); // INRANGE, UP
            Frame(device, x+120, y+48, 0x00020000, 0); // leave range
        } finally { DestroySyntheticPointerDevice(device); }
    }
}
