using System;
using System.Runtime.InteropServices;
using System.Text;

namespace AmrtSdk {
    public sealed partial class VisionContext {
        public string GetSelectionAttemptReport(ulong attemptId) {
            lock (Gate) return SelectionReports.Read(Raw, attemptId, false);
        }
        public Model LoadModelSelected(string path, am_backend_candidate_t[] candidates, out ulong attemptId,
                                       string profile = null, bool allowFallback = true, uint budgetMs = 1500) {
            return LoadSelected(path, null, candidates, out attemptId, profile, allowFallback, budgetMs);
        }
        public Model LoadModelMemorySelected(byte[] bytes, am_backend_candidate_t[] candidates, out ulong attemptId,
                                             string profile = null, bool allowFallback = true, uint budgetMs = 1500) {
            if (bytes == null) throw new ArgumentNullException("bytes");
            return LoadSelected(null, bytes, candidates, out attemptId, profile, allowFallback, budgetMs);
        }
        private Model LoadSelected(string path, byte[] bytes, am_backend_candidate_t[] candidates, out ulong attemptId,
                                   string profile, bool fallback, uint budget) {
            attemptId = 0;
            if (candidates == null || candidates.Length < 1 || candidates.Length > 3)
                throw new ArgumentException("Provide one to three backend/device candidates", "candidates");
            lock (Gate) using (Arena arena = new Arena()) {
                HardwareReports.RequireApi(true);
                IntPtr cap = arena.Struct(Arena.Sized<amv_capability_t>());
                Diagnostics.Check(Native.amv_query_capability(Raw, cap), "query selection capability", Raw, true);
                if ((Arena.Read<amv_capability_t>(cap).feature_bits & VisionCapabilities.OrderedSelection) == 0)
                    throw new NotSupportedException("Ordered selection requires matching FrameBraid ABI 1.1 bindings and library");
                int size = Marshal.SizeOf(typeof(am_backend_candidate_t));
                IntPtr values = arena.Allocate(checked(size * candidates.Length));
                for (int i = 0; i < candidates.Length; ++i)
                    Marshal.StructureToPtr(candidates[i], IntPtr.Add(values, i * size), false);
                am_backend_selection_t policy = Arena.Sized<am_backend_selection_t>();
                policy.candidates = values; policy.candidate_count = (uint)candidates.Length;
                policy.allow_fallback = fallback ? 1U : 0U; policy.budget_ms = budget;
                IntPtr raw = arena.Allocate(IntPtr.Size), id = arena.Allocate(8);
                int code;
                if (bytes == null) {
                    am_model_desc_t desc = Arena.Sized<am_model_desc_t>();
                    desc.path_utf8 = arena.Utf8(System.IO.Path.GetFullPath(path));
                    desc.profile_id = arena.Utf8(profile); desc.load_mode = profile == null ? 1U : 0U; desc.intra_threads = 2;
                    code = Native.amv_model_load_selected(Raw, arena.Struct(desc), arena.Struct(policy), raw, id);
                } else {
                    am_model_memory_desc_t desc = Arena.Sized<am_model_memory_desc_t>();
                    desc.bytes = arena.Bytes(bytes); desc.size = (ulong)bytes.LongLength;
                    desc.profile_id = arena.Utf8(profile); desc.load_mode = profile == null ? 1U : 0U;
                    code = Native.amv_model_load_memory_selected(Raw, arena.Struct(desc), arena.Struct(policy), 2, raw, id);
                }
                attemptId = unchecked((ulong)Marshal.ReadInt64(id));
                Diagnostics.Check(code, "load selected model", Raw, true);
                return new Model(this, Marshal.ReadIntPtr(raw));
            }
        }
    }
    public sealed partial class Model {
        public string GetSelectionReport() { lock (Gate) return SelectionReports.Read(Raw, 0, true); }
    }
    internal static class SelectionReports {
        internal static string Read(IntPtr owner, ulong attemptId, bool model) {
            using (Arena arena = new Arena()) {
                IntPtr buffer = arena.Allocate(8192), count = arena.Allocate(8);
                int code = model ? Native.amv_get_model_selection_report(owner, buffer, 8192, count)
                                 : Native.amv_get_selection_attempt_report(owner, attemptId, buffer, 8192, count);
                Diagnostics.Check(code, "read selection report", owner, true);
                long length = Marshal.ReadInt64(count);
                if (length < 1 || length > 8192) throw new InvalidOperationException("Invalid selection report length");
                byte[] bytes = new byte[checked((int)length - 1)]; Marshal.Copy(buffer, bytes, 0, bytes.Length);
                return Encoding.UTF8.GetString(bytes);
            }
        }
    }
}
