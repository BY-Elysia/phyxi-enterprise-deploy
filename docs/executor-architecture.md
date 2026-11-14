# 单机控制台与宿主机执行器

## 范围与分层
一台 Ubuntu 22.04/24.04 GPU 服务器，一个运行模型。不是两台服务器，不引入 Kubernetes、调度集群、Redis 或独立数据库服务。

- 控制台容器：静态前端、管理员密码登录、CSRF、经过鉴权的产品 API 转发。非 root（10001:10001）、只读根文件系统，无 Docker socket、GPU、宿主机根目录、模型权重或执行器数据库挂载。
- 宿主机执行器：同一 Rust 发布二进制以 `PHYXI_ROLE=executor` 运行，systemd 托管，只监听 `/run/phyxi/executor.sock`。接收有类型的配置和任务 API，负责检查、下载、容器管理、固定宿主机操作及持久化记录。
- 推理接入：一个 vLLM/SGLang 容器、一个 Nginx 容器、一个工作区鉴权容器。首次不启动这些容器。业务请求不经控制台或执行器。

完整部署为四个容器加一个宿主机执行器；首次只有控制台容器和执行器。SQLite 嵌入应用，无数据库容器。

## 数据与权限
部署配置、组合快照、任务和发布记录保留在执行器的 `/srv/phyxi/data/state.db`，由执行器唯一持有。控制台通过产品配置 API 保存配置，不再打开同一份数据库，避免前端重启误把执行任务标记为中断。控制台自身的登录数据在 `/srv/phyxi/console-auth`，与宿主机状态隔离。旧密码首次迁移时保留；之后改密码使用 console-auth 目录。

Unix socket 权限 0660，目录 0750，由 root:phyxi-console 管理。内部服务凭据只在宿主机和控制台只读挂载中使用，不是用户登录令牌，不发送给浏览器。执行器仍校验服务凭据，并拒绝登录和静态页面接口。它不暴露任意命令、文件操作或 Docker API；保留参数校验、任务资源互斥和容器归属检查。模型路径在执行器模式下只允许状态目录 models/ 或 /srv/phyxi/models/，解析真实路径后防止软链接越界。

执行器仍是有高权限的可信组件：固定任务接口降低攻击面，并不等于沙箱。单机模式不实现跨服务器连接、mTLS 或多节点调度。

## 首次安装与更新
首次由运维运行 `sudo python3 scripts/bootstrap-host.py --install`：检查已有驱动，仅在 Docker 缺失时从官方签名 APT 源安装 Docker 与 Compose；已有 Docker 不升级、不重启。显卡驱动缺失或现有 containerd/runc 包冲突时停止并说明原因，不自动卸载。初始化不安装 Toolkit/Nginx/vLLM。导入并校验发布镜像后执行：

```bash
sudo python3 scripts/install-executor.py --data-dir /srv/phyxi/data --image sha256:<已导入镜像ID> --admin-password-file /root/phyxi-admin-password
```

已有 admin.hash 的迁移/更新不需要密码文件。新安装密码文件应为 0600，用后删除。安装器提取相同发布镜像中的二进制，安装 systemd 服务和受限控制台容器。更新前等待任务完成，并创建加密备份；只有功能验证通过后才能删除旧版本。执行器启动失败时使用 systemd 日志诊断，不将失败解释为安装成功。

新 compose 为 `deployment/compose.executor.yaml`，不要叠加旧的 `compose.gpu-utility.yaml`。`compose.console-only.yaml` 是迁移前兼容方式，不是新部署推荐入口。

## 宿主机环境操作
页面明确显示数据来自宿主机执行器，提供三个独立任务：

1. 安装 NVIDIA 容器工具：检查 Ubuntu、驱动与 Docker；已安装且配置正常直接成功，不重复安装。缺失时使用 NVIDIA 官方签名 APT 源安装固定版本，不安装/更新驱动、CUDA Toolkit 或 Nginx。
2. 修复 Docker GPU 配置：需先安装 Toolkit，备份已有 daemon.json，再配置 NVIDIA runtime。涉及 Docker 重启，控制台会短暂断开；任务继续由宿主机执行器记录。
3. 验证 GPU 容器访问：使用已经导入的固定控制镜像执行 nvidia-smi，禁用网络，不下载镜像，不加载模型；设置资源限制，超时清理任务专属容器。

安装/修复前检查正在运行的容器与 GPU 计算进程。仅允许本产品控制台容器在线；发现模型、接入组件或外部容器时拒绝变更，不能擅自停止任何负载。安装/修复任务不允许强制取消，避免中断包管理器。Docker 重启和并发外部启动任务存在运维竞态，维护期间需由管理员保证不启动其他工作负载。

## 状态与故障
控制台重启：任务继续；重新登录后读取原任务与事件。
执行器不可用：控制台仍可登录，API 返回 503 和明确提示；已有 Docker 服务继续运行。
执行器重启：持久化任务被标记为中断，不假装成功；不自动重放安装或部署操作，管理员检查后重试。容器状态重新读取。
服务器重启：systemd 与 Docker restart policy 恢复服务；单机部署不承诺高可用。

## 开发阶段与验证
1. 分离宿主机执行器、控制台权限与存储；验证鉴权、转发、重启恢复。
2. 接入固定宿主机安装/修复/GPU 验证任务，调整页面状态和说明。
3. 沿用已有模型、网关、工作区操作的类型化接口和归属检查，通过执行器统一执行；测试业务请求与管理进程解耦。

用户指定在目标服务器运行测试，不启动本地控制台或测试容器。服务器执行 fmt、clippy、cargo test/build、smoke-console.py、test-api-contract.py、test-executor.py；CI重复检查并构建原生 AMD64 发布包。真实 A10 证据仅证明测试过的 GPU 容器访问，不能代替 vLLM 模型生成验收。

参考：GPUStack Server/Worker 分工与允许同机部署、Portainer Server/Agent、Docker daemon 安全边界。此实现借鉴职责，不部署这些平台。

## 手动备份与恢复
使用 `sudo python3 scripts/backup-executor.py --output /srv/phyxi-backups/<新目录>`，交互输入加密密码；工具检查无执行任务后短暂停止控制台和执行器，分别加密备份执行器数据与控制台登录目录，再恢复原运行状态。模型/网关容器不停止，不备份可重新下载的模型权重。成功目录有 complete=true 的 manifest；不完整目录不能当成有效备份。

恢复前停止控制台和执行器，使用发布二进制 `--restore <archive>` 分别恢复两个归档到 manifest 中记录的原始绝对路径；恢复程序拒绝更改路径，避免证书和权重引用失效。先将旧目录移至备份位置，保证原路径不存在；恢复后核对内容，并恢复 console-auth 的 10001:10001 所有权。保持原 owner 标识及服务密钥，重新启动执行器和控制台，检查容器归属和工作区接入。恢复期间禁止执行新部署任务。

安装依据：[Docker Ubuntu 官方安装步骤](https://docs.docker.com/engine/install/ubuntu/)、[NVIDIA Container Toolkit 官方安装步骤](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/latest/install-guide.html)。
