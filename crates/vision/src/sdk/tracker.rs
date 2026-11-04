//! 目标跟踪（§4.1b，SORT 级轻量关联）纯逻辑：恒速预测 + IoU 贪心匹配 +
//! 确认帧数与 TTL 回收；输出喂入框原值透传（预测仅供匹配），不触引擎与像素。

/// IoU 匹配阈值默认值（create 参数 0 时生效）。
pub const IOU_THRESHOLD_DEFAULT: f32 = 0.3;
/// TTL 回收帧数默认值。
pub const TTL_FRAMES_DEFAULT: u32 = 30;
/// 新轨迹确认命中数默认值。
pub const CONFIRM_HITS_DEFAULT: u32 = 3;
/// 轨迹表容量默认值。
pub const MAX_TRACKS_DEFAULT: usize = 64;

/// 跟踪器建档参数（create 时定值，运行期不变；FFI 层完成 0=默认解析后构造）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrackerParams {
    /// 匹配 IoU 下限，低于该值的 (轨迹,框) 对不配。
    pub iou_threshold: f32,
    /// 连续未命中达该帧数轨迹回收。
    pub ttl_frames: u32,
    /// 新轨迹累计命中达该数才输出（防误检闪烁）。
    pub confirm_hits: u32,
    /// 轨迹表容量上限，满表新目标不建档。
    pub max_tracks: usize,
}

impl Default for TrackerParams {
    fn default() -> Self {
        Self {
            iou_threshold: IOU_THRESHOLD_DEFAULT,
            ttl_frames: TTL_FRAMES_DEFAULT,
            confirm_hits: CONFIRM_HITS_DEFAULT,
            max_tracks: MAX_TRACKS_DEFAULT,
        }
    }
}

impl TrackerParams {
    /// 有限性与范围校验：iou 0..=1、ttl ≤ 10000、confirm ≤ 1000、
    /// max_tracks 1..=4096；越界由 FFI 层拒绝为非法参数。
    pub fn valid(&self) -> bool {
        self.iou_threshold.is_finite()
            && (0.0..=1.0).contains(&self.iou_threshold)
            && self.ttl_frames <= 10_000
            && self.confirm_hits <= 1_000
            && (1..=4_096).contains(&self.max_tracks)
    }
}

/// 单帧跟踪输出：bounds/confidence/class_id 为喂入值透传（不平滑）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrackOutput {
    pub bounds: [f32; 4],
    pub confidence: f32,
    pub class_id: u32,
    pub track_id: u64,
    pub age_frames: u32,
    pub hits: u32,
}

struct Track {
    id: u64,
    /// 最近一次喂入框（输出透传与速度估计的基准）。
    bounds: [f32; 4],
    /// 恒速速度估计（当帧与上帧中心差；建档帧为 0，丢失期间冻结）。
    vx: f32,
    vy: f32,
    hits: u32,
    age: u32,
    misses: u32,
}

impl Track {
    fn center(bounds: [f32; 4]) -> (f32, f32) {
        ((bounds[0] + bounds[2]) / 2.0, (bounds[1] + bounds[3]) / 2.0)
    }
    /// 匹配用预测框：最近喂入框按速度推进 misses+1 步（丢失期间预测持续推进）。
    fn predicted(&self) -> [f32; 4] {
        let advance = self.misses as f32 + 1.0;
        let (dx, dy) = (self.vx * advance, self.vy * advance);
        [
            self.bounds[0] + dx,
            self.bounds[1] + dy,
            self.bounds[2] + dx,
            self.bounds[3] + dy,
        ]
    }
}

/// IoU：退化（零面积/不相交/负宽高）返回 0，不产生 NaN。
fn iou(a: [f32; 4], b: [f32; 4]) -> f32 {
    let intersection =
        ((a[2].min(b[2]) - a[0].max(b[0])).max(0.0)) * ((a[3].min(b[3]) - a[1].max(b[1])).max(0.0));
    let area_a = ((a[2] - a[0]).max(0.0)) * ((a[3] - a[1]).max(0.0));
    let area_b = ((b[2] - b[0]).max(0.0)) * ((b[3] - b[1]).max(0.0));
    let union = area_a + area_b - intersection;
    if union > 0.0 {
        intersection / union
    } else {
        0.0
    }
}

