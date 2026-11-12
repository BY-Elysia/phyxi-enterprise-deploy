# 开发约定

主开发分支：`dev`。Rust stable，Cargo.lock 提交，单 workspace。Rust 服务和内嵌静态页面一起构建，不引入 Node/TypeScript 构建链。代码模块分工见 [架构](architecture.md)。

## 每次提交的开发纪要

每次 commit 必须新增 `docs/development/<日期>-<序号>-<主题>.md`，包含以下四节：

- `## 目标`：本次具体要解决的问题。
- `## 变更`：实际完成的模块和行为。
- `## 验证`：执行的检查、结果、是否使用真实 GPU。
- `## 限制`：未完成的内容、边界及下一步。

提交说明必须包含 `Dev-Note: docs/development/<文件>.md`。纪要不提前写“测试通过”，提交前更新实际结果。无需把 commit hash 写入同一 commit 的纪要；Git 本身提供关联。

启用本地钩子（此工作区已配置）：

```bash
git config core.hooksPath .githooks
python3 scripts/check-dev-note.py --staged
```

commit-msg 钩子校验新增纪要与提交说明。CI 对推送范围/PR 范围内每个 commit 做同样校验；新分支以 origin/main 为基线。不要绕过失败的钩子。

## 验证分层

- Rust 单元/HTTP 路由测试：配置约束、命令边界、归属检查、鉴权、互斥、持久化。
- Python HTTP 集成测试：运行真实二进制，替换 PATH 为临时模拟 Docker/nvidia-smi，覆盖执行闭环及重启恢复。
- GPU 目标验收：真实镜像/权重/硬件、加载、推理、内存与停止行为；未执行不能报告通过。

旧代码在 `legacy/`，仅用于按需迁移；不把旧配置的通过结果作为新版模型兼容性证据。
