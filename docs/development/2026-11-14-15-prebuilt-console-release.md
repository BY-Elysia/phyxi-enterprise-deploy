# 控制台预构建发布与部署提速

## 目标

避免服务器现场下载 Rust 工具链和跨架构模拟编译。把构建移到发布阶段，部署只校验、导入和启动，保持首次仅一个控制台容器。

## 变更

- 新增 dev 自动触发的 AMD64 发布工作流：原生 Ubuntu 构建、Rust/接口/安全校验、BuildKit 与 Rust 缓存、按完整提交 SHA 保存离线安装包。产物沿用仓库访问规则，保留 30 天；不自动部署服务器或创建发布标签。
- Dockerfile 将依赖编译独立成缓存层，业务和前端修改只重编译应用。
- 增加镜像导出、SHA256/架构校验和离线导入脚本；同时记录 containerd manifest ID 与经典 Docker config ID，兼容两类镜像存储。
- 初始化默认要求选择预构建镜像或离线包；源码构建仅显式 --build。镜像准备成功后才创建状态，运行固定 ID 并禁止隐式构建/拉取，生成后续使用的 console.env。
- 更新安装说明与发布文档，新增损坏包、错误架构、镜像复用和失败无残留测试。

## 验证

- cargo fmt --all -- --check、cargo clippy --workspace --all-targets -- -D warnings、cargo test --workspace、cargo build --workspace、scripts/smoke-console.py、scripts/test-api-contract.py 通过。
- scripts/test-console-release.py 通过：错误架构/损坏包拒绝、已有镜像固定 ID、安装不构建不拉取、镜像失败不创建状态目录。
- 本地真实 Docker 新 Dockerfile 构建通过，安装包导出/完整性校验与实际二进制管理员初始化通过。
- 推送后运行 GitHub 首次原生 AMD64 发布任务，远端执行结果以工作流记录为准。

## 限制

首次 CI 构建仍需下载和编译，缓存降低后续成本；不保证固定网络耗时。默认交付 Actions 离线产物，不依赖新增镜像仓库。SHA256 是完整性校验，不代替可信来源。没有自动升级运行中的服务器或删除旧镜像、数据和组件。
