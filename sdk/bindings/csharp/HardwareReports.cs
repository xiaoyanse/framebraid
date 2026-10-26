using System;
using System.Runtime.InteropServices;
using System.Text;

namespace AmrtSdk {
    internal static class HardwareReports {
        internal static void RequireApi(bool vision = false) {
            uint version = vision ? Native.amv_get_abi_version() : Native.amc_get_abi_version();
            if ((version >> 16) != 1 || version < 0x00010001)
                throw new NotSupportedException("Hardware reports require FrameBraid C ABI 1.1 or later in major version 1.");
        }
        internal static string Read(IntPtr owner, bool vision = false) {
            RequireApi(vision);
            IntPtr buffer = Marshal.AllocHGlobal(65536);
            try {
                IntPtr count = Marshal.AllocHGlobal(8);
                try {
                    int code = vision ? Native.amv_get_inference_hardware_report(owner, buffer, 65536, count) : Native.amc_get_decode_hardware_report(owner, buffer, 65536, count);
                    Diagnostics.Check(code, "read hardware report", owner, vision);
                    long bytes = Marshal.ReadInt64(count);
                    if (bytes < 1 || bytes > 65536) throw new InvalidOperationException("Invalid hardware report length.");
                    byte[] utf8 = new byte[checked((int)bytes - 1)];
                    Marshal.Copy(buffer, utf8, 0, utf8.Length);
                    return Encoding.UTF8.GetString(utf8);
                } finally { Marshal.FreeHGlobal(count); }
            } finally { Marshal.FreeHGlobal(buffer); }
        }
    }
}
