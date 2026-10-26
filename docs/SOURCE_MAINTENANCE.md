# Source maintenance boundary / 后续源码维护边界

FrameBraid is the sole maintenance source for Media/Vision SDK code. AM-RT's
original decoupling is merged. Its authorized 2026-10-27 intermediate integration
consumes source commit `9ecd93307fc46947cf1aa70be165077e1cd1f072` through
Cargo Git dependencies. The source commit is available on GitHub main; it is
not a new tag or Release. Published v0.1.0 remains unchanged.
The host no longer carries seven editable core crates. The host statically links
SDK code; external DLL delivery is a separate option, not a second host runtime.

FrameBraid 为 SDK 唯一维护源。AM-RT 的源头解耦已合入，本轮已按授权切换为固定
Git 源码依赖并移除七份核心副本；业务 DTO、凭据持久化和任务适配仍在宿主。
SDK 未发布的本地改动不会自动进入宿主，不在两仓分别修核心。

Core fixes are reviewed/tested here. A host upgrade explicitly changes complete
revisions and lockfiles, verifies source identity/builds/affected behavior, and
is committed independently. Existing releases and extraction records are not
rewritten. No automatic source synchronization, upload or DLL replacement is implied.

版本升级与目录说明见 [Local layout / 两仓维护](LOCAL_LAYOUT.md)。本轮只本地提交，
不推送、不打标签、不发布；原始来源记录与历史交付证据保留。

## Reviewed local source revisions / 已评审源码修订

`sdk-source-manifest.json` remains the original extraction record. The optional
`sdk-source-revisions.json` binds that exact file's SHA-256 and a review-base commit,
then records each authorized replacement/addition with its previous/current digest and reason.
The checker verifies unchanged files against the original and revised files against the overlay;
unregistered drift still fails. Git history records the review and subsequent implementation commit.

原始清单保留原字节；修订文件必须有前后摘要和原因，新增文件以前摘要 null 明确登记。
修订不是自动信任机制，也不是正式发布记录；须人工审查差异，不能以覆盖指纹代替验证。
