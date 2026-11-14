# Docker 启动控制台

本文先说明回环兼容开发模式，企业部署请直接使用后文“企业四容器模式”。两种 Compose 不混用。

## 本地启动与更新

在仓库根目录执行：

```bash
cp .env.example .env
# 编辑 .env：将 PHYXI_DATA_DIR 设置为现有状态目录的绝对路径
mkdir -p /srv/phyxi/data  # 新安装时使用；已有目录无需创建
# 确认旧的本机控制台已停止，再启动同一数据目录
# macOS 使用 Colima 时先执行 colima start
docker compose up -d --build
docker compose ps
docker compose logs --tail=100 console
```

访问 http://127.0.0.1:8765 。令牌仍保存在状态目录的 `token` 文件中。已有数据库、配置、操作记录、owner 和令牌直接复用，不重建；升级前建议停服务后备份整个目录。`.env` 不提交 Git。

重启：`docker compose restart console`。停止：`docker compose stop console`。容器随 Docker 启动恢复运行（unless-stopped）；macOS 重启后需要启动 Colima。镜像使用多阶段构建，运行镜像只包含编译好的控制台、Docker 客户端、Nginx、OpenSSL、curl 和资源采样工具，不包含 Rust 编译器或模型权重。

## 网络与路径

采用 Docker host 网络，让模型的回环端口、控制台健康检查、网关验收继续使用同一网络空间；控制台仍只监听 127.0.0.1。Linux 支持直接运行；本地 Colima 使用其 Linux 虚拟机，并将端口转发到 Mac。普通 bridge 网络不能直接替换，因为模型监听的是 Docker 主机回环地址。参见 [Docker host 网络说明](https://docs.docker.com/engine/network/drivers/host/)。

状态目录使用宿主机与容器完全一致的绝对路径。Docker socket 操作的是宿主机 daemon，下载到状态目录的权重才能按同一路径挂载给模型容器。使用其他模型目录或企业证书目录时，在 compose volumes 中增加同路径 bind mount，已存在的配置路径不会自动改写。不要把不兼容的 Mac 二进制挂到 Linux 容器运行。

## 当前边界

- 挂载 Docker socket 供控制台管理自己的模型容器；具有该 socket 的容器拥有宿主机 Docker 管理权限，控制台访问仍需令牌。
- 容器禁用宿主机依赖安装；Docker、显卡驱动、NVIDIA Toolkit 应提前安装在服务器。不能在控制台容器内安装这些组件来替代宿主机安装。
- 基础 compose 供本地控制台及配置管理使用，不暴露 GPU。NVIDIA 服务器需按实际环境为控制台注入 GPU 的 utility 能力（nvidia-smi / NVML），以支持 UUID 发现和占用检查；未完成 GPU 实机验证。
- 页面资源读数对应 Linux 宿主机或 Colima 虚拟机，依赖版本对应控制台容器，不代表 Mac 原生环境。
- 镜像不内置 llama.cpp；原生 Mac CPU 测试配置会保留，但要在容器内执行，需要另行准备 Linux llama-server 及可访问的 GGUF。
- 不会启动 GPUStack、Xinference、Semaphore，也不会拉取模型推理镜像或下载权重。

## P1a：独立 Nginx 过渡模式

独立网关配置覆盖文件为 `deployment/compose.gateway.yaml`。仅用于本地迁移验证，仍使用 host 网络及旧鉴权；不是已完成的企业网络/工作区安全方案。

新状态目录先执行：

```bash
python3 scripts/prepare-gateway-container.py --data-dir /absolute/state/path
docker compose -f compose.yaml -f deployment/compose.gateway.yaml up -d --build
```

已有 `gateway/nginx.conf` 时初始化脚本拒绝覆盖。迁移前先确认旧网关已停止并备份配置，再人工移走旧 nginx.conf，执行初始化；不要覆盖运行配置。网关容器只挂载 gateway 子目录，不挂载 Docker socket、控制台数据库或模型权重。控制台通过标签和完全一致的目录挂载核对归属，校验后重载配置，读取运行配置摘要确认加载成功；控制台对象/进程重建后从持久化快照和实际摘要恢复状态。停止网关只停用业务监听，Nginx 容器保留诊断监听 127.0.0.1:18765。

真实容器验证：`python3 scripts/test-gateway-container.py`。该脚本使用独立临时目录、唯一容器名，检查 TLS 匿名拒绝、控制对象重建恢复、无效配置保留原服务、错误挂载归属拒绝及停止，只清理自身创建的容器。不验证 GPU 推理或工作区独立密钥。

## P2a：管理员密码初始化

企业模式已提供 HTTPS 管理入口；不要把本地兼容接口直接发布到内网。

先停止控制台，对同一状态目录执行 `scripts/set-admin-password.py --data-dir /absolute/state/path --binary /absolute/path/phyxi-console`。脚本用隐藏输入读取密码，经 stdin 传入，不放入命令参数。容器部署可在停止 console 后使用 `docker compose run --rm -T console --set-admin` 从安全的 stdin 输入密码（不把明文写进命令或提交文件）。必须使用 Linux 容器二进制或当前主机兼容的二进制。密码至少 12 字节；admin.hash 仅存 Argon2id 哈希。服务锁防止运行中重置；重启使内存会话失效。

存在 admin.hash 时前端自动切换为管理员密码登录，旧 Bearer Token 被拒绝。正式会话 Cookie 强制 Secure、HttpOnly、SameSite=Strict，有效期 8 小时；写操作需要 CSRF token。开发回环 HTTP 调试可显式设置 `PHYXI_LOCAL_HTTP_LOGIN=1`，仅限本地环境，不能作为企业 HTTPS 的替代。登录限速为全管理员入口每分钟 5 次、最多 32 个有效会话；无多账号或 SSO。

网关对象增加“上传企业证书”：三个 PEM 文件各最多 64 KiB，校验通过后保存为草稿，私钥仅保存在权限受限目录。保存不自动改变运行网关；独立容器模式可点击“在线应用网关配置”，原生测试使用准备/启动流程。管理员审计在“操作记录”展开查看，最多保存 5000 条、返回最新 200 条，记录账号/动作/状态/时间，以及企业网关覆盖的可信来源 IP。

## P1b/P3：企业四容器模式

独立部署文件为 `deployment/compose.enterprise.yaml`，不能与旧 host 网络覆盖文件混用。实际运行三个固定服务；部署模型时由控制台创建第四个模型容器。模型不发布宿主机端口。管理入口为 HTTPS 8443，业务入口为 HTTPS 18443；后端其他端口不发布。

在新的状态目录执行 `scripts/prepare-enterprise.py --data-dir /absolute/state --binary /absolute/phyxi-console --domain console.company.example --cert /path/chain.pem --key /path/key.pem --ca /path/ca.pem`，交互设置密码，脚本验证证书并创建权限受限的服务间密钥、网关启动配置和 enterprise.env。随后执行脚本打印的 Compose 命令。已有状态目录拒绝覆盖，需先备份并迁移；不要直接覆盖现有部署。生产服务器须允许内网访问发布端口，并提前准备 Docker/驱动/Toolkit；GPU utility 注入仍需目标机验证。

仅本地联调使用 `--local-test` 生成 7 天 localhost 测试证书，默认仅发布到 127.0.0.1。客户端应信任导出的 admin-ca.pem，不关闭 TLS 校验。不要修改本地测试模式的发布地址来冒充正式企业入口。Mac/Colima 与宿主机需保持时间同步。企业容器模式暂不支持原生 CPU 后端；CPU 小模型验证使用旧原生测试路径。

工作区先“应用接入配置”，再生成 API Key；密钥只展示一次，关闭后只能重置。更换网关会撤销旧密钥，停用也会撤销；启用不会复活旧密钥。服务端只存摘要，鉴权服务串行写入并持久化后确认成功，独占锁防止同一数据库双实例写入。鉴权成功不缓存，Nginx 子请求不带正文，故障在短超时后拒绝新请求。临时验收凭据使用 system- 保留命名空间、短期有效并在请求结束后撤销。

网关请求日志为结构化元数据，写入 Docker 日志（10 MiB × 3 上限），管理页按工作区过滤最近 200 行；不提供长期日志平台。管理审计从可信网关覆盖的来源头读取规范 IP，不信任请求体中的来源信息。控制台、鉴权服务互不共写数据库。独立网关不挂载 Docker socket，模型和鉴权也不挂载。

联调脚本：`python3 scripts/test-access-service.py` 与 `python3 scripts/test-enterprise-containers.py`。后者使用真实四个容器，但模型端是 Python 协议 fixture；它只证明接入、撤销、流式连接、故障和重启边界，不代表 vLLM/SGLang GPU 推理成功。运行时禁止已有同名企业部署或模型容器；结束只清理该脚本创建的部署。

## 重新部署与证书维护

在部署实例选择权重、运行环境、推理参数、服务模型名和 GPU UUID，点击“检查部署 / 查看差异”。运行卡片只显示实际快照；保存草稿不会改变实例。“重新部署”确认后停止原关联网关和所属模型，以新快照启动，再恢复关联网关。失败时查看任务阶段，修正或重新选择旧配置后部署；第一版没有自动回滚。变更后重新执行接口验收。

业务证书在网关页上传并校验，保存后点击“在线应用网关配置”。候选证书存独立目录，Nginx 配置先检查后重载并确认实际摘要；校验失败保留原运行入口。成功版本的证书目录暂不自动清理，避免误删仍被旧连接引用的材料。管理入口证书 admin.pem/admin.key/admin-ca.pem 不由该按钮修改，需要在维护窗口由运维验证并更新后重启 gateway。

## 手动加密备份与恢复

使用与服务器平台匹配的独立 PhyXi 二进制和 `scripts/backup-console.py`。口令隐藏输入，经 stdin 传入，不写到 argv 或项目文件。备份口令至少 12 字节，须独立保管；丢失无法恢复。格式使用 Argon2id 派生密钥和 XChaCha20-Poly1305 认证加密。

1. 保存企业 Compose 环境文件（包含路径和监听地址，不含登录密码），确认原状态目录绝对路径。停止 console 与 access，避免数据库并发修改。此时新的业务请求会失败，已有推理流可结束；安排维护窗口。
2. 执行下列备份命令。CLI 同时检查控制台和鉴权数据库锁，以 SQLite backup API 创建一致性副本。成功后重新启动 access、console；不需要重新部署模型。
3. 恢复前先从平台停止所属模型和业务网关，再停止整套企业 Compose；只处理本项目的容器。移走原状态目录保留为回退副本，恢复目标必须不存在，且必须是备份中的原绝对路径。
4. 执行恢复命令。错误口令、篡改、非白名单文件、路径不符或已有目标目录均拒绝，不覆盖原数据。
5. 恢复不包含权重、登录会话、请求日志和正在执行的进程；重新准备原路径权重。沿用保存的 Compose 环境文件启动基础服务，重新登录并检查资源，显式部署模型、启动业务网关并验收。

```bash
# 先通过企业 Compose 停止 console 和 access
python3 scripts/backup-console.py backup \
  --data-dir /srv/phyxi/data --file /srv/phyxi-backups/config.phyxi \
  --binary /opt/phyxi-console/phyxi-console

# 已停止所属模型与整套服务，移走旧 data 目录，恢复原路径
python3 scripts/backup-console.py restore \
  --data-dir /srv/phyxi/data --file /srv/phyxi-backups/config.phyxi \
  --binary /opt/phyxi-console/phyxi-console
```

备份最大 64 MiB，包含控制台与鉴权数据库、密码哈希、工作区密钥摘要、证书和内部服务凭据。恢复保留历史快照，但清除活动网关标记，Nginx 回到仅管理入口的初始配置，避免使用备份时的业务路由或不存在的证书版本。恢复工作区密钥状态也会回到备份时间点：恢复后在重新开放业务前检查并撤销备份之后已作废的密钥，必要时全部重置。第一版不支持跨路径自动迁移、在线恢复或备份权重。
