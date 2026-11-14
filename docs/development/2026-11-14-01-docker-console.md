# Docker 启动 PhyXi 控制台

## 目标

将本地控制台改为 Docker Compose 启动，保留配置、数据库与身份文件，保持轻量运行与单实例归属。

## 变更

- 新增多阶段 Dockerfile；Rust slim 构建，Debian slim 运行，只复制控制台与 Docker CLI，不运行容器内 Docker daemon、不打包编译器或模型。
- Compose 使用 host 网络保留回环推理/网关接口，状态目录按相同绝对路径挂载，复用宿主机 Docker socket；增加 unless-stopped、健康检查、日志轮转与 SIGINT 优雅停止。
- .dockerignore 采用白名单，排除本地数据库、令牌、权重、测试文件及 Git；.env.example 提供路径模板，实际 .env 不提交。
- 服务器环境页面标明 Docker 模式的资源/依赖采样范围。容器关闭宿主机安装功能。
- 增加 Docker 使用文档，说明目录、网络、Linux 二进制与 GPU utility 能力边界。

## 验证

- cargo fmt --all -- --check、cargo clippy --workspace --all-targets -- -D warnings 通过。
- cargo test --workspace：22 项通过；debug 构建、smoke-console.py、test-api-contract.py 与 JavaScript 语法检查通过。
- docker compose config --quiet、build、up -d --wait、restart 后健康检查通过。Mac 回环地址 HTTP 200、未授权 API 401；原配置库、token、owner 与启动前校验完全一致，重启后再次核对一致。容器可连接宿主机 Docker，环境 API 标明 containerized 且 host_setup_enabled=false。
- 本机 Colima 启动前发现遗留 vz.pid 指向无关的 ModelCatalogAgent；仅重命名已确认过期的 pid 文件后恢复启动，没有向该进程发信号或删除 Docker 数据。
- 现有 SQLite 数据库已备份到 .local-test/before-docker-20261114.db；未提交本地数据和身份凭据。

## 限制

- 本次是本地 Linux ARM64 容器控制台验证，不代表 NVIDIA GPU 推理已通过。服务器需预装驱动/Toolkit，并向控制台注入 utility 能力以发现 GPU。
- Docker 模式不能替宿主机安装 Docker、驱动或 Toolkit；界面中的系统读数来自 Linux 宿主机/虚拟机，依赖版本来自容器。
- 不内置 Linux llama-server，原 Mac CPU 测试配置保留但不自动转换为可运行的 Linux 配置；外部模型/证书目录需另行同路径挂载。
- Docker socket 提供宿主机 Docker 管理能力，API 仍有令牌鉴权且只监听回环地址。
