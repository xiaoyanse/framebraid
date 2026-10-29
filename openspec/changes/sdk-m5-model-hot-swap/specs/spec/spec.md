## ADDED Requirements

### Requirement: 模型热切换

SDK SHALL 提供 `amv_model_swap`：按槽位（B11 slot_key）原子替换运行中模型，在途请求由旧模型完成后释放，新请求即时路由新模型；消费方句柄与槽位身份不变。

#### Scenario: 换模瞬间并发请求
- **WHEN** 槽位存在在途请求时调用 swap
- **THEN** 在途请求由旧模型完成且结果不丢，swap 后新请求路由新模型，全程无请求失败

#### Scenario: 排空释放
- **WHEN** 被换出模型的所有在途请求完成
- **THEN** 模型按既有 close 语义释放（显式 close 或 DETACH 立即排空），selection 置 legacy_model