/// SORT 级轻量跟踪器：update 按调用序即帧序；调用方（FFI 域锁）保证串行。
pub struct Tracker {
    params: TrackerParams,
    tracks: Vec<Track>,
    next_id: u64,
}

impl Tracker {
    pub fn new(params: TrackerParams) -> Self {
        Self {
            params,
            tracks: Vec::new(),
            next_id: 1,
        }
    }

    /// 逐帧关联：预测框与喂入框按 IoU 降序贪心配对（每轨迹每框至多一次，
    /// IoU 低于阈值不配）；配对轨迹透传更新（bounds=喂入框、速度=中心差、
    /// hits+1、age+1、misses=0），未配对轨迹 misses+1（达 TTL 回收），
    /// 未配对框建档（满表跳过）；返回当帧有命中且已确认（hits≥confirm_hits）
    /// 的轨迹，按 track_id 升序。
    pub fn update(&mut self, boxes: &[([f32; 4], f32, u32)]) -> Vec<TrackOutput> {
        let mut candidates: Vec<(f32, usize, usize)> = Vec::new();
        for (ti, track) in self.tracks.iter().enumerate() {
            let predicted = track.predicted();
            for (bi, b) in boxes.iter().enumerate() {
                let overlap = iou(predicted, b.0);
                if overlap >= self.params.iou_threshold {
                    candidates.push((overlap, ti, bi));
                }
            }
        }
        // IoU 降序贪心；同分按（轨迹序,框序）稳定拆解，配对结果确定。
        candidates.sort_by(|a, b| {
            b.0.total_cmp(&a.0)
                .then_with(|| a.1.cmp(&b.1))
                .then_with(|| a.2.cmp(&b.2))
        });
        let mut track_used = vec![false; self.tracks.len()];
        let mut box_used = vec![false; boxes.len()];
        let mut outputs: Vec<TrackOutput> = Vec::new();
        for (_, ti, bi) in candidates {
            if track_used[ti] || box_used[bi] {
                continue;
            }
            track_used[ti] = true;
            box_used[bi] = true;
            let (bounds, confidence, class_id) = boxes[bi];
            let track = &mut self.tracks[ti];
            let (cx, cy) = Track::center(bounds);
            let (px, py) = Track::center(track.bounds);
            track.vx = cx - px;
            track.vy = cy - py;
            track.bounds = bounds;
            track.hits += 1;
            track.age += 1;
            track.misses = 0;
            if track.hits >= self.params.confirm_hits {
                outputs.push(TrackOutput {
                    bounds,
                    confidence,
                    class_id,
                    track_id: track.id,
                    age_frames: track.age,
                    hits: track.hits,
                });
            }
        }
        for (track, &used) in self.tracks.iter_mut().zip(&track_used) {
            if used {
                continue;
            }
            track.misses += 1;
            track.age += 1;
        }
        self.tracks
            .retain(|track| track.misses < self.params.ttl_frames);
        for bi in 0..boxes.len() {
            if box_used[bi] {
                continue;
            }
            if self.tracks.len() >= self.params.max_tracks {
                // 满表：剩余未配对框全部跳过（不建档）。
                break;
            }
            let (bounds, confidence, class_id) = boxes[bi];
            let id = self.next_id;
            self.next_id += 1;
            self.tracks.push(Track {
                id,
                bounds,
                vx: 0.0,
                vy: 0.0,
                hits: 1,
                age: 0,
                misses: 0,
            });
            if self.params.confirm_hits <= 1 {
                outputs.push(TrackOutput {
                    bounds,
                    confidence,
                    class_id,
                    track_id: id,
                    age_frames: 0,
                    hits: 1,
                });
            }
        }
        outputs.sort_by_key(|o| o.track_id);
        outputs
    }

