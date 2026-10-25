using System;
using System.Runtime.InteropServices;
using System.Text;

namespace AmrtSdk {
    public sealed class SdkLoadException : Exception {
        public string LibraryName { get; private set; }
        internal SdkLoadException(string library, Exception original)
            : base("Cannot load SDK " + library + " (process " + (IntPtr.Size * 8) + "-bit). " +
                   Hint(original) + " Original error: " + original.Message, original) { LibraryName = library; }
        private static string Hint(Exception error) {
            if (error is BadImageFormatException)
                return "Invalid binary format or architecture mismatch; use matching x64 process, SDK and dependencies.";
            if (error is EntryPointNotFoundException)
                return "Required SDK export is missing; deploy matching SDK, bindings and dependency versions.";
            return "Deploy the SDK DLL in the application directory or native search path. If it exists, check transitive " +
                   "dependencies and matching VC++ x64 runtime. Loader code 126 does not identify the missing library.";
        }
    }

    internal static class Diagnostics {
        [StructLayout(LayoutKind.Sequential)]
        private struct InitError {
            public uint struct_size;
            public int code;
            [MarshalAs(UnmanagedType.ByValArray, SizeConst=256)] public byte[] message;
            public IntPtr source_utf8;
            public long failed_at_ms;
            public ulong attempt_seq;
        }
        internal static int Initialize(bool vision, IntPtr parameters, out IntPtr handle) {
            string library = vision ? "am_vision.dll" : "am_camera.dll";
            try {
                int status = vision ? Native.amv_init(parameters, out handle) : Native.amc_init(parameters, out handle);
                if (status != 0) {
                    using (Arena arena = new Arena()) {
                        InitError info = Arena.Sized<InitError>(); IntPtr p = arena.Struct(info);
                        int result = vision ? Native.amv_get_init_error(p) : Native.amc_get_init_error(p);
                        info = Arena.Read<InitError>(p);
                        if (result == 0 && info.code == status) {
                            int length = Array.IndexOf(info.message, (byte)0); if (length < 0) length = info.message.Length;
                            throw new SdkException(status, "initialize " + library,
                                Encoding.UTF8.GetString(info.message, 0, length), Utf8(info.source_utf8));
                        }
                    }
                }
                return status;
            } catch (DllNotFoundException error) { throw new SdkLoadException(library, error); }
              catch (BadImageFormatException error) { throw new SdkLoadException(library, error); }
              catch (EntryPointNotFoundException error) { throw new SdkLoadException(library, error); }
        }
        internal static void Check(int status, string operation, IntPtr handle, bool vision) {
            if (status == 0) return;
            using (Arena arena = new Arena()) {
                IntPtr p = arena.Struct(Arena.Sized<am_error_info_t>());
                int result = vision ? Native.amv_get_last_error(handle, p) : Native.amc_get_last_error(handle, p);
                am_error_info_t info = Arena.Read<am_error_info_t>(p);
                if (result == 0 && info.code == status)
                    throw new SdkException(status, operation, Utf8(info.message_utf8), Utf8(info.source_utf8));
            }
            SdkException.Check(status, operation);
        }
        internal static string Utf8(IntPtr pointer) {
            if (pointer == IntPtr.Zero) return null;
            int length = 0; while (Marshal.ReadByte(pointer, length) != 0) ++length;
            byte[] bytes = new byte[length]; Marshal.Copy(pointer, bytes, 0, length);
            return Encoding.UTF8.GetString(bytes);
        }
        internal static SdkException Copy(int status, string operation, am_error_info_t info) {
            return new SdkException(status, operation, Utf8(info.message_utf8), Utf8(info.source_utf8));
        }
    }
}
