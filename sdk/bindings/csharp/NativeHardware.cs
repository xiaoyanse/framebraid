// Generated from the public ctypes ABI; do not edit.
using System;
using System.Runtime.InteropServices;
namespace AmrtSdk {
    internal static partial class Native {
        internal const uint WorkerBudgetCapability = 32768U;
        [DllImport("am_camera.dll", CallingConvention=CallingConvention.Cdecl)]
        internal static extern int amc_query_capability(IntPtr arg0, IntPtr arg1);
        [DllImport("am_camera.dll", CallingConvention=CallingConvention.Cdecl)]
        internal static extern int amc_set_worker_budget(IntPtr arg0, uint arg1);
        [DllImport("am_camera.dll", CallingConvention=CallingConvention.Cdecl)]
        internal static extern int amc_get_worker_budget(IntPtr arg0, IntPtr arg1);
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
        internal static extern int amv_query_capability(IntPtr arg0, IntPtr arg1);
        [DllImport("am_vision.dll", CallingConvention=CallingConvention.Cdecl)]
        internal static extern int amv_probe_inference_hardware(IntPtr arg0, uint arg1);
        [DllImport("am_vision.dll", CallingConvention=CallingConvention.Cdecl)]
        internal static extern int amv_get_inference_hardware_report(IntPtr arg0, IntPtr arg1, ulong arg2, IntPtr arg3);
        [DllImport("am_vision.dll", CallingConvention=CallingConvention.Cdecl)]
        internal static extern int amv_model_load_selected(IntPtr arg0, IntPtr arg1, IntPtr arg2, IntPtr arg3, IntPtr arg4);
        [DllImport("am_vision.dll", CallingConvention=CallingConvention.Cdecl)]
        internal static extern int amv_model_load_memory_selected(IntPtr arg0, IntPtr arg1, IntPtr arg2, uint arg3, IntPtr arg4, IntPtr arg5);
        [DllImport("am_vision.dll", CallingConvention=CallingConvention.Cdecl)]
        internal static extern int amv_get_model_selection_report(IntPtr arg0, IntPtr arg1, ulong arg2, IntPtr arg3);
        [DllImport("am_vision.dll", CallingConvention=CallingConvention.Cdecl)]
        internal static extern int amv_get_selection_attempt_report(IntPtr arg0, ulong arg1, IntPtr arg2, ulong arg3, IntPtr arg4);
    }
}
