using System;
using System.Runtime.InteropServices;

namespace AmrtSdk {
    public sealed class DetectCopy {
        public float[] Bounds;
        public float Confidence;
        public int ClassId;
        public string ClassName;
    }
    public sealed class ClassifyCopy {
        public int ClassId;
        public string ClassName;
        public float Logit, Probability;
        public bool HasProbability;
    }
    public sealed class SegmentCopy {
        public float[] Bounds;
        public float Confidence;
        public int ClassId;
        public uint Width, Height, Stride;
        public byte[] Mask;
        public am_roi_map_t RoiMap;
    }
    public sealed class ObbCopy {
        public float[] Vertices;
        public float CenterX, CenterY, Width, Height, Angle, Confidence;
        public int ClassId;
        public bool AngleNormalized;
    }
    public sealed partial class InferenceResult {
        private static void ValidateRows(int actualTask, int expectedTask, uint count, uint max, IntPtr rows) {
            if (actualTask != expectedTask || count > max || (count != 0 && rows == IntPtr.Zero))
                throw new InvalidOperationException("Invalid structured result");
        }
        private static T Row<T>(IntPtr rows, int index) where T : struct {
            return Arena.Read<T>(IntPtr.Add(rows, checked(index * Marshal.SizeOf(typeof(T)))));
        }
        private static string OptionalName(IntPtr value) { return value == IntPtr.Zero ? null : Utf8(value); }
        public DetectCopy[] CopyDetect() {
            lock (Gate) {
                amv_result_t r = Arena.Read<amv_result_t>(Raw);
                ValidateRows(r.task, 2, r.detect.count, 300, r.detect.boxes);
                DetectCopy[] output = new DetectCopy[checked((int)r.detect.count)];
                for (int i = 0; i < output.Length; ++i) {
                    am_detect_box_t v = Row<am_detect_box_t>(r.detect.boxes, i);
                    output[i] = new DetectCopy { Bounds = v.bounds, Confidence = v.confidence, ClassId = v.class_id, ClassName = OptionalName(v.class_name_utf8) };
                }
                return output;
            }
        }
        public ClassifyCopy[] CopyClassify() {
            lock (Gate) {
                amv_result_t r = Arena.Read<amv_result_t>(Raw);
                ValidateRows(r.task, 3, r.classify.count, 1000, r.classify.entries);
                ClassifyCopy[] output = new ClassifyCopy[checked((int)r.classify.count)];
                for (int i = 0; i < output.Length; ++i) {
                    am_classify_entry_t v = Row<am_classify_entry_t>(r.classify.entries, i);
                    output[i] = new ClassifyCopy { ClassId = v.class_id, ClassName = OptionalName(v.class_name_utf8), Logit = v.logit, Probability = v.probability, HasProbability = v.has_probability != 0 };
                }
                return output;
            }
        }
        public SegmentCopy[] CopySegment() {
            lock (Gate) {
                amv_result_t r = Arena.Read<amv_result_t>(Raw);
                ValidateRows(r.task, 4, r.segment.count, 300, r.segment.instances);
                am_segment_instance_t[] rows = new am_segment_instance_t[checked((int)r.segment.count)];
                ulong total = 0;
                for (int i = 0; i < rows.Length; ++i) {
                    am_segment_instance_t v = Row<am_segment_instance_t>(r.segment.instances, i);
                    if (v.mask_width == 0 || v.mask_width > 8192 || v.mask_height == 0 || v.mask_height > 8192 ||
                        v.mask_stride_bytes < (v.mask_width + 7) / 8 || v.mask_bits == IntPtr.Zero)
                        throw new InvalidOperationException("Invalid segment mask storage");
                    total += (ulong)v.mask_stride_bytes * v.mask_height;
                    if (total > 64UL * 1024 * 1024) throw new InvalidOperationException("Segment masks exceed copy budget");
                    rows[i] = v;
                }
                SegmentCopy[] output = new SegmentCopy[rows.Length];
                for (int i = 0; i < rows.Length; ++i) {
                    am_segment_instance_t v = rows[i];
                    byte[] mask = new byte[checked((int)((ulong)v.mask_stride_bytes * v.mask_height))];
                    Marshal.Copy(v.mask_bits, mask, 0, mask.Length);
                    output[i] = new SegmentCopy { Bounds = v.bounds, Confidence = v.confidence, ClassId = v.class_id, Width = v.mask_width, Height = v.mask_height, Stride = v.mask_stride_bytes, Mask = mask, RoiMap = v.roi_map };
                }
                return output;
            }
        }
        public ObbCopy[] CopyObb() {
            lock (Gate) {
                amv_result_t r = Arena.Read<amv_result_t>(Raw);
                ValidateRows(r.task, 5, r.obb.count, 300, r.obb.instances);
                ObbCopy[] output = new ObbCopy[checked((int)r.obb.count)];
                for (int i = 0; i < output.Length; ++i) {
                    am_obb_instance_t v = Row<am_obb_instance_t>(r.obb.instances, i);
                    output[i] = new ObbCopy { Vertices = v.vertices_xy, CenterX = v.center_x, CenterY = v.center_y, Width = v.width, Height = v.height, Angle = v.angle, Confidence = v.confidence, ClassId = v.class_id, AngleNormalized = v.angle_normalized != 0 };
                }
                return output;
            }
        }
    }
}
