# 命令分离与测试产物清理验证

日期2026-10-26，SDK源码基线`3d54853`。用户要求先用实际清理验证规则，再写入Codex全局约束；不改业务代码、不改AM-RT、不发布。

## 实测

原组合脚本包含`Get-Item -Force`检查、文档修改和`Remove-Item`，被自动审批返回`blocked by policy`，整条命令未执行。原记录保留，不把它直接认定为删除动作本身或文件权限被拒绝。

本轮在独立只读调用核验`.local/tests`、目标父目录及文件都无reparse point，目标在测试根内；废弃JPEG SHA-256与原证据一致，没有本轮造流/RTSP服务进程在用。随后仅执行：

```powershell
Remove-Item -LiteralPath 'E:\projects\framebraid\.local\tests\r2-codec-sampling-inputs\source-mjpeg.avi' -ErrorAction Stop
```

退出码0。下一独立只读调用`Test-Path`返回False，成功清除6,449,576字节废弃样本。此结果支持分离只读与副作用调用，不能证明所有策略拒绝都由`-Force`引起，也不构成提升权限或绕过实际操作拒绝的授权。

## 其余闲置工作目录

同一`.local/tests`内本轮已完成的16个`r2-cadence-*`及`r2-codec-sampling-*`工作目录，经绝对路径/链接/在用进程检查后，把日志、配置和结果原字节复制到忽略的`.local/evidence/cleanup-command-split-20261026/workspaces`，逐文件校验SHA-256。独立删除调用只含明确绝对路径的`Remove-Item -Recurse`，退出码0；随后再次核验16个源目录不存在、归档内容指纹一致。

对照DLL目录`r2-cadence-ab`、后续R2测试所需输入目录`r2-codec-sampling-inputs`继续保留；必要DLL指纹和输入存在性另行核对。其他测试根、业务数据、构建、正式制品与源码未纳入本次删除。完整清单/哈希及最终状态为上述私有证据目录的`manifest.json`和`result.json`。

## 规则同步

验证成功后，规则写入`C:/Users/14212/.codex/AGENTS.md`，同一修改同步至规则源码`C:/Users/14212/.claude/platforms/codex/AGENTS.md`。只读检查和删除/修改/服务操作必须独立工具调用；遇拒绝先查完整脚本组合，每步核验；独立副作用操作仍拒绝则停止，不变换工具、权限或编码绕过。原有数据、链接、授权和CodeGraph限制保留。

修改前已保留原字节备份与哈希；仅同步本次两份文件，不运行全量install或触碰其他平台配置。带运行规则绝对路径的分发audit通过，未提交/推送规则仓库；SDK清理记录按既有授权本地提交，二者范围分开。

最终核验：归档62个文件、262,252字节，16个工作目录及废弃JPEG均不存在；两份必要对照DLL与后续输入仍在。Codex规则audit通过（findings为空），分发相关14项测试通过，dependencies齐全，Git diff/敏感信息检查未发现本轮新增凭据。规则源码只有本次AGENTS.md未提交改动，归属本维护任务；运行规则已同步，未推送/发布。原字节规则备份及验证清单保存在规则仓库忽略的backups/codex-command-split-20261026-1793028025237325700。
