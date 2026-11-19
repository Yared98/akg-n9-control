param([int]$Start = 0xF000, [int]$End = 0xF3FF, [string]$Out = "$PSScriptRoot\nvkeys.csv", [string]$Sel = 'HID\VID_0ECB&PID_20D9&MI_03&COL02')
# Read-only sweep of Airoha NVKeys over the dongle. For each NVKey id it sends RACE_NVKEY_READFULLKEY
# (0x0A00) and records any non-empty value. Nothing is written to the device.
# Request RACE packet: 05 5A 06 00 00 0A <nvid LE> <maxlen LE>. HID: [06][len][80] + packet.
Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading;
using Microsoft.Win32.SafeHandles;
public static class NvSweep {
    [DllImport("kernel32.dll", SetLastError=true, CharSet=CharSet.Unicode)]
    static extern SafeFileHandle CreateFile(string n, uint a, uint s, IntPtr sa, uint c, uint f, IntPtr t);
    [DllImport("hid.dll", SetLastError=true)] static extern bool HidD_SetOutputReport(SafeFileHandle h, byte[] b, int l);
    [DllImport("hid.dll", SetLastError=true)] static extern bool HidD_GetInputReport(SafeFileHandle h, byte[] b, int l);
    static SafeFileHandle Open(string path, uint access) { return CreateFile(path, access, 3, IntPtr.Zero, 3, 0, IntPtr.Zero); }
    // Find a 0x0A00 reply in the report-07 buffer; returns the NV value hex, or null.
    static string FindNv(byte[] b, int wantId) {
        int valid = 3 + b[1];
        if (valid > b.Length) valid = b.Length;
        for (int i = 3; i + 8 < valid; i++) {
            if (b[i] != 0x05 || (b[i+1] != 0x5B && b[i+1] != 0x5D)) continue;
            int id = b[i+4] | (b[i+5] << 8);
            if (id != 0x0A00) continue;
            // after id: length short LE at (b+6,b+7), data from +8 (no separate status byte for NVKey read)
            int len = b[i+6] | (b[i+7] << 8);
            if (len <= 0 || len > 240) return null;
            int ds = i + 8;
            if (ds + len > valid) len = valid - ds;
            if (len <= 0) return null;
            var sb = new System.Text.StringBuilder();
            for (int k = 0; k < len; k++) sb.AppendFormat("{0:X2} ", b[ds + k]);
            return sb.ToString().Trim();
        }
        return null;
    }
    public static void Run(string path, int start, int end, string outCsv) {
        var h = Open(path, 0xC0000000u);
        if (h.IsInvalid) { Console.WriteLine("open failed " + Marshal.GetLastWin32Error()); return; }
        using (var w = new StreamWriter(outCsv, false)) {
            w.WriteLine("nvkey_hex,nvkey_dec,length,value_hex,value_ascii");
            int found = 0;
            for (int id = start; id <= end; id++) {
                var req = new byte[62];
                byte[] pkt = { 0x06, 0x0A, 0x80, 0x05, 0x5A, 0x06, 0x00, 0x00, 0x0A,
                               (byte)(id & 0xFF), (byte)((id >> 8) & 0xFF), 0xF0, 0x00 };
                Array.Copy(pkt, req, pkt.Length);
                HidD_SetOutputReport(h, req, req.Length);
                string val = null;
                for (int tries = 0; tries < 4 && val == null; tries++) {
                    Thread.Sleep(12);
                    var r = new byte[62]; r[0] = 0x07;
                    if (HidD_GetInputReport(h, r, r.Length)) val = FindNv(r, id);
                }
                if (val != null) {
                    var bytes = val.Split(' ');
                    var asc = new System.Text.StringBuilder();
                    foreach (var hx in bytes) { int v = Convert.ToInt32(hx,16); asc.Append(v>=32&&v<127?(char)v:'.'); }
                    w.WriteLine(string.Format("0x{0:X4},{1},{2},{3},{4}", id, id, bytes.Length, val.Replace(" ",""), asc));
                    w.Flush();
                    found++;
                    Console.WriteLine(string.Format("0x{0:X4}  len={1}  {2}  |{3}|", id, bytes.Length, val, asc));
                }
            }
            Console.WriteLine("done; populated keys: " + found);
        }
    }
}
'@
$id = (Get-PnpDevice -PresentOnly | Where-Object InstanceId -like ($Sel + '*') | Select-Object -First 1).InstanceId
$p = '\\?\' + ($id -replace '\\', '#') + '#{4d1e55b2-f16f-11cf-88cb-001111000030}'
[NvSweep]::Run($p, $Start, $End, $Out)
