# Ubuntu / A10 验证记录 · 2026-11-12

## 本轮范围与结束状态

用户授权在目标服务器测试，并确认不再使用旧版。随后要求不再处理镜像下载，完成其余工作后停止。本轮已停止镜像下载、模型下载和临时文件传输；保留新版控制服务运行，不运行模型或网关。没有启动后台自动验收任务。

服务器：Ubuntu 24.04.4 LTS、Linux 6.8 x86_64、2×NVIDIA A10（每卡 23028 MiB）、驱动 580.126.09。`nvidia-smi` 显示 CUDA 13.0 驱动兼容版本。原有 Docker 29.8.0、NVIDIA Container Toolkit 1.20.1，无运行中或停止的旧模型容器。

## 已通过

- Linux 原生 release 编译；Rust 1.94.0 和 vendor 依赖位于独立构建目录，不是服务运行依赖。
- 安装脚本创建 `phyxi` 账号、固定 root 所有二进制、受控 sudo 入口与 systemd 服务；服务启动和重启通过，仅监听 `127.0.0.1:8765`。
- 通过控制台 API 执行环境准备，安装缺失的 Nginx 1.24.0 和更新系统提供的 OpenSSL；保留驱动/CUDA和既有 Docker 数据目录。Nginx 系统服务未启动。
- 原有 Docker APT 源出现索引大小/哈希不匹配，准备任务按预期失败。将该旧源移入归档、保留另一条有效签名源后重试成功；未关闭包签名或哈希校验。
- 六项环境检查通过；Docker 检查结果缩减为版本与 runtime 名称，避免输出整份运行时能力 JSON。
- 实际 CUDA 内核：按 GPU UUID 仅暴露一张 A10，设备数为 1、计算结果为 42；进程退出后两张卡均为 0 MiB。此项仅证明基础 CUDA 和设备可见性，不代表 vLLM 或容器 GPU 验收。
- 自签测试证书通过控制台生成/验证；证书仅供回环测试，不是企业真实 CA。
- Linux 上 14 项 Rust 测试、HTTP smoke 与协议测试通过；后两项使用隔离模拟运行时，不冒充真实模型测试。
- SSH 隧道下真实页面连接、环境/任务展示、长错误信息换行与列表摘要检查。控制台进程采样约 7.3 MiB RSS（单次采样，非峰值）。

## 下载与未完成项目

计划使用 SmolLM2-135M-Instruct，固定 revision `12fd25f77366fa6b3b4b768ec3050bf629380bac`。主权重 269060552 字节，官方元数据 SHA256 为 `5af571cbf074e6d21a03528d2330792e532ca608f24ac70a143f6b369968ab8c`。本地官方源下载与校验通过，但服务器传输未完成。

服务器官方 Hugging Face 请求超时，hf-mirror 元数据可达；固定 revision 和主权重哈希与官方元数据相同。新增管理员启动变量 `PHYXI_HF_ENDPOINT`，仅允许官方站/hf-mirror，源地址记入任务与模型清单。镜像站部分配置文件下载通过，主权重下载按用户要求中止，不能宣称完整权重准备完成。

候选镜像来自[阿里云官方预置镜像说明](https://help.aliyun.com/zh/cap/user-guide/custom-model-deployment)，使用 `serverless-registry.cn-hangzhou.cr.aliyuncs.com/functionai/vllm-openai:v0.11.0`，manifest 固定摘要 `sha256:d8d39b59e909d2378ac4feeb191f7e7b6f1342477dc66b7c47cec89e9985ad8a`，压缩层合计 12453996057 字节。下载慢，已按用户要求停止，未加载或启动该镜像。

因此以下项目仍未通过实机验收：vLLM 容器启动、容器 GPU UUID 隔离、GPU 文本/SSE、实际模型经 TLS 网关发布、模型存活时控制台重启恢复、企业 CA 与工作站接入。`scripts/test-gpu-server.py` 已准备好这些验收步骤，但本轮未执行；不要将脚本存在视为测试通过。

## 留存位置与后续复现

- 新版程序：`/opt/phyxi-console/phyxi-console`；服务：`phyxi-console.service`。
- 服务状态：`/var/lib/phyxi-console`；令牌文件保持私有，不写入本记录。
- 源码：`/opt/phyxi-console-src`；构建工具：`/opt/phyxi-build`。
- 旧版目录与失效 APT 源：`/opt/phyxi-retired-20261112`，权限 0700，未恢复 Notebook。
- 镜像下载临时 systemd 单元已停止；无推理容器、无网关子进程、无执行中控制台任务。部分下载文件留作诊断，不是可用模型。

待镜像和权重完整、模型配置限制为一张 GPU / 单并发 / 上下文 ≤2048 / 显存预算 ≤20%，且控制台无运行实例时，可在服务器执行：

```bash
sudo python3 /opt/phyxi-console-src/scripts/test-gpu-server.py \
  --state-dir /var/lib/phyxi-console \
  --evidence /var/lib/phyxi-console/test-evidence/gpu-acceptance.json \
  --restart-service
```

该脚本会实际启动模型、进行验收并清理本次实例，不能在有业务实例时执行。本轮没有自动安排此命令。
