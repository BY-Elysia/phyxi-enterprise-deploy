> 企业版架构及分阶段验收：[完整架构与开发方案](docs/enterprise-plan.md)。P0–P4 已完成本地实现；GPU 推理仍待服务器验收。

# PhyXi 轻量部署控制台

一个控制台完成 **环境检查/准备 → 权重准备 → 模型部署 → TLS 网关 → 接口验收 → 发布记录与客户端导出**。面向一台 Ubuntu NVIDIA 服务器、一套业务模型服务；本地开发可用 CPU 小模型验证流程。

融合 GPUStack 的资源/实例管理、Xinference 的引擎和模型配置、Semaphore 的受控任务/日志职责，不部署或依赖这三个平台。当前实现为 Rust API + 独立任务模块 + SQLite + 内嵌 HTML/CSS/少量原生 JS。技术选择以实际资源占用为准，不为语言本身引入复杂性。无需 Node、TS、WASM、Redis、外部数据库；Prometheus/Grafana 可选。

## 当前能力

| 模块 | 能力 |
|---|---|
| 环境 | 检查系统、驱动、Docker、Toolkit、Nginx、OpenSSL；受控准备 Ubuntu 22.04/24.04 缺失依赖 |
| 权重 | 本地导入；公开 Hugging Face 固定 commit 下载，LFS SHA256 校验及本地清单 |
| 模型 | Docker/vLLM 或 SGLang + 明确 GPU UUID；本地 llama.cpp CPU 测试后端；运行配置快照 |
| 安全与恢复 | 管理员密码/会话/CSRF、工作区独立密钥、元数据审计、离线加密备份恢复 |
| 执行 | 串行任务、阶段、实际日志、失败信息、合作式取消、重启中断标记 |
| 网关 | 独立 Nginx、TLS、工作区 API Key、流式转发、证书在线应用 |
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

前端以独立配置库和部署对象为入口，采用侧边导航、列表与配置抽屉：

1. **部署实例**：先创建具名对象，也可暂不选择配置；执行时引用模型、镜像、推理参数，并分配 GPU UUID 和端口。允许保存多个方案，但同时仅运行一个实例。
2. **模型权重 / 推理引擎镜像 / 推理参数**：分别创建、修改、复用。权重下载属于模型配置，镜像拉取属于镜像配置；vLLM 配置不包含企业 CA。
3. **企业网关**：独立保存域名、证书、监听地址和访问规则；选择部署作为后端，准备证书后启动。模型直连验收从部署页执行，网关验收/发布从网关页执行。
4. **工作区**：客户端接入配置，选择网关并指定凭据环境变量名；导出 OpenCode 配置模板。模板不包含真实令牌，不代表已通过发布验收。真实 CA、工作站令牌等发布材料仍在状态目录 `gateway/client-export/`。
5. **服务器资源与依赖**：以中文指标卡片查看系统/依赖版本、GPU、内存、磁盘和负载，原始诊断默认折叠。Mac 内存标注估算，系统负载与 CPU 使用率分开解释。安装缺失依赖放在折叠区域，Mac 上明确标记 NVIDIA 依赖不适用。
6. **操作记录**：执行进度直接显示在当前配置页面；历史页提供对象归属、最近日志及提交时的配置快照。

首次升级自动将旧配置迁移到独立记录，空白状态目录不创建虚假配置。配置使用修订号防止覆盖其他标签页的更新，被引用的记录不能删除。每次启动固定完整组合快照；修改共享配置不修改运行实例。网关使用已运行实例的真实快照，不把后续草稿写成已发布配置。详情见 [独立配置与组合 API](docs/composition.md)。

当前非特权网关端口默认 18443；企业可通过网络入口映射到 443。企业模式模型 API 只在内部 Docker 网络开放，原生测试回环绑定。保持域名、端口与业务模型别名不变，即可在换底层权重时保持客户端入口稳定。

## 配置与运行边界

