using System;
using System.Collections.Generic;

namespace AmrtSdk {
    public sealed partial class VisionContext {
        public InferencePool CreateInferencePool(string path, uint shards, uint lanes,
                string profile, uint backend, bool strict, uint intraThreads = 0,
                uint? maxBatchItems = null, uint maxWaitMs = 0) {
            if (shards < 2 || shards > 8 || lanes < shards || lanes % shards != 0)
                throw new ArgumentException("Use 2 to 8 shards and a lane count divisible by shards");
            lock (Gate) using (Arena arena = new Arena()) {
                am_model_desc_t model = Arena.Sized<am_model_desc_t>();
                am_backend_policy_t policy = Arena.Sized<am_backend_policy_t>();
                policy.requested = backend; policy.strict = strict ? 1U : 0U;
                model.path_utf8 = arena.Utf8(System.IO.Path.GetFullPath(path));
                model.profile_id = arena.Utf8(profile);
                model.load_mode = profile == null ? 1U : 0U;
                model.backend = arena.Struct(policy); model.intra_threads = intraThreads;
                am_pool_desc_t desc = Arena.Sized<am_pool_desc_t>();
                desc.shards = shards; desc.lanes = lanes; desc.model = arena.Struct(model);
                if (maxBatchItems.HasValue) {
                    am_batching_options_t options = Arena.Sized<am_batching_options_t>();
                    options.flags = 1; options.max_batch_items = maxBatchItems.Value;
                    options.max_wait_ms = maxWaitMs;
                    desc.batching = arena.Struct(options);
                }
                IntPtr raw;
                Diagnostics.Check(Native.amv_pool_create(Raw, arena.Struct(desc), out raw), "create inference pool", Raw, true);
                return new InferencePool(this, raw, shards, lanes);
            }
        }
    }

    public sealed class InferencePool : OwnedResource {
        private readonly VisionContext context;
        private readonly uint lanes;
        private bool closing;

        internal InferencePool(VisionContext owner, IntPtr raw, uint shards, uint laneCount)
            : this(owner, raw, shards, laneCount, new List<IntPtr>()) { }

        private InferencePool(VisionContext owner, IntPtr raw, uint shards, uint laneCount, List<IntPtr> children)
            : base(raw, owner, Native.amv_pool_close, handle => Destroy(handle, children), null) {
            context = owner; lanes = laneCount;
            for (uint shard = 0; shard < shards; shard++) children.Add(Lane(shard * (lanes / shards)));
        }

        private static int Destroy(IntPtr pool, List<IntPtr> children) {
            while (children.Count != 0) {
                int index = children.Count - 1;
                int status = Native.amv_object_destroy(children[index]);
                if (status != 0) return status;
                children.RemoveAt(index);
            }
            return Native.amv_object_destroy(pool);
        }

        private IntPtr Lane(uint lane) {
            if (closing) throw new InvalidOperationException("Inference pool is closing");
            if (lane >= lanes) throw new ArgumentOutOfRangeException("lane");
            IntPtr model;
            Diagnostics.Check(Native.amv_pool_lane_model(Raw, lane, out model), "resolve inference lane", Raw, true);
            return model;
        }

        internal override void BeforeClose() { closing = true; }

        internal ulong SubmitPixel(uint lane, am_pixel_view_t pixel, am_raw_preprocess_t? preprocessing, float confidence) {
            lock (Gate) using (Arena arena = new Arena()) {
                am_image_input_t image = Arena.Sized<am_image_input_t>();
                image.kind = 2; image.view = arena.Struct(pixel);
                return Model.SubmitImage(Lane(lane), arena, image, preprocessing, confidence);
            }
        }

        public ulong SubmitEncoded(uint lane, byte[] bytes, bool png,
                                  am_raw_preprocess_t? preprocessing = null, float confidence = 0.25F) {
            lock (Gate) using (Arena arena = new Arena()) {
                am_image_input_t image = Arena.Sized<am_image_input_t>();
                image.kind = png ? 1U : 0U;
                image.encoded = arena.Bytes(bytes); image.encoded_size = (ulong)bytes.LongLength;
                return Model.SubmitImage(Lane(lane), arena, image, preprocessing, confidence);
            }
        }

        public InferenceResult InferEncoded(uint lane, byte[] bytes, bool png, out ulong request,
                                           am_raw_preprocess_t? preprocessing = null, float confidence = 0.25F) {
            lock (Gate) using (Arena arena = new Arena()) {
                am_image_input_t image = Arena.Sized<am_image_input_t>();
                image.kind = png ? 1U : 0U;
                image.encoded = arena.Bytes(bytes); image.encoded_size = (ulong)bytes.LongLength;
                return Model.InferImage(context, Lane(lane), arena, image, preprocessing, confidence, out request);
            }
        }
    }
}
