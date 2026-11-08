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
 J -->|是| L[网关 + Prometheus]
 L --> M[接口 / 图片 / 两轮工具 / 并发验收]
 M --> N[导出工作站 CA 与 Token]
 N --> O[Windows 安装 OpenCode / 项目连接适配]
 O --> P[项目真实任务与运维验收]
```

```mermaid
flowchart LR
 U[项目 WebUI] --> OC[独立 OpenCode 2.2.0]
 CFG[opencode-gateway/opencode.json] -.-> OC
 CON[connection.json] -.-> U
 OC -->|HTTPS + 工作站 Token| GW[Nginx 企业 IP:443]
 GW -->|后端 Key| V[vLLM model:8000]
 V --> GPU[8 × H20 / TP8 / FP8]
 W[只读模型目录] -.-> V
 P[Prometheus] -->|内部 metrics| V
```

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
