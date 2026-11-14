# 镜像下载来源

## 决策与使用

单服务器不增加 Harbor 或代理容器，不修改 Docker 全局 registry-mirrors。执行器在拉取前显式解析地址；控制台在“推理引擎镜像 → 镜像下载来源”配置。

- 默认：DaoCloud 官方推荐前缀 `m.daocloud.io/docker.io`，只转发公开的 `library/nginx`、`vllm/vllm-openai`、`lmsysorg/sglang` 命名空间。
- 企业仓库代理：填写如 `registry.company.example/dockerhub`；仓库需提前同步所选版本并保留上述路径，不能只提供一个空仓库。私有仓库认证由运维在执行器宿主机的 Docker 配置中准备，界面不收集或显示仓库密码。
- 直接使用原始地址：适用于网络已开放或自行准备原始地址镜像。
- 自定义完整地址：推理引擎配置和 Nginx 地址均可填写企业自己的路径，如 `registry.company.example/ai/vllm:approved`。带显式仓库主机的地址原样使用，不再加公共前缀。未列入公开名单的 Docker Hub 命名空间也不转发到公共源。

可选手工 Compose 模板使用同一默认 Nginx 地址，运维通过 `PHYXI_NGINX_IMAGE` 覆盖；这些手工模板不读取控制台数据库或 JSON 设置。

工作区鉴权复用已安装控制台镜像，不额外下载第三方鉴权镜像。权重下载和宿主机 apt 源独立，不受此设置影响。

保存来源不会下载镜像，也不重启服务；只影响下一次准备与检查。切换来源后按新地址检查本地镜像；导入离线镜像时应保留实际下载地址的标签/摘要，或把镜像配置改为导入的完整地址/不可变 ID。已有 Nginx 容器恢复时沿用其镜像，此设置不自动升级它。

## 版本与安全边界

预设固定官方摘要，由 Docker 校验下载内容。vLLM v0.11.0、SGLang v0.5.5 是现有版本的固定化，不表示最新版本或已通过当前 GPU 推理验收。用户可自定义版本。启动模型及新建 Nginx 时解析本地不可变镜像 ID，使用 `--pull never`。

- Nginx 1.28-bookworm：`sha256:552e7481ca93ffccd046aa658dbbed22caefbc09c66fa7cd247cbb90b8a5c609`
- vLLM v0.11.0：`sha256:014a95f21c9edf6abe0aea6b07353f96baa4ec291c427bb1176dc7c93a85845c`
- SGLang v0.5.5：`sha256:5877b90ffcdba23b6072ee90a2f3ab046e926be19dd828074a6956a0c6c77ba5`

镜像来源保存到执行器数据目录 `image-sources.json`（0600），纳入加密备份。接口沿用管理员登录、CSRF 与管理审计，执行任务时禁止修改来源。失败明确显示实际地址及诊断日志，不静默切换其他公共服务。下载可取消，单次最多四小时；Docker 保留已完成层，重试不承诺未完成层按字节续传。

## 服务器实测（2026-11-14）

目标 47.120.39.81，Ubuntu / Linux AMD64；只在服务器执行容器验证。

| 路径 | 实测结果 |
|---|---|
| Docker Hub registry-1.docker.io | TCP 443 连接超时 |
| DaoCloud docker.m.daocloud.io | Nginx 完整下载成功；vLLM 官方清单摘要匹配，8 MiB 大层采样约 0.44 MiB/s |
| DaoCloud 推荐 m.daocloud.io/docker.io | 固定摘要 Nginx 拉取成功，隔离容器运行输出 nginx/1.28.0；vLLM 8 MiB 采样 18.13 秒（约 0.44 MiB/s）；SGLang 8 MiB 采样 8.30 秒（约 0.96 MiB/s），两者清单与官方摘要一致 |
| docker.1ms.run | vLLM 清单匹配；约 0.07 MiB/s，20 秒采样超时 |
| docker.1panel.live | vLLM 清单请求被拒绝 |

采样不能证明长期稳定性，也不是完整 vLLM/SGLang 下载或推理测试。vLLM 有约 12.45 GB 压缩层，90 秒限时拉取仅完成部分层后取消；没有启动模型。Nginx 推荐路径检查复用了先前完整下载的层，不能据此计算完整下载速度。

选择 DaoCloud 是基于这台服务器的可达性、内容校验及候选对比，不承诺公共源永久可用或高速。企业生产长期优先配置自己的受控仓库；完全离线需运维传入镜像归档并 `docker load`，仅改源无法解决断网。

参考：[DaoCloud 官方说明](https://github.com/DaoCloud/public-image-mirror)、[Docker 镜像代理说明](https://docs.docker.com/docker-hub/image-library/mirror/)。代理首次缓存可能仍需要上游网络。
