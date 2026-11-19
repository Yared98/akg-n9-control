param([string]$Log = "$PSScriptRoot\probe.log", [string]$Req = "06 07 80 05 5A 03 00 D6 0C 00", [string]$Sel = 'HID\VID_0ECB&PID_20D9&MI_03&COL02', [switch]$Quiet)
# Sends ONE read-only query to the AKG dongle's FF13 (62-byte) collection and records the answers.
# Query = Airoha RACE "get battery" (id 0x0CD6), the same packet the Audeze Maxwell driver uses:
#   output report 06: [06][07][80][05 5A 03 00 D6 0C 00]
Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading;
using Microsoft.Win32.SafeHandles;
public static class Probe {
    [DllImport("kernel32.dll", SetLastError=true, CharSet=CharSet.Unicode)]
    static extern SafeFileHandle CreateFile(string n, uint a, uint s, IntPtr sa, uint c, uint f, IntPtr t);
    [DllImport("hid.dll", SetLastError=true)] static extern bool HidD_SetOutputReport(SafeFileHandle h, byte[] b, int l);
    [DllImport("hid.dll", SetLastError=true)] static extern bool HidD_GetInputReport(SafeFileHandle h, byte[] b, int l);
    [DllImport("hid.dll")] static extern bool HidD_GetPreparsedData(SafeFileHandle h, out IntPtr p);
    [DllImport("hid.dll")] static extern bool HidD_FreePreparsedData(IntPtr p);
    [StructLayout(LayoutKind.Sequential)] public struct CAPS {
        public ushort Usage, UsagePage, InLen, OutLen, FeatLen;
        [MarshalAs(UnmanagedType.ByValArray, SizeConst=17)] public ushort[] r;
        public ushort a,b,c,d,e,f,g,h,i,j; }
    [DllImport("hid.dll")] static extern int HidP_GetCaps(IntPtr p, out CAPS c);
    [StructLayout(LayoutKind.Explicit, Size=72)] public struct VCAPS {
        [FieldOffset(0)] public ushort UsagePage; [FieldOffset(2)] public byte ReportID; }
    [DllImport("hid.dll")] static extern int HidP_GetValueCaps(int type, [Out] VCAPS[] v, ref ushort n, IntPtr p);
    [StructLayout(LayoutKind.Explicit, Size=72)] public struct BCAPS {
        [FieldOffset(0)] public ushort UsagePage; [FieldOffset(2)] public byte ReportID; }
    [DllImport("hid.dll")] static extern int HidP_GetButtonCaps(int type, [Out] BCAPS[] v, ref ushort n, IntPtr p);
    static readonly object L = new object();
    static StreamWriter W;
    public static bool Quiet = false;
    static void Log(string s) { lock (L) { string line = DateTime.Now.ToString("HH:mm:ss.fff") + "  " + s; if (!Quiet) Console.WriteLine(line); W.WriteLine(line); W.Flush(); } }
    static string Hex(byte[] b, int n) { return BitConverter.ToString(b, 0, n).Replace("-", " "); }
    static string Ids(IntPtr pd, int type) {
        string s = ""; ushort n = 32; var v = new VCAPS[32];
        if (HidP_GetValueCaps(type, v, ref n, pd) == 0x00110000) for (int i = 0; i < n; i++) s += string.Format("v{0:X2} ", v[i].ReportID);
        n = 32; var b = new BCAPS[32];
        if (HidP_GetButtonCaps(type, b, ref n, pd) == 0x00110000) for (int i = 0; i < n; i++) s += string.Format("b{0:X2} ", b[i].ReportID);
        return s;
    }
    public static void Run(string path, string log, byte[] q) {
        W = new StreamWriter(log, false);
        var h = CreateFile(path, 0xC0000000, 3, IntPtr.Zero, 3, 0, IntPtr.Zero);
        if (h.IsInvalid) { Log("open failed err " + Marshal.GetLastWin32Error()); W.Close(); return; }
        IntPtr pd; CAPS c = new CAPS(); HidD_GetPreparsedData(h, out pd); HidP_GetCaps(pd, out c);
        Log(string.Format("caps {0:X4}:{1:X4} in={2} out={3} feat={4}", c.UsagePage, c.Usage, c.InLen, c.OutLen, c.FeatLen));
        Log("report ids  input: " + Ids(pd, 0) + " output: " + Ids(pd, 1) + " feature: " + Ids(pd, 2));
        HidD_FreePreparsedData(pd);
        int inLen = c.InLen;
        var rh = CreateFile(path, 0x80000000, 3, IntPtr.Zero, 3, 0, IntPtr.Zero);
        var t = new Thread(() => {
            var fs = new FileStream(rh, FileAccess.Read, inLen, false); var buf = new byte[inLen];
            try { while (true) { int n = fs.Read(buf, 0, inLen); if (n <= 0) break; Log("interrupt in: " + Hex(buf, n)); } } catch {}
        });
        t.IsBackground = true; t.Start();
        var req = new byte[c.OutLen];
        // request bytes come from -Req
        Array.Copy(q, req, q.Length);
        Log("send: " + Hex(req, 12) + " ...");
        bool ok = HidD_SetOutputReport(h, req, req.Length);
        Log("SetOutputReport ok=" + ok + (ok ? "" : " err " + Marshal.GetLastWin32Error()));
        if (!ok) {
            var fs = new FileStream(h, FileAccess.Write, req.Length, false);
            try { fs.Write(req, 0, req.Length); fs.Flush(); Log("WriteFile ok"); } catch (Exception e) { Log("WriteFile failed: " + e.Message); }
        }
        for (int i = 0; i < 120; i++) {
            Thread.Sleep(15);
            var r = new byte[inLen]; r[0] = 0x07;
            if (HidD_GetInputReport(h, r, r.Length)) Log("GetInputReport 07: " + Hex(r, r.Length));
            else Log("GetInputReport 07 failed err " + Marshal.GetLastWin32Error());
        }
        Thread.Sleep(1500);
        Log("done"); W.Flush();
    }
}
'@
$id = (Get-PnpDevice -PresentOnly | Where-Object InstanceId -like ($Sel + '*') | Select-Object -First 1).InstanceId
[Probe]::Quiet = $Quiet.IsPresent
[Probe]::Run('\\?\' + ($id -replace '\\', '#') + '#{4d1e55b2-f16f-11cf-88cb-001111000030}', $Log, [byte[]]($Req -split ' ' | ForEach-Object { [Convert]::ToByte($_, 16) }))
