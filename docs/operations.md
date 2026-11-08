# 运行、发布与维护

以下路径以默认 `/srv/phyxi` 为例，自定义 `data_root` 时使用对应根目录。首次部署参数均在 Notebook 第 1 节填写，无需编辑脚本。服务运行后修改版本、数据目录或资源配置，按维护发布流程执行；Notebook 不热覆盖正在运行的配置。

## 状态和停止

```bash
cd /srv/phyxi/current
bash dc.sh ps
bash dc.sh logs --tail 100 model
systemctl status phyxi-model.service
# 停止服务并释放 GPU，保留数据：
systemctl stop phyxi-model.service
# 检查指定 GPU 空闲后恢复：
systemctl start phyxi-model.service
```

不要对正式服务器运行共享训练测试的独立 Engine 脚本。本仓库固定使用专用服务器 `/var/run/docker.sock`，`dc.sh` 清除环境覆盖。

## 新工作站 Token

由管理员以安全随机数创建新 Token，保存到 root 600 文件，并将 `"Bearer <TOKEN>" "cad-ws-02";` 写入 `/srv/phyxi/secrets/client-keys.map`。保持文件由 UID 101 读取；运行 `bash dc.sh exec -T gateway nginx -t`，通过后 `nginx -s reload`。秘密不输出到 Notebook，不放入 Git。后端密钥与工作站密钥职责不同，不能混用。

## 证书与密钥轮换

Nginx 挂载整个 secrets 目录，替换证书后先执行 nginx -t 再 reload。检查域名、证书链、私钥配对和工作站信任。后端 Key 是独立文件挂载：轮换后重建 model 容器并同步网关认证头，在维护窗口完成。不要运行初始化脚本覆盖多工作站 map。

## 监控

Prometheus 15 秒抓取内部 /metrics，保留 15 天或最多 10 GB；本机 9090 可通过 SSH 隧道访问。Notebook 中填写企业 Alertmanager 地址后才有告警接收目标；规则包括服务不可达和等待队列持续堆积。实际通知通道由企业 Alertmanager 管理，需做一次真实送达测试。

## 发布与回滚

每个发布目录都包含完整 Compose、脚本、.env 和部署参数快照，模型和镜像按固定版本保留。升级统一在 Notebook 中执行：

1. 执行第 19 节停止服务。
2. 在第 1 节填写新的 `release` 和变更参数，执行配置单元。
3. 按变更项重新运行软件、镜像或模型准备步骤。
4. 执行第 10 节生成配置，保存 `previous-release.json` 并切换 `current`；已尝试启动的发布目录不可用变更参数覆盖。
5. 域名或证书变化时执行第 11、12 节，然后执行第 13—15 节检查、启动和验收。

回滚时，在维护窗口执行（将路径替换为 `previous-release.json` 中记录的旧发布目录）：

```bash
systemctl stop phyxi-model.service
ln -s /srv/phyxi/releases/旧版本 /srv/phyxi/current.next
mv -Tf /srv/phyxi/current.next /srv/phyxi/current
systemctl start phyxi-model.service
```

失败时停服务，把 current 指回已保存旧版本，再启动并重跑接口验收。不要同时在相同八卡上启动新旧模型。保留相容密钥、证书、镜像和模型文件。测试主机重启后的挂载、Docker 与模型服务恢复；单节点升级有中断，连续服务需另配节点。

## 交付证据

保存 `artifacts/`、`evidence/`，包括软件包版本、镜像 digest、模型 commit/hash、配置、API 结果、实际任务样本、吞吐和延迟。生产容量按各实际任务分别测量，多轮工具等待也计入任务耗时。Notebook 中的单轮短测只作基础负载验收。