| 环境变量 | 默认 | 用途 |
|---|---|---|
| `PHYXI_STATE_DIR` | `.phyxi-console` | 状态、模型缓存、证书、任务及发布记录 |
| `PHYXI_PORT` | `8765` | 控制台回环端口 |
| `PHYXI_DOCKER_HOST` | `unix:///var/run/docker.sock` | 明确的本地 Docker socket |
| `PHYXI_LLAMA_SERVER` | 未启用 | CPU 测试引擎绝对路径 |
| `PHYXI_NGINX` | `nginx` | 网关程序路径 |
| `PHYXI_ENABLE_HOST_SETUP` | 未启用 | 设为 1 才开放受控主机准备任务 |
| `PHYXI_HF_ENDPOINT` | `https://huggingface.co` | 可显式设为 `https://hf-mirror.com`，保持固定 revision 与 LFS SHA256 校验 |

- 保存配置不修改运行实例；模型变更通过“重新部署”有序停止关联网关和旧模型、启动新模型并恢复网关，之后重新验收。当前有停机窗口，无自动切流或回滚。
- 模型容器必须拥有本状态目录的 owner label；不接管旧 Compose/systemd 或其他平台的实例。CPU/Nginx 只通过当前进程持有的 child handle 停止，不信任磁盘上的 PID。
- 关闭浏览器任务继续。控制台重启：遗留任务标记 interrupted，Docker 模型可能继续运行；本地 CPU/Nginx 子进程不会自动恢复。检查真实状态后重新操作，不自动重放安装/发布。
- 取消在阶段边界生效，不立即打断包管理器/下载。失败保留日志；模型启动失败后需要显式停止清理。
- 下载不执行仓库 Python，不自动启用 trust_remote_code；只支持公开仓库，私有源认证暂未提供。模型仓库可填 owner/model 或官方/国内镜像站仓库地址；下载站点限定 Hugging Face 和 hf-mirror，不接受任意站点。
- 日志每任务保留最近 2000 个片段，最多 4096 字符/片段；页面显示最近 30 项任务。任务快照暂未自动清理，近期资源样本限制 120 项。
- 已完成本地真实 CPU 小模型和网关验证；Ubuntu 24.04 上的服务安装、已有 Docker/Toolkit 环境准备、证书生成及 A10 CUDA 基础计算已验证。GPU/vLLM 推理、从零安装 Docker/Toolkit、企业真实 CA、Windows/OpenCode 实机集成仍待验证，详见 [服务器验证记录](docs/server-validation.md)。

[架构与职责](docs/architecture.md) · [运维](docs/operations.md) · [开发约定](docs/development.md) · [本地验证记录](docs/local-validation.md)

## 开发检查

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --locked
python3 scripts/smoke-console.py
python3 scripts/test-api-contract.py
python3 scripts/test-composition.py
```

开发分支 `dev`。每次 commit 必须包含开发纪要，通过检查后直接推送到 `origin/dev`。旧代码仅在 `legacy/` 作为迁移参考，Notebook 不再使用。

### 先准备资源，再组合部署

推理引擎镜像预设提供 vLLM / SGLang 的 Docker 地址，也可填写自定义地址。保存后点击「下载推理引擎镜像」，或自行在部署服务器执行 `docker pull`。配置允许标签；执行快照固定为本地镜像 ID。

模型权重预设提供 Qwen3-0.6B、Qwen3-8B、GLM-4-9B-0414 的仓库地址，也可自定义仓库或填写已准备好的服务器路径。保存来源时仅查询版本元数据，将 main/tag 固定为 commit；点击「下载权重」才会下载文件。预设不会预装任何模型。

部署前先确认两类资源均已准备，选择匹配的推理引擎镜像与推理参数，再组合启动。启动不会自动拉取镜像或下载权重。SGLang 已通过参数及生命周期协议测试，实际 NVIDIA 推理兼容性仍需服务器验证。

### Docker 启动

已提供根目录 `Dockerfile` 与 `compose.yaml`。在 `.env` 设置绝对路径 `PHYXI_DATA_DIR` 后执行 `docker compose up -d --build`，访问 http://127.0.0.1:8765 。支持复用已有状态目录；运行镜像不包含 Rust 工具链或模型。网络、目录挂载和 GPU/CPU 边界见 [Docker 启动说明](docs/docker.md)。

企业部署使用独立的 `deployment/compose.enterprise.yaml`：console、access、gateway 三个固定容器，加一个按需模型容器；管理员 HTTPS 密码登录。根目录 Compose 保留为回环兼容开发模式。初始化、维护和加密备份详见 [Docker 运维说明](docs/docker.md)。
