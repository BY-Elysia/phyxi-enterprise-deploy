# GLM-5.3-Flash 企业部署

面向 **8×H20 专用服务器**的 vLLM + Nginx + Prometheus 部署与 OpenCode 接入。模型使用官方 `zai-org/GLM-5.3-Flash` 原生 FP8 checkpoint。主要入口为 [部署 Notebook](notebooks/01-deploy-glm53-flash.ipynb)。

## 从全新服务器开始

准备 Ubuntu 24.04 x86_64、SSH 管理权限、已挂载到 `/srv/phyxi` 的数据卷。推荐 ≥768 GiB 主机内存、≥1.5 TB 数据卷；初次制备至少 600 GiB 空闲。磁盘分区和格式化由管理员完成，Notebook 不擦除磁盘。

在服务器管理员终端执行（私有仓库通过自己的 GitHub SSH key 或凭据获取，不把 token 写入 URL）：

```bash
sudo -i
apt-get update
apt-get install -y git ca-certificates
# 需为当前管理员账号配置有权读取该私有仓库的 GitHub SSH key。
git clone git@github.com:BY-Elysia/phyxi-enterprise-deploy.git /opt/phyxi-enterprise-deploy
cd /opt/phyxi-enterprise-deploy
bash scripts/bootstrap-notebook.sh
```

也可在有权限的工作站下载仓库 ZIP，上传后解压到 `/opt/phyxi-enterprise-deploy`，从 bootstrap 命令继续；无需在服务器配置 GitHub 凭据。

在自己的工作站建立隧道：

```bash
ssh -N -L 8888:127.0.0.1:8888 管理用户@企业服务器
```

使用服务器终端输出的 Jupyter token，在浏览器访问 `http://127.0.0.1:8888`，打开 `notebooks/01-deploy-glm53-flash.ipynb`。按单元顺序执行。第 1 节列出所有需要填写或确认的参数，在 `OVERRIDES` 字典中把对应 `None` 替换成实际值即可；无需修改脚本、JSON 或 YAML。`None` 保留已保存值，首次采用默认值。每个步骤前说明相关字段和生效条件。管理员 Notebook 只监听回环，不开放公网；交付后关闭 Jupyter，模型由 systemd 独立运行。

驱动安装后的重启、企业 CA 签发是两个明确停点。重启后再次运行 bootstrap，重跑 Notebook 的环境定位/配置读取单元，再从提示的步骤继续。

## 文件结构

```text
notebooks/01-deploy-glm53-flash.ipynb  分步说明、配置、执行、验收入口
scripts/deploy.py                    服务器部署阶段实现
scripts/download-model.py            固定 commit 下载与哈希清单
scripts/verify-api.py                鉴权/文字/SSE/图片/两轮工具验收
scripts/load-test.py                 有界并发与实际 token 统计
scripts/setup-workstation.ps1        Windows OpenCode 与项目接入
scripts/start-client.py              隔离 OpenCode 配置与进程
scripts/stop-client.py               只停止记录的本地客户端
scripts/install-adapter.py           按源码 hash 安装项目连接适配
scripts/bootstrap-notebook.sh        全新服务器的 Jupyter 引导
 deployment/                         Compose、Nginx、systemd、监控、版本锁
integrations/                       项目连接适配与来源/校验记录
docs/                               参数、架构、运维、工作站说明
tests/                              本地静态和隔离测试；不调用 GPU
```

## 基线与版本策略

- **模型**：官方 FP8，固定 commit，详见 `deployment/versions.json`。
- **推理镜像**：从官方 nightly 解析的 Linux amd64 digest。运行时始终使用 digest，不随 nightly 漂移；Notebook 进一步核对 GLM 架构、glm47 解析器和依赖。
- **多模态**：Python 前端、`tokenizer-mode=auto`；H20 上采用 BF16 KV。
- **资源**：TP=8，初始上下文 65,536 token，`max-num-seqs=16`，每卡显存预算 0.85。这些是配置初值，不是容量保证。
- **服务**：仅 Nginx 在企业内网 IP:443 发布；模型 8000 不映射宿主机；Prometheus 9090 只监听服务器回环。
- **存储**：默认 `/srv/phyxi`，首次部署可在 Notebook 的 `data_root` 修改为已挂载的专用数据卷；生成的配置路径同步替换。已部署的 Docker 数据目录不能当作普通参数直接迁移。版本、资源参数的变更与生效顺序已写在 Notebook 第 10 节，全部通过单元格完成。
- **思考**：使用 GLM 支持的 `reasoning_effort=low`；仍可能生成思考 token。TPS 按 API 实际 completion_tokens 统计，包含思考。

[参数说明](docs/parameters.md) · [架构流程](docs/architecture.md) · [Windows 接入](docs/workstation.md) · [运维与回滚](docs/operations.md) · [官方依据](docs/sources.md)

## 验证范围

仓库交付的是 GLM 企业部署方案。此前小模型联调仅用于改进隔离、认证、工具回填和 OpenCode 接入逻辑，不上传其测试配置或把吞吐结果用于 GLM。首次 GLM GPU 启动、图片/工具正确性和容量数据由 Notebook 在目标服务器生成。

本地检查：

```bash
python3 -m unittest discover -s tests -v
python3 scripts/check-repository.py
```

`config.local.json`、密钥、Notebook 执行输出及服务器证据不应提交。仓库未包含模型权重、工作站 Token 或服务器 TLS 私钥。
