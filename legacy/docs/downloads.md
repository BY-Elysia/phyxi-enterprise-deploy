> 历史方案资料：此文描述旧部署流程。Notebook 已移除，新版入口与边界见仓库根目录 README。

# 下载源与生效位置

默认使用公共镜像，不需要搭建镜像仓库。Notebook 第 1 节 `DOWNLOADS` 字典直接修改，执行后保存为 `downloads.local.json`（不提交 Git）。下载阶段重新读取此文件；Windows 工作站将该文件放在仓库根目录，安装脚本同样读取。未配置时使用 `deployment/downloads.example.json`。

| 下载内容 | 默认来源 | 生效步骤 |
|---|---|---|
| Jupyter、模型下载工具、工作站 Python 依赖 | 清华 PyPI | bootstrap、model、Windows 安装 |
| Ubuntu 系统包与安全更新 | 清华 Ubuntu | bootstrap、base |
| Docker CE 安装包 | 清华 Docker CE | docker |
| vLLM、Nginx、Prometheus 容器镜像 | DaoCloud 公共 Docker Hub 加速 | docker 配置 Engine，images 拉取 |
| GLM 固定 commit 权重 | HF-Mirror | model |
| OpenCode Windows 包 | npmmirror | Windows 安装 |
| NVIDIA 驱动/Toolkit、Python Windows 安装器、GitHub 项目源码 | 官方源 | 相应安装步骤 |

清华 Docker CE 提供安装包，并非 Docker Hub 镜像。容器加速通过 Engine 的 `registry-mirrors` 生效，镜像名称及 vLLM digest 不变；公共源未缓存、限流或无法提供目标镜像时，Docker 可能尝试官方源。需要排查时查看拉取日志；不取消 digest 校验或换用其他版本。APT 保留签名校验，Python 安装器保留发行者签名校验，OpenCode 保留包 SHA512 校验，模型保持固定 commit 和下载后的文件哈希记录。

NVIDIA 等未确认完整兼容的公共镜像仍使用官方仓库。GitHub 私有部署仓库使用原始认证地址，不能通过公共代理转发凭据；也可从工作站下载后上传。`python_installer_url`、`project_archive_url` 可填写同版本文件的 HTTPS 镜像地址；项目 commit 与安装器版本必须保持不变。

## Notebook 安装前

bootstrap 本身已默认使用清华源。需要临时更换时：

```bash
PIP_INDEX_URL=https://mirrors.tuna.tsinghua.edu.cn/pypi/web/simple \
UBUNTU_MIRROR=https://mirrors.tuna.tsinghua.edu.cn/ubuntu \
  bash scripts/bootstrap-notebook.sh
```

第一次拉仓库前若 Ubuntu 下载慢，在专用 Ubuntu 24.04 服务器上先执行（只改 Ubuntu 官方源，备份原文件）：

```bash
sudo cp -n /etc/apt/sources.list.d/ubuntu.sources /etc/apt/sources.list.d/ubuntu.sources.phyxi-original
sudo sed -E -i 's#https?://([A-Za-z0-9.-]+\.)?archive.ubuntu.com/ubuntu/?#https://mirrors.tuna.tsinghua.edu.cn/ubuntu#g; s#https?://security.ubuntu.com/ubuntu/?#https://mirrors.tuna.tsinghua.edu.cn/ubuntu#g' /etc/apt/sources.list.d/ubuntu.sources
sudo apt-get update
sudo apt-get install -y git ca-certificates
```

以上文件不存在时，按清华说明处理 `/etc/apt/sources.list` 格式。bootstrap 和 base 对两种格式均处理，并保留 `.phyxi-original` 备份。不设置全局 pip/npm 配置。

修改 Notebook 下载源后，重新执行对应步骤。Docker 已安装时，仅在本部署模型停止后重跑 docker 步骤才会更新 Engine 加速地址。不要为换源直接重启运行中的服务。工作站如变更下载源，应复制最新 `downloads.local.json` 再运行安装脚本。bootstrap 发生在 Notebook 之前，其源由上述环境变量控制。

## 镜像站说明

- [清华 PyPI](https://mirrors.tuna.tsinghua.edu.cn/help/pypi/)
- [清华 Ubuntu](https://mirrors.tuna.tsinghua.edu.cn/help/ubuntu/)
- [清华 Docker CE](https://mirrors.tuna.tsinghua.edu.cn/help/docker-ce/)
- [DaoCloud 公共镜像加速](https://github.com/DaoCloud/public-image-mirror)
- [HF-Mirror](https://hf-mirror.com/)
- [npmmirror](https://npmmirror.com/)
