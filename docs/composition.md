# 独立配置与组合使用

## 数据与职责

| 配置类型 | 保存内容 | 引用 |
|---|---|---|
| models | 服务模型名、服务器路径、可选固定版本下载来源 | 无 |
| images | 引擎类型、Docker 标签或摘要地址；执行时固定本地镜像 ID | 无 |
| runtimes | vLLM/SGLang/CPU 后端、上下文、并发、显存比例、解析器、验收能力 | 无 |
| deployments | 名称、服务模型名、GPU UUID、端口 | model、image、runtime |
| gateways | 域名、网段、TLS/CA、监听地址 | deployment |
| workspaces | 客户端凭据环境变量名 | gateway |

可以先创建空引用的部署、网关或工作区，再完成组合。非空引用必须存在。保存独立配置只检查自身字段；启动部署时验证完整组合（包括 GGUF/目录、GPU UUID/TP、上下文与并发限制）。CPU 后端不要求镜像。无需企业 CA 即可启动模型；企业 CA 属于网关。工作区是客户端连接模板，不启动工作站、容器或模型。

多个草稿可以复用同一配置。每次修改递增修订号，陈旧修订写入被拒绝。每类最多 100 条，不接受任意启动 shell 参数。不允许删除仍有引用的记录或正在使用的部署/网关对象。

## 生命周期与快照

启动时解析引用并保存所有依赖的名称、ID、修订号和内容。运行时读取任务快照，不读取后来修改的配置库。停止根据实例启动任务判断归属，不会因同名或相同配置而误停别的部署。停止模型前需停止网关。

模型验收比较选定组合与真实运行配置，草稿偏离时拒绝验收。网关启动绑定选定部署的已运行实例；网关验收、发布使用真实模型快照及选定网关配置。独立容器模式可在线应用网关草稿，原生模式仍需停止后重新启动，之后重新验收发布。模型换版仍需停机窗口；没有自动切流。

网关配置与工作区模板可独立修改，但不自动变更运行服务。工作区导出使用当前引用的配置草稿，明确标记为模板；正式发布材料由网关验收成功后生成。企业模式客户端使用所属工作区新生成的 API Key，并安装可信 CA；一个工作区一个有效密钥，可独立重置、撤销和停用。旧原生测试模式使用共享令牌，仅限兼容验证。

## HTTP API

企业模式使用管理员会话 Cookie，写操作另需 CSRF；仅本地兼容模式接受控制台 Bearer Token。浏览器不直接接触 Docker socket。

- `GET /api/presets`：只返回运行环境、模型仓库及下载站点预设，不下载资源。
- `GET /api/models/{id}/status`：检查本地权重与下载清单是否完整。
- `GET /api/catalog`：读取独立配置库。
- `PUT /api/catalog/{kind}/{id}`：`{"name":"名称","revision":0,"data":{...}}`；创建修订为 0，更新传读取到的修订号。
- `DELETE /api/catalog/{kind}/{id}`：被引用或运行中的对象拒绝删除。
- `GET /api/deployments/{id}/preview`：检查组合并返回最终 Config 和依赖快照。
- `GET /api/images/{id}/status`：读取本地 Docker 镜像 ID、架构、大小及标签，不启动容器。
- `GET /api/workspaces/{id}/export`：返回入口、模型、凭据环境变量、CA 路径提示和 OpenCode 模板；不含私钥和真实令牌。
- `POST /api/actions/{action}?kind={kind}&id={id}`：作用于明确对象的操作。
  - models：download
  - images：pull_image
  - deployments：start、redeploy、stop、verify
  - gateways：certificate、gateway_start、gateway_apply、gateway_stop、verify、publish
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

## 资源准备

`models.source` 包含 repo、revision、endpoint、可选 file。支持官方/HF 镜像站仓库 URL 或 owner/name；main/tag 保存时解析为固定 commit，endpoint 限定官方与 hf-mirror。目录来源需下载清单与实际文件一致，自行准备的本地目录至少检查 config.json 和非空权重文件；完整可加载性由推理引擎判断。

images.engine 与 runtimes.engine 必须一致。GPU 部署读取已存在的本地镜像 ID，以 `--pull never` 和离线模型环境变量启动；缺少资源时要求用户先执行 download / pull_image，不隐式下载。预设与自定义地址使用相同准备流程。验证：`python3 scripts/test-presets.py`。

## 企业接口与应用语义

- `GET /api/deployments/{id}/comparison`：草稿、实际运行快照、字段差异及准备错误；不会应用配置。
- `POST /api/actions/redeploy?kind=deployments&id=…`：停止所属模型及关联网关，以冻结组合重新启动；保留旧快照，无自动回滚。
- `POST /api/gateways/{id}/certificate`：revision、certificate、private_key、ca；校验证书后保存草稿，不立即生效。
- `POST /api/actions/gateway_apply?kind=gateways&id=…`：独立 Nginx 业务网关在线应用；先验证证书与配置，再重载并确认摘要。
- `GET/POST /api/workspaces/{id}/credential`：查询状态 / apply、rotate、revoke、disable、enable；只在 rotate 成功响应返回明文密钥。
- `GET /api/workspaces/{id}/requests`：该工作区最近请求元数据，不含推理正文。
- `GET /api/audit`：管理操作记录；共享账号不能区分具体个人。

服务模型名由部署指定，留空时继承旧模型记录中的 model_name，保持存量配置兼容。企业容器内固定模型端口 8000，不发布宿主机端口。备份恢复使用离线 CLI，详见 Docker 运维说明。
