> 历史方案资料：此文描述旧部署流程。Notebook 已移除，新版入口与边界见仓库根目录 README。

# 部署与请求流程

```mermaid
flowchart TB
 A[专用 Ubuntu 24.04 + 已挂载数据卷] --> B[Notebook 配置域名 / IP / 八卡 UUID]
 B --> C[系统依赖 → 驱动 → 必要时重启]
 C --> D[Docker / containerd 数据目录 → NVIDIA Toolkit]
 D --> E[固定镜像摘要 → GLM 架构与依赖检查]
 E --> F[固定 FP8 模型 commit → 分片与 SHA256 检查]
 F --> G[生成 Compose / .env / Nginx / 两层密钥]
 G --> H[生成 CSR → CA 签发 → 证书配对校验]
 H --> I[八卡空闲检查 → CUDA 检查 → systemd 启动]
 I --> J{模型 healthy}
 J -->|否| K[按日志修复；不跳过检查]
 K --> I
 J -->|是| L[网关就绪；检查 Prometheus]
 L --> M[接口 / 图片 / 两轮工具 / 并发验收]
 M --> N[导出工作站 CA 与 Token]
 N --> O[Windows 安装 OpenCode / 项目连接适配]
 O --> P[项目真实任务与运维验收]
```

## 部署架构与 OpenCode 请求链路

```mermaid
flowchart TB
    subgraph ADMIN["① 部署配置与服务管理"]
        NB["部署 Notebook<br/>单元格修改域名、IP、GPU UUID、版本和资源参数"]
        CONFIG["config.local.json<br/>执行配置单元后保存"]
        SCRIPT["scripts/deploy.py<br/>安装环境、检查版本、下载权重、生成配置"]
        RELEASE["/srv/phyxi/releases/发布编号/<br/>compose.yaml · .env · nginx.conf<br/>start-model.sh · prometheus.yml"]
        CURRENT["/srv/phyxi/current<br/>指向当前发布目录"]
        SYSTEMD["phyxi-model.service<br/>通过 Docker Compose 管理三个容器"]
        NB --> CONFIG --> SCRIPT --> RELEASE
        RELEASE --> CURRENT --> SYSTEMD
    end

    subgraph WS["② 企业 Windows 工作站"]
        SETUP["setup-workstation.ps1<br/>安装项目、连接适配和 OpenCode"]
        FILES["%LOCALAPPDATA% / PhyXiEnterprise /<br/>opencode-gateway/opencode.json<br/>opencode-gateway/connection.json<br/>company-ca.pem · cad-ws-01.token"]
        UI["PhyXiCATIAagent WebUI<br/>使用项目连接适配"]
        OC["独立 OpenCode 2.2.0<br/>监听本机 127.0.0.1 动态端口<br/>模型：phyxi-enterprise / glm-5.3-flash"]
        SETUP --> FILES
        FILES -. "connection.json：本机地址与密码" .-> UI
        FILES -. "opencode.json：网关地址与模型配置" .-> OC
        UI <-->|"本机 API · 会话与消息"| OC
    end

    subgraph SERVER["③ 专用推理服务器 · Ubuntu 24.04 · 8×H20"]
        ENTRY["企业模型域名 → 企业内网 IP:443"]
        subgraph DOCKER["Docker Compose · phyxi-enterprise · inference 桥接网络"]
            GW["Nginx 网关容器<br/>容器端口 8443<br/>TLS、来源网段、工作站 Token 校验<br/>连接数限制、流式响应转发"]
            VLLM["vLLM 模型容器 · model:8000<br/>OpenAI 兼容 API · Python 多模态前端<br/>glm47 工具调用与思考解析器"]
            PROM["Prometheus 容器<br/>采集推理指标、保存监控数据"]
        end
        GPU["8×H20 · TP=8<br/>一份 GLM-5.3-Flash 跨八卡推理<br/>原生 FP8 权重 · BF16 KV Cache"]
        MODEL["/srv/phyxi/models/glm53-flash/commit/<br/>固定 commit 的模型权重<br/>只读挂载到容器 /model"]
        CACHE["/srv/phyxi/cache/<br/>编译与运行缓存"]
        SECRET["/srv/phyxi/secrets/<br/>server.key · server-chain.pem · company-ca.pem<br/>client-keys.map · upstream-auth.conf<br/>backend-key · 工作站 Token"]
        MONDATA["/srv/phyxi/prometheus/<br/>持久化监控数据"]
        MONENTRY["服务器 127.0.0.1:9090<br/>管理员通过 SSH 隧道查看"]
        ENTRY -->|"宿主机 443 → 容器 8443"| GW
        GW <-->|"内部 HTTP · 替换为后端 API Key"| VLLM
        VLLM <--> GPU
        MODEL -. "读取权重" .-> VLLM
        CACHE -. "读写缓存" .-> VLLM
        SECRET -. "TLS、工作站认证和后端认证头" .-> GW
        SECRET -. "backend-key" .-> VLLM
        PROM -->|"每 15 秒抓取 /metrics"| VLLM
        PROM --> MONDATA
        MONENTRY --> PROM
    end

    EXPORT["/srv/phyxi/client-export/<br/>受控分发给指定工作站<br/>company-ca.pem · 工作站 Token<br/>endpoint.json 与客户端脚本"]
    SCRIPT -->|"生成密钥、CSR 并导入证书"| SECRET
    SCRIPT -->|"下载并校验"| MODEL
    SYSTEMD -->|"启动与停止"| DOCKER
    SCRIPT -->|"export 阶段生成分发材料"| EXPORT
    SECRET -->|"仅导出 CA 与工作站 Token"| EXPORT
    EXPORT --> FILES
    OC <-->|"HTTPS · 工作站 Token<br/>文字、图片、工具消息与 SSE 响应"| ENTRY

    classDef config fill:#eef2ff,stroke:#6366f1,color:#1e1b4b;
    classDef client fill:#ecfdf5,stroke:#10b981,color:#064e3b;
    classDef service fill:#eff6ff,stroke:#3b82f6,color:#1e3a8a;
    classDef storage fill:#fff7ed,stroke:#f59e0b,color:#78350f;
    class NB,CONFIG,SCRIPT,RELEASE,CURRENT,SYSTEMD config;
    class SETUP,FILES,UI,OC client;
    class ENTRY,GW,VLLM,PROM,GPU,MONENTRY service;
    class MODEL,CACHE,SECRET,MONDATA,EXPORT storage;
```

