# Docker 启动控制台

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
