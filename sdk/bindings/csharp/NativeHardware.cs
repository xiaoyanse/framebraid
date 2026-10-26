// Generated from the public ctypes ABI; do not edit.
using System;
using System.Runtime.InteropServices;
namespace AmrtSdk {
    internal static partial class Native {
        [DllImport("am_camera.dll", CallingConvention=CallingConvention.Cdecl)]
        internal static extern uint amc_get_abi_version();
        [DllImport("am_camera.dll", CallingConvention=CallingConvention.Cdecl)]
        internal static extern int amc_probe_decode_hardware(IntPtr arg0, uint arg1, uint arg2);
        [DllImport("am_camera.dll", CallingConvention=CallingConvention.Cdecl)]
        internal static extern int amc_get_decode_hardware_report(IntPtr arg0, IntPtr arg1, ulong arg2, IntPtr arg3);
    }
}
