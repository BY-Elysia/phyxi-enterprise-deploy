# 首次只安装控制台，后续通过页面部署

## 部署顺序

默认使用[预构建发布包](prebuilt-release.md)，服务器不现场编译。

首次安装只运行 PhyXi 控制台一个应用容器，不启动 Nginx、鉴权服务或模型，不要求准备域名与企业证书。后续企业网关、HTTPS 管理入口、证书和客户端工作区仍然保留，通过控制台显式设置和启用。

Docker 是运行控制台容器的前提。GPU 驱动和 NVIDIA Container Toolkit 属于宿主机基础设施，不在容器内部安装替代品；首次安装前准备，页面会检测其状态。本次不新增任意宿主机命令执行接口。

```bash
# 在项目根目录，先确认 Docker 已安装并运行
python3 scripts/bootstrap-console.py --data-dir /srv/phyxi/data --bundle /opt/phyxi-release
# 脚本交互读取管理员密码，不写入命令参数或明文配置
```

NVIDIA 服务器在已安装驱动和 NVIDIA Container Toolkit 后，初始化命令加 `--gpu`；这只向控制台提供 GPU 检测所需的 utility 能力，不启动推理容器。更新时同样追加 `-f deployment/compose.gpu-utility.yaml`。本地无 NVIDIA 的测试环境省略。

脚本仅用于全新状态目录，已有目录拒绝初始化。管理员固定 admin。控制台只监听服务器 127.0.0.1:8765，初始访问通过加密 SSH 隧道：

```bash
ssh -N -L 8765:127.0.0.1:8765 root@SERVER
# 本机浏览器访问 http://127.0.0.1:8765
```

该模式文件为 `deployment/compose.console-only.yaml`，不要与企业四容器 Compose 同时使用同一目录。控制台有 Docker socket 权限，是管理信任边界；只有运维应能登录。

## 页面流程

1. **服务器资源与依赖 → 企业接入组件**：点击“部署 / 恢复接入组件”。控制台检查并在缺失时下载 `nginx:1.28-bookworm`，复用本次安装的控制台镜像启动独立鉴权服务。创建两个受当前 owner 和目录约束的容器，等待健康检查；任务步骤、下载日志和错误可在操作记录查看。重复执行恢复现有容器，不重新创建数据库。
2. **模型权重 / 推理引擎镜像 / 推理参数**：分别准备，部署实例组合后启动。首次不自动拉取 vLLM 或 SGLang，也不自动下载模型。
3. **企业网关**：创建域名、监听 IP、允许来源和证书配置；上传企业证书。业务入口绑定已运行的部署，再显式启动。与“组件容器已运行”不同，此时才开放模型 HTTPS 接口。
4. **客户端工作区**：绑定网关，应用接入配置，再生成独立 API Key。撤销/停用继续即时拒绝新请求，已开始请求允许完成。
5. **企业网关 → 启用企业管理入口**：无需关联运行模型；使用此配置的域名、监听 IP、允许来源及经校验的企业证书，在固定 8443 端口开放管理员控制台。初始 SSH 入口保留。管理入口必须使用企业证书，自签联调继续使用 SSH。管理员响应 Cookie 由 Nginx 强制设置 Secure、HttpOnly、SameSite=Strict。
6. “关闭企业管理入口”仅关闭管理 HTTPS；“停止网关”停业务监听；“停止接入组件”停两个支撑容器，需先停止业务网关，也会中断企业管理入口，SSH 控制台仍可用。

管理入口与业务入口应使用不同端口。管理入口设置会立即使用所选网关草稿的域名/来源/证书，后续更改草稿需再次显式应用。首次版本只提供一个企业管理入口，后一次应用替换前一次；更换证书先校验，Nginx 配置检查失败恢复旧文件。

## 网络与持久化

此模式使用 Linux Docker host 网络；控制台、鉴权服务和 Nginx 健康端口分别回环监听 8765、8766、18765，模型接口也仅发布回环地址。Nginx 企业业务/管理监听由用户配置。该模式不等于旧四容器模式的 bridge 网络隔离，宿主机上的受信任运维可访问回环接口。工作区鉴权仍在独立进程内执行，控制台退出不影响已部署的业务服务。

控制台持有整个状态目录；Nginx 仅挂载 gateway 子目录，鉴权服务仅挂载 access 目录及只读内部密钥，两者均无 Docker socket。容器设置自动重启和有界日志。默认镜像版本由产品提供；组件升级/替换流程本期不自动实现，重试只恢复原容器。

## 更新与验证

```bash
docker compose --env-file /srv/phyxi/data/console.env -f deployment/compose.console-only.yaml up -d --no-build --pull never --wait console
```

更新控制台不会停止模型或接入组件，不自动接管旧外部 Compose 的容器。升级前按备份文档备份状态。已有四容器安装继续使用原部署方式，不能直接混用迁移。

真实 Docker 集成测试（先构建测试镜像）：

```bash
docker build -t phyxi-console:managed-test .
python3 scripts/test-managed-services.py
```

测试拒绝任何已存在的 `phyxi-managed-access`、`phyxi-managed-gateway`、`phyxi-test-managed-console`，只清理本次创建的容器 ID。验证首次无组件、按需部署、幂等恢复、HTTPS 管理登录、停止/重启和外部同名容器拒绝；不启动推理模型，不声称 GPU 推理验证通过。
