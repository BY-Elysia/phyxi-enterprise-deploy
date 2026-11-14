# 首次单控制台安装与按需企业接入

## 目标

首次安装不要求 Nginx、鉴权服务、模型或企业证书；这些能力保留，在控制台中显式配置和部署。服务器部署保持暂停，本次仅代码和本地验证。

## 变更

- 新增单控制台 Compose 和交互初始化脚本，要求管理员初始化、回环监听；GPU utility 注入作为可选覆盖文件。保留已有四容器维护方式，不自动接管。
- 新增受任务串行锁约束的接入组件部署、恢复、停止操作。Nginx 默认镜像缺失时下载，鉴权复用已安装控制台镜像；组件具有 owner 标签、目录核验、自动重启与有界日志，无 Docker socket。
- 服务器页面增加组件状态、任务反馈和组件日志；已有模型镜像/权重/部署、证书与工作区流程继续使用。
- 企业 HTTPS 管理入口由网关配置显式启用/关闭，使用企业证书、域名、监听地址和来源规则；不依赖模型运行。Nginx 强制管理员 Cookie 安全属性，重载后校验 TLS 与配置代次，失败恢复旧文件和配置。
- 未部署或停止的受管网关视为未运行，不阻塞模型操作。预留管理/鉴权/健康端口避免与模型和业务监听冲突。
- 新增安装说明与真实容器集成测试，更新 README 和架构文档入口。

## 验证

- cargo fmt --all -- --check、cargo clippy --workspace --all-targets -- -D warnings、cargo test --workspace、cargo build --workspace。
- scripts/smoke-console.py、scripts/test-api-contract.py、scripts/test-admin-security.py、node --check console/web/app.js。
- scripts/test-managed-services.py 使用隔离状态目录及真实 Docker：首次无组件、按需创建、重复恢复不替换容器、HTTPS 管理登录 Secure Cookie、失败重载恢复、停止/恢复、控制台重启后归属恢复、同名外部容器拒绝操作。
- 界面使用独立本地状态目录检查组件入口和未部署状态，不更改已有本地部署配置。

## 限制

新模式使用 Linux host 网络与回环端口；不是旧企业模式的 bridge 网络隔离。宿主机 Docker 是首次安装前提，GPU 驱动/Toolkit 仍由宿主机准备；未增加任意宿主机命令执行能力。接入组件镜像版本使用产品默认值，升级/替换流程不自动执行。只支持一个企业管理入口（8443），从所选网关草稿应用域名/来源/证书；SSH 入口始终保留。未做真实 GPU 推理或新服务器部署验证。
