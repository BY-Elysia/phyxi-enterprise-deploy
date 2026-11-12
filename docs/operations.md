# 安装与运维

## 新服务器

需要 Ubuntu 22.04/24.04、已工作且可运行 nvidia-smi 的驱动；不要求事先安装 Docker/Toolkit/Nginx。控制台不安装或升级显卡驱动，不修改 CUDA 安装，不格式化数据盘。

在匹配目标架构的 Linux 环境构建 `cargo build --release --locked`，复制二进制与 `scripts/install-console.sh`，执行 README 中安装命令。Mac ARM64 二进制不能直接复制到 Linux A10/H20 服务器。

安装器创建 `phyxi` 账号、固定 `/opt/phyxi-console/phyxi-console`、状态目录 `/var/lib/phyxi-console`、systemd服务及精确 sudoers。通过 SSH 隧道打开控制台，检查环境后执行准备。任务输出实际包版本。环境准备完成后若Docker组权限未生效，重启控制台服务。

已有业务容器/训练任务的服务器：不执行自动主机准备，管理员先配置独立运行环境，再通过 `PHYXI_DOCKER_HOST` 指定已准备好的 Unix socket。自动准备只操作默认系统 Docker，不迁移旧容器数据。

## 证书与业务入口

测试模式自动生成7天自签证书，只能监听127.0.0.1。企业模式由管理员将已签发证书链、私钥和CA放到服务账号可读的路径，通过表单导入；验证证书链、域名及公私钥配对后复制到受控状态目录。

网关端口默认18443，来源网段必须明确配置。企业DNS/入口应指向该服务；如果业务要求443，可由现有企业入口映射到18443。后端API始终监听回环。网关只开放 `/v1/models` 和 `/v1/chat/completions`，工作站token校验后清除该认证头再转发。服务账号与同机管理员属于信任边界。

修改证书前停止本控制台网关。修改模型前先停止网关再停止模型。先加载新模型、准备网关，再验收发布；只有一组GPU时存在停机窗口。

## 工作站材料

发布成功在 `gateway/client-export/` 写入：

- `company-ca.pem`
- `workstation.token`
- `endpoint.json`
- `opencode-2.2.json`：沿用项目原 OpenCode 2.2 配置结构，API key 引用 `PHYXI_GATEWAY_TOKEN` 环境变量，能力来自本次验收配置。

由管理员受控分发；不要提交Git，不把私钥交给工作站。客户端设置 `OPENCODE_CONFIG`、`NODE_EXTRA_CA_CERTS` 和 `PHYXI_GATEWAY_TOKEN`，按原集成流程接入。当前没有验证最新OpenCode版本和Windows桌面实际安装，不将配置导出等同于端到端工作站验收。

## 失败与恢复

| 情况 | 操作 |
|---|---|
| 缺依赖 | 环境页面检查，专用服务器才执行准备 |
| GPU占用 | 重新分配或等待外部任务结束；不自动杀外部进程 |
| 主机准备失败 | 查任务日志/系统APT状态，解决后重试；不自动回滚已安装软件 |
| 下载校验失败 | 保留partial诊断，修复网络/来源后重试；失败文件不发布为正式文件 |
| 模型失败/超时 | 查模型日志，显式停止，再调参重试 |
| 网关启动失败 | 检查状态目录 `gateway/error.log` 和 `gateway/process.log` |
| 验收失败 | 查看报告和任务日志，禁止记录新发布 |
| 控制台重启 | 查真实模型状态，任务显示interrupted；重新启动本地CPU/网关，不自动重复安装/发布 |
| 无法操作旧实例 | 必须使用原owner状态目录，不按同名/PID猜测归属 |

`--prepare-host` 使用临时 `/usr/sbin/policy-rc.d` 防止安装包自动启动服务，正常退出会移除自己创建的文件。若管理员强制杀死准备进程，需检查APT及该文件状态；原有policy文件从不覆盖。首次写Docker配置前保留 `.phyxi-original` 备份。

停止控制服务后备份整个状态目录，包括SQLite/WAL、owner、token、网关证书和发布材料。不要只复制数据库或删除owner。

Prometheus/Grafana按需部署，示例见 `deployment/prometheus.example.yml`。控制台采样与模型指标分别采集；模型指标是否可用取决于实际引擎启动配置。默认不部署外部监控服务。