实线表示配置生成、服务管理或请求路径，虚线表示组件读取的配置、凭据与挂载文件。

- **配置生效**：修改 Notebook 并执行配置单元，再生成发布文件。运行中的部署先停止，变更已启动发布时使用新发布编号，随后启动并验收。
- **请求路径**：WebUI → 本机 OpenCode → Nginx → vLLM → 八张 H20；响应沿原路径返回。网关开放 `/v1/models` 和 `/v1/chat/completions`。
- **两层认证**：工作站 Token 由 `start-client.py` 读入 `PHYXI_GATEWAY_TOKEN` 环境变量，`opencode.json` 引用该变量。Nginx 验证后换成后端 API Key；后端 Key 和 TLS 私钥留在服务器。
- **端口与启动**：企业内网 IP 的 443 映射到网关 8443；模型 8000 不映射宿主机；监控 9090 只绑定服务器回环。网关等待模型健康后启动，Prometheus 没有该启动依赖。
- **工作站接入**：连接适配通过 `PHYXI_OPENCODE_CONNECTION` 读取 `connection.json`；OpenCode 通过 `OPENCODE_CONFIG` 读取自己的配置，使用 `NODE_EXTRA_CA_CERTS` 信任企业 CA。当前启动器默认禁止本地工具执行，业务工具按项目另行启用和验收。
- **目录**：图中 `/srv/phyxi` 为 `data_root` 默认值；首次修改后生成路径同步替换。`commit` 和“发布编号”代表实际模型 commit 与 `release` 值。Docker Engine 使用专用服务器的 `/var/run/docker.sock`，Docker 与 containerd 数据分别存放于数据根目录的 `docker/`、`containerd/`。

服务器目录（默认根目录；首次可在 Notebook 中设置 `data_root`）：

```text
/srv/phyxi/
├── docker/、containerd/        独立于系统盘的容器数据
├── models/glm53-flash/commit/  固定模型权重
├── cache/                     编译缓存
├── prometheus/                监控数据
├── secrets/                   API Key、工作站 Token、TLS 文件
├── artifacts/                 版本、镜像/模型清单、CSR
├── evidence/                  接口与负载验收结果
├── client-export/             仅管理员可读的工作站分发材料
├── releases/发布编号/          Compose、启动脚本、网关配置、.env
└── current -> releases/发布编号/
```
