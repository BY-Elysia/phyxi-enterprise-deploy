# 运行与恢复

服务账号应能访问明确指定的 Docker socket。使用本机 Unix socket，禁止由请求指定任意运行时地址。Docker 权限具有主机级能力，控制台应只面向受信任管理员，通过 SSH 隧道访问。

建议把 `PHYXI_STATE_DIR` 设为服务账号专属的固定绝对路径。不要在模型运行时删除状态目录或 owner 文件，否则控制台会拒绝操作失去归属证明的容器。数据库、token、owner 一并备份；停止管理服务后备份完整目录，避免遗漏 WAL。

模型端口仅回环绑定，没有企业 TLS/API 鉴权。当前不能作为企业正式入口直接发布；未来按明确需求迁移 legacy 网关和业务验收。

## 常见情况

| 情况 | 处理 |
|---|---|
| nvidia-smi 不存在 | 在目标 Linux GPU 服务器准备驱动；控制台不自动安装 |
| Docker 不可用 | 检查 socket 路径、服务状态与账号权限 |
| 所选 GPU 已占用 | 查看外部任务，重新分配空闲卡；不自动终止其他进程 |
| 同名容器属于其他控制台 | 使用原状态目录管理，或由管理员审查迁移，不自动删除 |
| 容器退出/健康超时 | 刷新模型日志；保留证据后停止实例，再调整配置部署 |
| 控制台重启后任务 interrupted | 查看运行状态；已运行模型可能继续提供服务，确认后停止/重试 |
| 取消申请已提交 | 等待当前命令安全结束；如已创建容器，再执行停止 |
| 修改模型或 GPU | 保存新配置；停止旧实例；部署。首版有停机窗口 |

控制台不提供自动回滚。旧权重目录不删除，任务快照可在 `/api/jobs` 查看，用其恢复参数后重新部署。恢复需要正常的加载和健康检查时间。

## 常驻运行示例

在 Linux 创建服务账号及专属状态目录后，可参考下列 systemd 单元；路径与账号必须按实际安装填写，不由控制台自动写入系统：

```ini
[Unit]
Description=PhyXi deployment console
After=docker.service

[Service]
User=phyxi
Group=phyxi
SupplementaryGroups=docker
Environment=PHYXI_STATE_DIR=/var/lib/phyxi-console
ExecStart=/opt/phyxi-console/phyxi-console
Restart=on-failure
UMask=0077

[Install]
WantedBy=multi-user.target
```

启停控制台不会自动停止模型容器。运行配置仍由模型任务管理。