    /// 清空轨迹表并重新编号（track_id 复位为 1）。
    pub fn reset(&mut self) {
        self.tracks.clear();
        self.next_id = 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(iou: f32, ttl: u32, confirm: u32, max: usize) -> TrackerParams {
        TrackerParams {
            iou_threshold: iou,
            ttl_frames: ttl,
            confirm_hits: confirm,
            max_tracks: max,
        }
    }

    fn box_at(x0: f32, y0: f32, x1: f32, y1: f32, conf: f32, class: u32) -> ([f32; 4], f32, u32) {
        ([x0, y0, x1, y1], conf, class)
    }

    fn ids(outputs: &[TrackOutput]) -> Vec<u64> {
        outputs.iter().map(|o| o.track_id).collect()
    }

    #[test]
    fn iou_math_normal_and_degenerate() {
        let a = [0.0, 0.0, 0.1, 0.1];
        assert!((iou(a, a) - 1.0).abs() < 1e-6, "identical boxes IoU=1");
        assert_eq!(iou(a, [0.5, 0.5, 0.6, 0.6]), 0.0, "disjoint=0");
        // 半重叠：交 0.005 / 并 0.015 = 1/3
        assert!((iou(a, [0.05, 0.0, 0.15, 0.1]) - 1.0 / 3.0).abs() < 1e-6);
        // 退化（零面积/负宽高）不产生 NaN
        assert_eq!(iou([0.1, 0.1, 0.1, 0.2], a), 0.0);
        assert_eq!(iou([0.6, 0.6, 0.4, 0.4], a), 0.0);
    }

    #[test]
    fn new_track_hidden_until_confirm_hits() {
        let mut tracker = Tracker::new(TrackerParams::default());
        let b = box_at(0.1, 0.1, 0.4, 0.4, 0.9, 2);
        assert!(tracker.update(&[b]).is_empty(), "第1帧 hits=1 < 3 不输出");
        assert!(tracker.update(&[b]).is_empty(), "第2帧 hits=2 < 3 不输出");
        let out = tracker.update(&[b]);
        assert_eq!(ids(&out), vec![1], "第3帧 hits=3 达确认输出");
        assert_eq!(out[0].hits, 3);
        assert_eq!(out[0].age_frames, 2, "建档帧 age=0，此后每帧+1");
    }

    #[test]
    fn track_ids_increase_monotonically_and_never_reuse() {
        let mut tracker = Tracker::new(params(0.3, 2, 1, 64));
        let a = box_at(0.0, 0.0, 0.2, 0.2, 0.9, 0);
        let b = box_at(0.6, 0.6, 0.8, 0.8, 0.8, 1);
        assert_eq!(ids(&tracker.update(&[a, b])), vec![1, 2]);
        // 连续未命中两帧（ttl=2）后两轨迹均回收
        tracker.update(&[]);
        tracker.update(&[]);
        // 回收后重现：不复用旧 ID，从 3 继续单调递增
        assert_eq!(ids(&tracker.update(&[a, b])), vec![3, 4]);
    }

    #[test]
    fn reappearance_within_ttl_keeps_id() {
        let mut tracker = Tracker::new(params(0.3, 3, 1, 64));
        let a = box_at(0.1, 0.1, 0.3, 0.3, 0.9, 0);
        assert_eq!(ids(&tracker.update(&[a])), vec![1]);
        // 丢失2帧（misses=2 < ttl=3）：轨迹未回收
        assert!(tracker.update(&[]).is_empty());
        assert!(tracker.update(&[]).is_empty());
        let out = tracker.update(&[a]);
        assert_eq!(ids(&out), vec![1], "TTL 内重现保持同 ID");
        assert_eq!(out[0].hits, 2);
        assert_eq!(out[0].age_frames, 3, "丢失帧也计入 age（建档0+3帧）");
    }

    #[test]
    fn reappearance_after_ttl_assigns_new_id() {
        let mut tracker = Tracker::new(params(0.3, 2, 1, 64));
        let a = box_at(0.1, 0.1, 0.3, 0.3, 0.9, 0);
        assert_eq!(ids(&tracker.update(&[a])), vec![1]);
        // 丢失2帧 = misses 达 ttl=2，轨迹删除（第2次空帧后即回收）
        tracker.update(&[]);
        tracker.update(&[]);
        // TTL 内（misses=1）重现仍同 ID：验证删除时机是 misses>=ttl 而非首miss
        let mut tracker2 = Tracker::new(params(0.3, 2, 1, 64));
        assert_eq!(ids(&tracker2.update(&[a])), vec![1]);
        tracker2.update(&[]);
        assert_eq!(ids(&tracker2.update(&[a])), vec![1]);
        // 超 TTL 重现：新 ID
        assert_eq!(ids(&tracker.update(&[a])), vec![2]);
    }

    #[test]
    fn moving_box_tracked_via_velocity_prediction() {
        let mut tracker = Tracker::new(params(0.3, 30, 1, 64));
        // 建档帧速度=0；随后以 0.02/帧匀速运动建立速度估计
        assert_eq!(
            ids(&tracker.update(&[box_at(0.00, 0.0, 0.10, 0.1, 0.9, 0)])),
            vec![1]
        );
        assert_eq!(
            ids(&tracker.update(&[box_at(0.02, 0.0, 0.12, 0.1, 0.9, 0)])),
            vec![1]
        );
        assert_eq!(
            ids(&tracker.update(&[box_at(0.04, 0.0, 0.14, 0.1, 0.9, 0)])),
            vec![1]
        );
        // 第4帧位移跳到 0.06：与上帧静态 IoU=0.25 < 0.3，但预测框 [0.06,0.16]
        // 与喂入框 IoU≈0.43 ≥ 0.3 —— 恒速预测使其继续配对而非新建档
        let out = tracker.update(&[box_at(0.10, 0.0, 0.20, 0.1, 0.9, 0)]);
        assert_eq!(ids(&out), vec![1], "静态 IoU 不足但预测命中须保持同 ID");
        assert_eq!(out[0].hits, 4);
    }

    #[test]
    fn crossing_boxes_keep_identity() {
        let mut tracker = Tracker::new(params(0.3, 30, 1, 64));
        // 两框相向而行（A 右移 B 左移，0.04/帧），在交叉帧两预测框同时与
        // 两喂入框存在 ≥阈值 的交叉 IoU；贪心按最高 IoU 先配（自身预测 IoU=1）
        // 不得串 ID
        let frame = |ax: f32, bx: f32| {
            [
                box_at(ax, 0.40, ax + 0.20, 0.60, 0.9, 0),
                box_at(bx, 0.40, bx + 0.20, 0.60, 0.8, 1),
            ]
        };
        assert_eq!(ids(&tracker.update(&frame(0.00, 0.40))), vec![1, 2]);
        assert_eq!(ids(&tracker.update(&frame(0.04, 0.36))), vec![1, 2]);
        assert_eq!(ids(&tracker.update(&frame(0.08, 0.32))), vec![1, 2]);
        assert_eq!(ids(&tracker.update(&frame(0.12, 0.28))), vec![1, 2]);
        // 交叉重叠帧：A=[0.16,0.36] B=[0.24,0.44]，交叉 IoU≈0.43 也超阈值
        let out = tracker.update(&frame(0.16, 0.24));
        assert_eq!(ids(&out), vec![1, 2], "交叉帧不得换 ID");
        // 按 track_id 升序输出：id1=A、id2=B，框值透传验证身份
        assert!((out[0].bounds[0] - 0.16).abs() < 1e-6);
        assert!((out[1].bounds[0] - 0.24).abs() < 1e-6);
    }

    #[test]
    fn max_tracks_capacity_blocks_new_tracks() {
        let mut tracker = Tracker::new(params(0.3, 2, 1, 2));
        let a = box_at(0.0, 0.0, 0.1, 0.1, 0.9, 0);
        let b = box_at(0.3, 0.0, 0.4, 0.1, 0.8, 0);
        let c = box_at(0.6, 0.0, 0.7, 0.1, 0.7, 0);
        assert_eq!(
            ids(&tracker.update(&[a, b, c])),
            vec![1, 2],
            "满表第三框不建档"
        );
        // 次帧三框仍在：C 依旧被跳过
        assert_eq!(ids(&tracker.update(&[a, b, c])), vec![1, 2]);
        // A 丢失回收（ttl=2）后腾出容量，C 才可建档
        let mut tracker2 = Tracker::new(params(0.3, 2, 1, 2));
        assert_eq!(ids(&tracker2.update(&[a, b, c])), vec![1, 2]);
        tracker2.update(&[b, c]);
        tracker2.update(&[b, c]);
        let out = tracker2.update(&[b, c]);
        assert_eq!(ids(&out), vec![2, 3], "A 回收后 C 以新 ID 建档");
    }

    #[test]
    fn empty_frame_advances_misses_without_output() {
        let mut tracker = Tracker::new(params(0.3, 30, 1, 64));
        let a = box_at(0.2, 0.2, 0.5, 0.5, 0.9, 4);
        assert_eq!(tracker.update(&[a]).len(), 1);
        for _ in 0..5 {
            let out = tracker.update(&[]);
            assert!(out.is_empty(), "空帧（无命中）绝不输出");
        }
        assert_eq!(ids(&tracker.update(&[a])), vec![1], "未超 TTL 轨迹保持");
    }

    #[test]
    fn outputs_passthrough_fed_values_without_smoothing() {
        let mut tracker = Tracker::new(params(0.3, 30, 1, 64));
        let first = tracker.update(&[box_at(0.10, 0.10, 0.40, 0.40, 0.90, 5)]);
        assert_eq!(
            (first[0].bounds, first[0].confidence, first[0].class_id),
            ([0.10, 0.10, 0.40, 0.40], 0.90, 5),
            "首帧喂入值原样输出"
        );
        // 次帧喂入抖动框与不同置信度/类别：输出=喂入值，不做平均/预测改写
        let second = tracker.update(&[box_at(0.12, 0.08, 0.38, 0.42, 0.55, 6)]);
        assert_eq!(second[0].track_id, first[0].track_id);
        assert_eq!(
            (second[0].bounds, second[0].confidence, second[0].class_id),
            ([0.12, 0.08, 0.38, 0.42], 0.55, 6),
            "透传合同：喂入框值原样出，预测仅供匹配"
        );
        assert_eq!((second[0].age_frames, second[0].hits), (1, 2));
    }

    #[test]
    fn reset_clears_table_and_renumbers() {
        let mut tracker = Tracker::new(params(0.3, 30, 2, 64));
        let a = box_at(0.2, 0.2, 0.4, 0.4, 0.9, 0);
        tracker.update(&[a]);
        tracker.update(&[a]);
        assert_eq!(ids(&tracker.update(&[a])), vec![1]);
        tracker.reset();
        // reset 后：轨迹表清空且重新编号（id 复位为 1），确认计数也重新累计
        assert!(tracker.update(&[a]).is_empty());
        let out = tracker.update(&[a]);
        assert_eq!(ids(&out), vec![1], "reset 后重新从 1 编号");
        assert_eq!((out[0].hits, out[0].age_frames), (2, 1));
    }

    #[test]
    fn params_defaults_and_validation_bounds() {
        assert_eq!(
            TrackerParams::default(),
            params(0.3, 30, 3, 64),
            "合同默认 0.3/30/3/64"
        );
        assert!(TrackerParams::default().valid());
        // 边界值合法
        assert!(params(1.0, 10_000, 1_000, 4_096).valid());
        assert!(params(0.0, 1, 1, 1).valid());
        // 越界非法
        assert!(!params(f32::NAN, 30, 3, 64).valid());
        assert!(!params(f32::INFINITY, 30, 3, 64).valid());
        assert!(!params(1.0001, 30, 3, 64).valid());
        assert!(!params(-0.1, 30, 3, 64).valid());
        assert!(!params(0.3, 10_001, 3, 64).valid());
        assert!(!params(0.3, 30, 1_001, 64).valid());
        assert!(!params(0.3, 30, 3, 4_097).valid());
        assert!(!params(0.3, 30, 3, 0).valid(), "容量 0 无意义非法");
    }

    #[test]
    fn unmatched_box_below_iou_threshold_starts_new_track() {
        let mut tracker = Tracker::new(params(0.3, 30, 1, 64));
        assert_eq!(
            ids(&tracker.update(&[box_at(0.0, 0.0, 0.2, 0.2, 0.9, 0)])),
            vec![1]
        );
        // 与上帧 IoU=0（不相交）必然低于阈值：新建档
        let out = tracker.update(&[box_at(0.5, 0.5, 0.7, 0.7, 0.9, 0)]);
        assert_eq!(ids(&out), vec![2]);
        assert_eq!(out[0].age_frames, 0, "新建档帧 age=0");
    }
}
