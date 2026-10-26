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
        [DllImport("am_camera.dll", CallingConvention=CallingConvention.Cdecl)]
        internal static extern int amc_source_set_decode_policy(IntPtr arg0, uint arg1);
        [DllImport("am_vision.dll", CallingConvention=CallingConvention.Cdecl)]
        internal static extern uint amv_get_abi_version();
        [DllImport("am_vision.dll", CallingConvention=CallingConvention.Cdecl)]
        internal static extern int amv_probe_inference_hardware(IntPtr arg0, uint arg1);
        [DllImport("am_vision.dll", CallingConvention=CallingConvention.Cdecl)]
        internal static extern int amv_get_inference_hardware_report(IntPtr arg0, IntPtr arg1, ulong arg2, IntPtr arg3);
    }
}
