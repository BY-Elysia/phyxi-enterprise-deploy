# PhyXi 轻量部署控制台

一个控制台完成 **环境检查/准备 → 权重准备 → 模型部署 → TLS 网关 → 接口验收 → 发布记录与客户端导出**。面向一台 Ubuntu NVIDIA 服务器、一套业务模型服务；本地开发可用 CPU 小模型验证流程。

融合 GPUStack 的资源/实例管理、Xinference 的引擎和模型配置、Semaphore 的受控任务/日志职责，不部署或依赖这三个平台。当前实现为 Rust API + 独立任务模块 + SQLite + 内嵌 HTML/CSS/少量原生 JS。技术选择以实际资源占用为准，不为语言本身引入复杂性。无需 Node、TS、WASM、Redis、外部数据库；Prometheus/Grafana 可选。

## 当前能力

| 模块 | 能力 |
|---|---|
| 环境 | 检查系统、驱动、Docker、Toolkit、Nginx、OpenSSL；受控准备 Ubuntu 22.04/24.04 缺失依赖 |
| 权重 | 本地导入；公开 Hugging Face 固定 commit 下载，LFS SHA256 校验及本地清单 |
| 模型 | Docker/vLLM + 明确 GPU UUID；本地 llama.cpp CPU 测试后端；运行配置快照 |
| 执行 | 串行任务、阶段、实际日志、失败信息、合作式取消、重启中断标记 |
| 网关 | Nginx TLS、来源网段、工作站 Bearer Token、流式转发；独立启动/停止 |
| 验收 | 模型名、真实文本、SSE、网关鉴权，以及可选图片/两轮工具调用 |
| 发布 | 必须重新通过网关验收，保存最多 20 条发布记录和对应配置，导出客户端材料 |
| 观察 | 控制台 RSS/CPU/GPU 近期采样、引擎 metrics 按需读取、可选 Prometheus 抓取 |

## 本地构建

```bash
cargo build --release --locked
./target/release/phyxi-console
```

打开 `http://127.0.0.1:8765`，填写 `.phyxi-console/token`。默认仅监听回环；状态目录权限 0700，访问令牌 0600。构建需要 Rust stable 和 C 编译器；运行不需要 Rust 工具链。

### 本地小模型验证

使用自备的 llama-server、Nginx 和小 GGUF 文件；测试脚本强制权重小于 200 MiB、2 CPU 线程、512 上下文、单并发，生成请求最多 32 token。不会连接 GPU 服务器或安装主机包。

```bash
python3 scripts/test-small-model.py \
  --llama /absolute/path/llama-server \
  --nginx /absolute/path/nginx \
  --model /absolute/path/small-model.gguf
```

该测试启动真实模型和 TLS 网关，验证文本/SSE/鉴权/发布/导出及配置偏离拒绝，采样各进程 RSS，最后清理测试进程。小模型结果不代表 vLLM、图片或工具能力通过。

如需在页面操作本地 CPU 后端：

```bash
PHYXI_LLAMA_SERVER=/absolute/path/llama-server \
PHYXI_NGINX=/absolute/path/nginx \
./target/release/phyxi-console
```

选择 CPU 测试后端，填 GGUF 路径；GPU 留空、TP=1、序列数=1。模型、Nginx 均为独立受控子进程，不把推理库或权重加载进控制台。测试证书只允许回环地址。

## Ubuntu 服务器起步

服务器可以只有 Ubuntu + 可用显卡驱动/CUDA。先将匹配 Linux 架构的二进制和 `scripts/install-console.sh` 传入服务器：

```bash
sudo bash scripts/install-console.sh /path/to/linux/phyxi-console
```

安装器建立 `phyxi` 服务账号、root 所有的二进制、systemd 控制服务和仅允许 `--prepare-host` 的 sudo 入口；不立即安装 Docker/Toolkit 或部署模型。通过 SSH 隧道访问：

```bash
ssh -N -L 8765:127.0.0.1:8765 用户@服务器
```

前端采用侧边导航、部署列表和配置抽屉。概览显示资源采样，任务与日志单独成页；部署按以下流程执行：

1. **环境准备**：检查缺项，审阅计划，再执行安装。保持驱动/CUDA不变，不格式化磁盘。已有 GPU 工作负载或任何 Docker 容器时拒绝自动配置环境。
2. **模型部署**：准备完整权重或填写公开仓库+固定 commit；填兼容的 vLLM 镜像摘要，选择 GPU 与参数，部署并查看加载日志。
3. **企业接入**：导入 CA 签发证书/私钥/CA，配置域名、IPv4、端口与来源网段；验证证书后启动网关。自签模式仅供本地测试。
4. **验收发布**：选择实际需要的图片/工具能力，执行验收；发布必须经过网关再次验收。成功后在状态目录 `gateway/client-export/` 生成 CA、工作站令牌、endpoint.json 和 OpenCode 2.2 配置模板。

当前非特权网关端口默认 18443；企业可通过网络入口映射到 443。模型 API 始终回环绑定。保持域名、端口与业务模型别名不变，即可在换底层权重时保持客户端入口稳定。

## 配置与运行边界

| 环境变量 | 默认 | 用途 |
|---|---|---|
| `PHYXI_STATE_DIR` | `.phyxi-console` | 状态、模型缓存、证书、任务及发布记录 |
| `PHYXI_PORT` | `8765` | 控制台回环端口 |
| `PHYXI_DOCKER_HOST` | `unix:///var/run/docker.sock` | 明确的本地 Docker socket |
| `PHYXI_LLAMA_SERVER` | 未启用 | CPU 测试引擎绝对路径 |
| `PHYXI_NGINX` | `nginx` | 网关程序路径 |
| `PHYXI_ENABLE_HOST_SETUP` | 未启用 | 设为 1 才开放受控主机准备任务 |

- 保存配置不修改运行实例；模型变更要停止网关、停止模型、重新部署、重新验收。当前有停机窗口，无自动切流或回滚。
- 模型容器必须拥有本状态目录的 owner label；不接管旧 Compose/systemd 或其他平台的实例。CPU/Nginx 只通过当前进程持有的 child handle 停止，不信任磁盘上的 PID。
- 关闭浏览器任务继续。控制台重启：遗留任务标记 interrupted，Docker 模型可能继续运行；本地 CPU/Nginx 子进程不会自动恢复。检查真实状态后重新操作，不自动重放安装/发布。
- 取消在阶段边界生效，不立即打断包管理器/下载。失败保留日志；模型启动失败后需要显式停止清理。
- 下载不执行仓库 Python，不自动启用 trust_remote_code；只支持公开仓库，私有源/镜像源配置暂未提供。
- 日志每任务保留最近 2000 个片段，最多 4096 字符/片段；页面显示最近 30 项任务。任务快照暂未自动清理，近期资源样本限制 120 项。
- 已完成本地真实 CPU 小模型和网关验证。Ubuntu 安装流程、A10/H20/vLLM、企业真实 CA、Windows/OpenCode 实机集成仍待服务器阶段验证。

[架构与职责](docs/architecture.md) · [运维](docs/operations.md) · [开发约定](docs/development.md) · [本地验证记录](docs/local-validation.md)

## 开发检查

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --locked
python3 scripts/smoke-console.py
python3 scripts/test-api-contract.py
```

开发分支 `dev`。每次 commit 必须包含开发纪要。旧代码仅在 `legacy/` 作为迁移参考，Notebook 不再使用。
