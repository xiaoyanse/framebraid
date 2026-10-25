using System;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;

internal static class Native {
    [StructLayout(LayoutKind.Sequential)]
    internal struct Init {
        internal uint Size, Version;
        internal IntPtr RuntimeDirectories;
        internal uint DirectoryCount;
        [MarshalAs(UnmanagedType.ByValArray, SizeConst = 4)] internal uint[] Reserved;
        internal ulong FeatureFlags;
    }
    [DllImport("am_camera.dll", CallingConvention = CallingConvention.Cdecl)] internal static extern uint amc_get_abi_version();
    [DllImport("am_camera.dll", CallingConvention = CallingConvention.Cdecl)] internal static extern int amc_init(ref Init parameters, out IntPtr handle);
    [DllImport("am_camera.dll", CallingConvention = CallingConvention.Cdecl)] internal static extern int amc_shutdown(IntPtr handle, uint timeout, out uint state);
    [DllImport("am_camera.dll", CallingConvention = CallingConvention.Cdecl)] internal static extern int amc_object_destroy(IntPtr handle);
    [DllImport("am_camera.dll", CallingConvention = CallingConvention.Cdecl)] internal static extern int amc_object_query_state(CameraHandle handle, out uint state);
}

internal sealed class CameraHandle : SafeHandleZeroOrMinusOneIsInvalid {
    internal CameraHandle(IntPtr value) : base(true) { SetHandle(value); }
    protected override bool ReleaseHandle() {
        uint state;
        return Native.amc_shutdown(handle, 5000, out state) == 0 && state == 3 && Native.amc_object_destroy(handle) == 0;
    }
}

internal static class Lifecycle {
    private static void Main() {
        var parameters = new Native.Init { Size = (uint)Marshal.SizeOf(typeof(Native.Init)), Version = Native.amc_get_abi_version(), Reserved = new uint[4] };
        IntPtr raw;
        if (Native.amc_init(ref parameters, out raw) != 0) throw new Exception("SDK init failed");
        using (var context = new CameraHandle(raw)) {
            uint state;
            if (Native.amc_object_query_state(context, out state) != 0 || state != 0) throw new Exception("SDK state invalid");
        }
        Console.WriteLine("C# lifecycle passed: SafeHandle pins native call and closes before destroy");
    }
}
