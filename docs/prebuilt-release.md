# 预构建控制台发布与快速部署

## 默认路径：服务器不编译

`Console release package` 工作流在 dev 的相关修改推送后自动运行，也支持手动触发。它先执行 Rust、接口和管理员安全检查，再在 GitHub 原生 AMD64 runner 构建镜像，生成按完整提交 SHA 命名的 Actions 安装包。沿用仓库访问权限，不创建公开镜像仓库或自动部署到服务器。

安装包包含压缩镜像、manifest.json（源码提交、镜像 ID、架构、SHA256）、SHA256SUMS、安装脚本与 Compose 文件。不含编译工具链、数据库、证书或密码。Actions 产物保存 30 天；长期版本应由运维归档下载包。本期提供 Actions 构建产物，不自动创建 GitHub Release 标签。

本机先下载并传入服务器（RUN_ID 使用成功工作流的运行编号）：

```bash
gh run download RUN_ID --repo BY-Elysia/phyxi-enterprise-deploy \
  --name phyxi-console-linux-amd64-COMMIT_SHA --dir console-release
scp -r console-release root@SERVER:/opt/phyxi-release
```

服务器无需 Rust、Git 或访问 Docker Hub：

```bash
cd /opt/phyxi-release
python3 scripts/bootstrap-console.py --data-dir /srv/phyxi/data --bundle . --gpu
```

脚本先校验镜像包 SHA256 和 Docker 主机架构，导入后核对实际镜像 ID，再交互初始化管理员。仅启动控制台，使用 `--no-build --pull never`；镜像下载/校验失败不会创建未完成的数据目录。Docker、驱动和 Toolkit 仍是宿主机基础条件。非 NVIDIA 环境省略 --gpu。

也可使用已有镜像仓库（建议不可变提交标签或 digest）：

```bash
python3 scripts/bootstrap-console.py --data-dir /srv/phyxi/data \
  --image REGISTRY/phyxi-console:COMMIT_SHA --gpu
```

本地存在该镜像则直接复用，否则仅拉取一次；实际运行固定为解析到的镜像 ID，不在重启时追踪可变标签。初始化后的 `/srv/phyxi/data/console.env` 记录数据目录和固定镜像 ID，后续命令使用这个环境文件。

## 更新已有控制台

不要对已有目录再次运行初始化脚本。先验证新包并获取其固定镜像 ID：

```bash
python3 scripts/console-image.py --bundle /opt/phyxi-release
```

导入命令不会修改容器或凭据。按维护流程备份后，将 `console.env` 中 PHYXI_CONTROL_IMAGE 改为验证得到的 ID，并显式启动新版本；失败时恢复旧 ID。保留旧镜像，不自动清理：

```bash
docker compose --env-file /srv/phyxi/data/console.env \
  -f deployment/compose.console-only.yaml -f deployment/compose.gpu-utility.yaml \
  up -d --no-build --pull never --wait console
```

此更新仅操作控制台；不自动升级接入组件或模型。首次部署的旧版本若使用项目 .env，需要显式迁移到记录镜像 ID 的 console.env，不能假定文件已经存在。

## 构建优化与边界

CI 使用原生 AMD64 构建，避免 Mac ARM 模拟编译；Dockerfile 将依赖编译与业务源码分层，前端修改不重复编译全部依赖。CI 通过 BuildKit GHA 缓存复用镜像层，Rust 校验也有依赖缓存。镜像导出使用低压缩级别，Actions 对已压缩文件不再次压缩。

构建缓存参考 [Docker 官方说明](https://docs.docker.com/build/cache/backends/gha/)。CI 仍需网络和首次构建时间；优化是把这些工作移到发布前，不保证所有网络环境的固定部署秒数。SHA256 用于完整性检查，不替代可信产物来源；下载应选择本仓库成功的工作流。

源码现场构建仍可显式使用 --build，仅供开发或应急，不是服务器默认安装路径。
