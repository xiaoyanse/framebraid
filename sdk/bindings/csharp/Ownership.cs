using System;
using System.Runtime.InteropServices;
using System.Threading;
using Microsoft.Win32.SafeHandles;

namespace AmrtSdk {
    internal sealed class NativeHandle : SafeHandleZeroOrMinusOneIsInvalid {
        private readonly Native.CloseFunction close;
        private readonly Native.DestroyFunction destroy;
        private readonly Native.ReleaseFunction release;
        private readonly NativeHandle parent;
        private readonly Action onReleased;
        private bool parentPinned;
        private bool ownershipReturned;
        internal NativeHandle(IntPtr raw, NativeHandle parentHandle, Native.CloseFunction closer,
                              Native.DestroyFunction destructor, Native.ReleaseFunction releaser, Action released) : base(true) {
            parent = parentHandle; close = closer; destroy = destructor; release = releaser;
            onReleased = released;
            if (parent != null) parent.DangerousAddRef(ref parentPinned);
            SetHandle(raw);
        }
        internal bool TryClose(uint timeout) {
            if (IsClosed || IsInvalid) return true;
            if (release != null) SdkException.Check(release(handle), "release");
            else {
                uint state;
                SdkException.Check(close(handle, timeout, out state), "close");
                if (state != 3) return false;
                SdkException.Check(destroy(handle), "destroy");
            }
            SetHandleAsInvalid(); UnpinParent(); return true;
        }
        private void UnpinParent() {
            if (!ownershipReturned) { ownershipReturned = true; if (onReleased != null) onReleased(); }
            if (parentPinned) { parentPinned = false; parent.DangerousRelease(); }
        }
        protected override bool ReleaseHandle() {
            // 终结器只尽力关闭；超时绝不强 destroy，不卸载有在途工作的原生库。
            try {
                if (release != null) {
                    if (release(handle) != 0) return false;
                } else {
                    uint state;
                    if (close(handle, 0, out state) != 0 || state != 3 || destroy(handle) != 0) return false;
                }
                UnpinParent(); return true;
            } catch { return false; }
        }
    }

    public abstract class OwnedResource : IDisposable {
        internal readonly object Gate = new object();
        internal readonly NativeHandle Handle;
        private readonly OwnedResource parent;
        private int children;
        private bool disposed;
        internal OwnedResource(IntPtr raw, OwnedResource owner, Native.CloseFunction close,
                               Native.DestroyFunction destroy, Native.ReleaseFunction release) {
            parent = owner;
            Handle = new NativeHandle(raw, owner == null ? null : owner.Handle, close, destroy, release, owner == null ? null : new Action(owner.ReleaseChild));
            if (parent != null) Interlocked.Increment(ref parent.children);
        }
        internal IntPtr Raw {
            get { if (disposed || Handle.IsClosed || Handle.IsInvalid) throw new ObjectDisposedException(GetType().Name); return Handle.DangerousGetHandle(); }
        }
        internal virtual void BeforeClose() { }
        public void Close(uint timeoutMs) {
            lock (Gate) {
                if (disposed) return;
                if (Volatile.Read(ref children) != 0) throw new InvalidOperationException("Close child resources before their parent");
                BeforeClose();
                if (!Handle.TryClose(timeoutMs)) throw new TimeoutException("SDK is still closing; release borrowed resources then retry Close");
                Handle.Dispose(); disposed = true;
            }
        }
        public void Dispose() { Close(5000); }
        private void ReleaseChild() { Interlocked.Decrement(ref children); }
    }
}
