# 独立配置与组合使用

## 数据与职责

| 配置类型 | 保存内容 | 引用 |
|---|---|---|
| models | 服务模型名、服务器路径、可选固定版本下载来源 | 无 |
| images | 带 SHA256 摘要的镜像地址，可包含可读标签 | 无 |
| runtimes | vLLM/CPU 后端、上下文、并发、显存比例、解析器、验收能力 | 无 |
| deployments | 名称、GPU UUID、端口 | model、image、runtime |
| gateways | 域名、网段、TLS/CA、监听地址 | deployment |
| workspaces | 客户端凭据环境变量名 | gateway |

可以先创建空引用的部署、网关或工作区，再完成组合。非空引用必须存在。保存独立配置只检查自身字段；启动部署时验证完整组合（包括 GGUF/目录、GPU UUID/TP、上下文与并发限制）。CPU 后端不要求镜像。无需企业 CA 即可启动模型；企业 CA 属于网关。工作区是客户端连接模板，不启动工作站、容器或模型。

多个草稿可以复用同一配置。每次修改递增修订号，陈旧修订写入被拒绝。每类最多 100 条，不接受任意启动 shell 参数。不允许删除仍有引用的记录或正在使用的部署/网关对象。

## 生命周期与快照

启动时解析引用并保存所有依赖的名称、ID、修订号和内容。运行时读取任务快照，不读取后来修改的配置库。停止根据实例启动任务判断归属，不会因同名或相同配置而误停别的部署。停止模型前需停止网关。

模型验收比较选定组合与真实运行配置，草稿偏离时拒绝验收。网关启动绑定选定部署的已运行实例；网关验收、发布使用真实模型快照及选定网关配置。修改网关草稿后需要停止、重新准备证书并启动，才可重新发布。模型换版仍需停机窗口；没有自动切流。

网关配置与工作区模板可独立修改，但不自动变更运行服务。工作区导出使用当前引用的配置草稿，明确标记为模板；正式发布材料由网关验收成功后生成。客户端环境变量需要设置为该服务器 `gateway/client.token` 中的凭据，并安装可信 CA；此版本只支持单个活动网关及其共享令牌，不宣称每工作区独立鉴权或凭据隔离。

## HTTP API

以下接口均使用控制台 Bearer Token，浏览器不持有 root 权限。

- `GET /api/catalog`：读取独立配置库。
- `PUT /api/catalog/{kind}/{id}`：`{"name":"名称","revision":0,"data":{...}}`；创建修订为 0，更新传读取到的修订号。
- `DELETE /api/catalog/{kind}/{id}`：被引用或运行中的对象拒绝删除。
- `GET /api/deployments/{id}/preview`：检查组合并返回最终 Config 和依赖快照。
- `GET /api/images/{id}/status`：读取本地 Docker 镜像 ID、架构、大小及标签，不启动容器。
- `GET /api/workspaces/{id}/export`：返回入口、模型、凭据环境变量、CA 路径提示和 OpenCode 模板；不含私钥和真实令牌。
- `POST /api/actions/{action}?kind={kind}&id={id}`：作用于明确对象的操作。
  - models：download
  - images：pull_image
  - deployments：start、stop、verify
  - gateways：certificate、gateway_start、gateway_stop、verify、publish
- `GET /api/jobs`：返回操作记录与完整 composition 快照。
- `GET /api/jobs/{id}/events?tail=true`：最近 200 条；原 `after` 游标仍可使用。

服务器级 prepare/cancel 不要求配置对象。旧 `/config`、`/gateway` 和无作用域的 actions 保留兼容现有验收脚本；旧配置在首次升级迁移一次，之后兼容接口不再同步修改新配置库。新前端只使用有作用域的模型和网关操作。旧版启动的存量模型没有组合归属时需先使用原接口停止，再用新部署对象启动。

## 验证

```bash
python3 scripts/test-composition.py
python3 scripts/test-composition.py \
  --llama /absolute/path/llama-server \
  --model /absolute/path/small-model.gguf \
  --nginx /absolute/path/nginx
```

默认使用隔离的协议模拟器测试 CRUD、引用、修订、组合、归属和持久化。传入三个实际文件路径后测试 <200 MiB GGUF、单并发、512 上下文的真实 CPU 推理，以及 TLS 网关、发布与快照一致性；测试结束清理自身实例。GPU/vLLM 和企业 CA 仍需服务器实机验收。
