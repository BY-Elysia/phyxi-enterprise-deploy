# 2026-11-12 · Rust 轻量控制台首版

## 目标

在 `dev` 分支替代 Notebook 入口，用 Rust 实现必要的配置、执行与观察闭环，控制部署重量并明确模块分工。每次提交记录开发纪要。

## 变更

- 新增 Rust workspace：Axum/Tokio、内嵌 SQLite，单二进制包含 HTML/CSS 与少量原生 JavaScript，无 Node/TS/WASM 构建链。
- 模块划分为配置校验、持久化、Docker/vLLM 运行时、HTTP/任务提交与静态 UI。配置与任务使用不可变快照关联。
- 实现模型名/本地路径/镜像摘要/上下文/并发/显存预算/解析器表单，GPU UUID 选择与 TP 校验。
- 实现 GPU 检查、镜像准备、模型启动、健康探测、停止、合作式取消、任务历史、增量日志；运行状态与任务成功分开显示。
- 本机监听、访问令牌、单进程文件锁、任务互斥、容器 owner 校验、明确 Unix Docker socket、无 shell 命令拼接。
- 删除 Notebook 与 Jupyter 引导脚本；旧企业脚本/网关/客户端/文档/保留测试移至 legacy，仅作为迁移参考。
- 新增架构、运维、开发约定；新增 commit-msg 钩子与逐 commit 的 CI 纪要检查。
- 创建 dev 前，将尚未提交的旧 H20/Notebook 工作保存为本地 stash：`pre-dev: preserve legacy H20 and notebook work`。未将其混入新实现，也未删除备份。

## 验证

- `cargo fmt --all -- --check`：通过。
- `cargo clippy --workspace --all-targets --locked -- -D warnings`：通过。
- `cargo test --workspace --locked`：7 项通过。
- `cargo build --workspace --locked` 与 `cargo build --release --locked`：通过；本机 macOS ARM64 release 二进制约 4.1 MiB，非 Linux 包体承诺。
- `python3 scripts/smoke-console.py`：真实二进制 HTTP 闭环，隔离模拟 Docker/nvidia-smi；覆盖鉴权、配置、GPU、启动、健康、日志、快照、停止、归属保护、占用拒绝、取消、重启恢复、单写入者锁。
- `node --check console/web/app.js`：语法通过（仅开发检查，无 Node 运行依赖）。
- `python3 -m unittest discover -s legacy/tests -v`：17 项通过。
- `python3 legacy/scripts/check-repository.py`：通过。
- 浏览器预览工具调用超时，未完成目视验收。未运行真实 GPU/模型推理。

## 限制

- 首版为单机单实例 direct Docker/vLLM，未接入 GPUStack/Xinference/Semaphore。已建立职责边界，按具体需求再接入，避免三套管理服务重复运行。
- 使用已准备好的本地模型权重，不管理权重下载。未认证 GLM-5.3-Flash/H20 或其他具体组合；部分模型可能需要扩充受控启动参数。
- 模型 API 仅回环地址，尚无企业网关/业务验收/自动回滚/多租户/多服务器。健康检查不等于业务验收。
- 取消在命令边界生效；镜像拉取期间不会立即中断。取消/失败保留已创建容器供诊断，停止操作清理容器。
- 关闭浏览器任务继续；控制台进程重启则标记 interrupted，检查真实状态后手动停止/重试，不自动重放。
- 首版按任务保留最近 2000 个日志片段、页面展示最近 30 个任务；数据库任务快照尚无自动保留策略。
- 下一步优先在目标 Linux GPU 服务器验证一个小模型的完整流程，再按实际 GLM 参数补充适配与业务验收。
