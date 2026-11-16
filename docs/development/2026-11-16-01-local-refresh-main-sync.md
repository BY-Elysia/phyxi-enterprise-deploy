# 本地新版重建与 main 同步

## 目标

清理本地旧构建，启动 GitHub dev 新版，并在验证后将 dev 快进合并到 main。

## 变更

- 接口契约测试轮询下载进度时兼容任务初始的 null progress，继续等待实际字节进度；保留后续进度、取消及校验断言。
- 清除旧 target 构建产物，重新构建 debug/release；保留既有本地配置、模型和状态。

## 验证

- cargo fmt --all -- --check：通过。
- cargo clippy --workspace --all-targets --locked -- -D warnings：通过。
- cargo test --workspace --locked：30 通过，1 默认忽略。
- cargo build --workspace --locked 与 cargo build --release --locked：通过。
- scripts/smoke-console.py、scripts/test-api-contract.py、scripts/test-composition.py：通过。
- scripts/test-small-model.py 使用明确的 llama-server、Nginx 路径及小于 200 MiB 的 SmolLM2 GGUF：真实 CPU 模型、TLS 网关和发布流程通过。
- release 控制台在 127.0.0.1:8765 启动，认证接口返回 password 模式，保留原管理员账户。

## 限制

- 本次仅验证 macOS 本地 CPU 环境，不构成 NVIDIA GPU 推理或 Ubuntu 部署验收。
- 控制台为本地后台进程，未配置开机自启；现有配置和模型数据未删除。
