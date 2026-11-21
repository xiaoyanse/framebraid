# sdk-abi19 U3-B 计数语义合同定稿验证（2026-11-20）

用户经协调线批准本地修订未发布 ABI 1.9 候选合同（2026-11-20 21:3x）。
范围：合同措辞定稿 + 生成链重生成 + freeze 重锚 + 受影响测试复跑。
不提交、不推送、不发布、不动宿主；实现行为零变更（纯注释/文档差异）。

## 合同差异（docs/design/dual-dll-abi.md §3.6a；dual-dll-abi-types.md 附录）

- 弃用「两形态同参数同语义」表述：两形态共用参数与谓词（argmax 类别+严格 `>`），
  计数语义结构性不同——驻留=NMS 前筛选候选数（仅观测），非驻留=对解码去重后
  实例的过滤交付（count=过滤后实例数），两形态计数不可互换比较。
- 驻留 detect.count：启用受支持 detect 头筛选时为筛选候选数；正值不得用作最终
  检出/告警/跟踪/业务统计；未启用恒 0（占位，非空帧证据）；启用时 0 亦仅观测。
- 新增空帧门控前提条款：同一张量、受支持档案/布局、相同类别规则、有效阈值
  （argmax 平局+严格比较+有限合法输出）、最终实例 ⊆ 筛选候选、独立对拍零漏判，
  全部成立才可在该条件下以筛选零值作空帧判据；证据不足一律仅观测。
- 同步：am_device_filter_t / am_infer_params_t.device_filter 头注释；
  vision-ffi 实现注释对齐（execution.rs 三处、results.rs 两处，零行为变更）。

## 生成链与冻结

- 四生成器重跑：generate-abi（171 函数/3 头，仅 am_sdk_types.h 变化）、
  generate-python-abi（97 结构）、generate-csharp-types（无差异）、
  generate-rust-abi（36 文件，crates 侧零差异——注释不进生成 Rust）。
  四者 `--check` 全过；check-bindings --check 过（exit 0）。
- freeze 重锚（r17 原位，am-sdk-cabi-1.9-resident-r17 不改 id：未发布候选、
  git 历史保留旧指纹）：freeze-v1.json 仅 am_sdk_types.h 一条变化
  （sha22af39df…→ab965aab…，bytes 91040→91312），`freeze-abi.mjs --check` 11/11 过。

## 测试复跑（修订+fmt 归一后）

| 门 | 结果 |
|---|---|
| am-sdk-abi 布局/生成测试 | 99 通过 0 失败（am_device_filter_t/am_infer_params_t 布局含内） |
| am-vision-ffi resident 单测 | 5 通过 0 失败 3 ignored（前缀/账本/readout 拒绝/0=未设置） |
| 驻留 CUDA 三门（AM_SDK_ORT_DIR+FB_ROI_PHOTOS，--ignored 串行） | 3/3 过：full_chain readout、filter counts（kernel vs 宿主同谓词整数相等断言内含 43/43、类 10/10、阈值边界、暗帧 0）、budget ledger+stats |
| cargo fmt --check / check-zero-semantics --check | 全过 |

原始日志：`.local/evidence/sdk-resident-ub-20261120/u3b-count-contract-rerun.txt`；
筛选观测数值档见同目录 `abi-prefix-resident_device_filter_counts_match_host_predicate.txt`
（行为零变更，断言口径不变）。

## 边界

- 未启用筛选的零=占位、启用筛选的零=仅观测：本批不开启任何空帧跳过。
- 不适用于生产门控、批量驻留、OV 驻留（合同既有 notSupported 拒绝不变）。
- U3-B 整批仍未验收（G.2 待 AM-RT 同源真机对拍；MJPEG 并行偶发单列未归因）；
  U3-C 未开；v0.8.4 未过门不发。
