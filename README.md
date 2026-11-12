# PhyXi 部署控制台

轻量的单机模型部署控制台：Rust 单二进制、内嵌 HTML/CSS/原生 JavaScript、SQLite。无 Node、TypeScript、WASM、Jupyter、Redis 或外部数据库运行依赖。

当前为 **v0.1 开发版本**。已实现配置表单、GPU 发现与选择、Docker/vLLM 启停、固定配置快照、任务进度和日志、取消请求、重启后的中断标记。模型健康检查不代表图片/工具调用等业务验收通过。

## 构建与运行

需要 Rust stable 及 C 编译器（编译内嵌 SQLite），使用提交的 Cargo.lock：

```bash
cargo build --release --locked
./target/release/phyxi-console
```

打开 `http://127.0.0.1:8765`，将 `.phyxi-console/token` 的内容填入页面。默认状态目录为当前目录下的 `.phyxi-console/`；目录权限 0700，令牌文件 0600。控制台只监听回环地址。远程使用：

```bash
ssh -N -L 8765:127.0.0.1:8765 用户@服务器
```

可选环境变量：

| 变量 | 默认值 | 用途 |
|---|---|---|
| `PHYXI_STATE_DIR` | `.phyxi-console` | 配置、数据库、令牌和实例归属标识；建议使用固定绝对路径 |
| `PHYXI_PORT` | `8765` | 控制台端口 |
| `PHYXI_DOCKER_HOST` | `unix:///var/run/docker.sock` | 本地 Docker socket；共享环境可指定已准备好的独立 Engine |

真实模型运行需 Linux NVIDIA 服务器、可用的驱动/NVIDIA Container Toolkit、Docker CLI 和本地 Docker Engine。控制台使用服务账号已有的 Docker socket 权限，不安装系统依赖、不调用 sudo。管理服务无需 GPU；在无 GPU 工作站上可以打开页面，但设备检测和部署会报告不可用。

## 首次部署

1. 预先准备完整本地模型权重目录（含 `config.json`），以及与模型兼容的 vLLM 镜像摘要。
2. 页面填写服务模型名、权重目录、`镜像名@sha256:64位摘要`。
3. 勾选 GPU，首版 TP 自动等于选卡数；设置上下文、最大序列数和显存比例。解析器按模型填写。
4. 点击“保存并部署”：检查 → 拉取镜像 → 再次检查 GPU → 创建容器 → 等待健康。
5. 查看任务与模型日志。模型 API 只发布到 `127.0.0.1:<配置端口>`。

配置保存不会改变运行实例。换模型/参数须先停止旧实例，再部署。停止只移除此状态目录归属的 `phyxi-console-model` 容器，不删除模型权重。未提供任意命令输入。首版不自动接管旧 Compose/systemd 服务，不允许它们与新版争用同一组 GPU。

## 当前边界

- 单服务器、单实例，直接 Docker/vLLM。GPUStack、Xinference、Semaphore 未安装或集成；借鉴其职责，后续按需新增适配器。
- 使用本地模型；无模型下载器、模型市场、自动调参、多租户、多节点、自动扩缩容。
- 暂无企业 TLS/认证网关、工作站自动导出、业务验收与回滚界面。模型 API 未配置独立 API key，只可通过本机访问，不能直接暴露公网。
- 依赖引擎默认 dtype/KV 配置；尚未认证 GLM-5.3-Flash/H20 或其他具体组合。模型专用参数按目标实测再补充，不声称任意模型即插即用。
- 关闭浏览器不影响任务。控制台进程退出会中断任务；重新启动标记 `interrupted`，不会自动重放。Docker 容器可能继续运行，须查看实际状态后停止/重试。
- 取消在执行边界生效，不强行中断镜像拉取；取消或健康超时不自动删除已创建容器。
- 日志每任务保留最近 2000 个片段，每片段最多 4096 字符。任务历史保存于 SQLite，页面显示最近 30 项；尚未自动清理任务快照。

[架构与分工](docs/architecture.md) · [开发约定](docs/development.md) · [运维说明](docs/operations.md)

## 验证

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --locked
python3 scripts/smoke-console.py
```

HTTP 集成测试使用隔离的模拟 Docker/GPU 命令，不创建真实容器。真实 GPU 性能与兼容性必须在目标服务器验证。

旧实现移至 `legacy/` 供迁移参考，Notebook 已移除。`main` 保留历史版本；迁移前的未提交工作保存在本地 stash，不属于新版运行路径。
