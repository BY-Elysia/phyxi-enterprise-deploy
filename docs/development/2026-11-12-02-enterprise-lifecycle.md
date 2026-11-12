# 2026-11-12 · 融合企业部署生命周期

## 目标

根据最新澄清，将 GPUStack、Xinference、Semaphore 的相关职责融合进一个轻量产品，不安装三个平台。覆盖只有 Ubuntu 与 NVIDIA 驱动的服务器起步流程，先完成本地小模型测试，再验证目标 GPU 服务器。

## 变更

- 保留 Rust/Axum、SQLite 和内嵌原生页面，版本更新为 0.2.0；按 API、统一任务、配置状态和系统运行时分层。
- 新增环境检查与固定的 Ubuntu 依赖准备入口、非特权 systemd 安装脚本；不替换驱动/CUDA，不操作已有容器或 GPU 工作负载。
- 新增公开模型仓库固定 revision 下载、SHA256 校验及清单；CPU llama.cpp 后端用于本地低内存验证，与 vLLM 实例互斥。
- 新增独立 Nginx 网关：TLS、来源网段、工作站令牌、SSE；证书校验、客户端材料和 OpenCode 2.2 模板导出。
- 新增真实文本/SSE、可选图片及工具验收；发布重新经网关验收，拒绝配置偏离，保存配置与结果。
- 新增有界资源采样、轻量趋势图与可选 Prometheus 配置；四步页面组织环境、模型、接入、验收。
- 增加协议/真实小模型测试，更新架构、运行文档和本地验证记录。

## 验证

- `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets --locked -- -D warnings` 通过。
- `cargo test --workspace --locked`：13 项通过；debug/release 构建通过。
- `python3 scripts/smoke-console.py`：HTTP、归属、快照、GPU 占用模拟、取消与重启检查通过。
- `python3 scripts/test-api-contract.py`：下载校验、文本/SSE/图片/两轮工具协议及失败覆盖旧报告通过，使用模拟服务。
- `scripts/test-small-model.py`：真实 SmolLM2 135M Q4 CPU 推理、真实 Nginx TLS、鉴权、发布、导出、配置偏离拒绝通过。
- 本机采样 RSS：控制台 5472 KiB，模型 287984 KiB，Nginx 8624 KiB；详见 `docs/local-validation.md`。
- `bash -n scripts/install-console.sh`、`node --check console/web/app.js`、`git diff --check` 通过。

## 限制

- A10 已关机，未验证真实 GPU/vLLM、Ubuntu 包安装、企业 CA 和 Windows/OpenCode。图片和工具只验证协议，不宣称小模型具备这些能力。
- 本次浏览器截图为空白，未完成新版 UI 目视验收；HTTP 与流程测试通过。
- 单机单实例、单工作站令牌；当前发布为验收记录，切换模型有停机窗口，无自动回滚、多节点或平台调度。
- 公开模型下载无私有源认证/断点续传；任务快照尚无自动清理。主机准备只能在满足无既有工作负载的前提下执行，详细包安装输出未逐行实时展示。
- 本地推理和网关测试进程已停止；控制台预览可以单独保持运行。浏览器内存不计入控制台进程 RSS。
