# ABI 1.1 本地兼容扩展冻结依据

本地开发首组新增两个Media导出：显式解码设备初始化探测、已有context/decoder的UTF8 JSON快照查询。合同以dual-dll-abi.md §1.1扩展为准；旧结构、枚举、导出和所有权保持，旧1.0客户端仍可初始化新DLL。新客户端使用新增导出前须协商1.1；SDK包版本仍开发期，发布时另行定版。

历史1.0实施冻结正文保持原字节，先将freeze-v1.json原件复制到sdk/abi/history/freeze-v1-r5.json，新快照引用其SHA。freeze-v1.json文件名沿用“ABI主版本1”的当前指针，内容abiVersion=65537，不覆盖旧快照。仅更新当前头文件、常量及语言生成物；布局没有变化。冻结只表示本地合同受控，运行验收和正式发布另记。

未承诺：RTSP耐压与完整设备矩阵、其他平台/硬件矩阵、硬件推理自动选择、4h/24h。无需新增通用框架或改宿主，禁止通过保留字段偷偷扩展。

本地r7兼容增加amc_source_set_decode_policy，原get_decode_hardware_report接受source句柄；不改旧结构/枚举。RTSP默认PREFER_HW，单拥有者在首次像素订阅前设置，push尚未装回调也冻结；同值幂等、共享/冻结后改变或ISAPI均modeConflict。source关闭快照无实际活动值，不读取其他共享拥有者后续状态。JSON新增source种类、等待状态、image-cpu后端及decode_generation诊断值，既有必需字段/意义保持。

r6当前冻结原件先存sdk/abi/history/freeze-v1.1-r6.json，r7引用该原件SHA；r6所引用的历史文件可按Git b065bf1查原字节。1.1尚未公开发布，当前main绑定与同提交DLL配套，不混用r6开发制品。故障仅清像素队列、不改变压缩配置代次，详细边界见native-hardware.md；本地冻结不冒充R3耐压或H2/H5全量完成。
