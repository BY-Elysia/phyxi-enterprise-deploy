# 本地验证 · 2026-11-12

## 环境与材料

macOS ARM64，Rust release 控制台，llama.cpp CPU 后端和独立 Nginx。没有连接已关机的 A10 服务器，没有安装系统服务、Docker VM 或 GPU 推理环境。测试材料位于被 Git 忽略的 `.local-test/`。

- llama.cpp 官方 b12317，commit `e0949d09b`。
- 模型：`bartowski/SmolLM2-135M-Instruct-GGUF`，revision `09816acd5d99df7be770d85ea30822623dab342c`，`SmolLM2-135M-Instruct-Q4_K_M.gguf`。
- 权重 105454432 字节，SHA256 `2e8040ceae7815abe0dcb3540b9995eaa1fa0d2ca9e797d0a635ae4433c68c2d`。
- Nginx 1.28.0，在工作区内编译，使用已有 OpenSSL/pcre2；未注册系统服务。
- CPU 模型限定 2 线程、512 上下文、1 序列；每次生成最多 32 token。

## 结果

`scripts/test-small-model.py` 使用隔离临时状态目录和端口启动真实进程，验证模型列表、文本生成（实际返回 `Hello!`）、SSE 文本及结束标志。随后生成本地测试证书，启动真实 TLS 网关，验证证书链、匿名请求拒绝、授权请求、发布记录与四份客户端导出文件。草稿偏离运行配置后拒绝发布；未启动网关时拒绝发布。测试结束停止自有模型与网关。

最后一次测试的 14 次采样中，各进程最大 RSS：

| 进程 | KiB | 约 MiB |
|---|---:|---:|
| 控制台 | 5472 | 5.3 |
| llama-server | 287984 | 281.2 |
| Nginx | 8624 | 8.4 |

这是短流程进程 RSS 采样，不是压力测试峰值，不包含浏览器、文件缓存或整个操作系统。完整本机证据保存于 `.local-test/real-evidence.json`，不提交机器路径和运行状态文件。

复现命令：

```bash
cargo build --release --locked
python3 scripts/test-small-model.py \
  --llama .local-test/llama-b12317/llama-server \
  --nginx .local-test/nginx-1.28.0/objs/nginx \
  --model .local-test/smollm2-135m-q4.gguf
```

此外通过 fmt、clippy（warnings 为错误）、13 项 Rust 测试、HTTP smoke 测试和协议测试。协议测试使用模拟引擎验证图片/两轮工具调用及失败处理，使用模拟下载源验证 SHA256 清单和损坏文件拒绝，不能替代真实多模态/工具模型测试。安装脚本通过 Bash 语法检查，浏览器脚本通过语法检查；浏览器已打开本地页面，但本次自动截图为空白，未完成新版界面的目视验收。

## 待服务器验证

Ubuntu 22.04/24.04 安装流程、Docker NVIDIA runtime、A10/H20 上 vLLM 的启动及资源隔离、真实目标模型的图片/工具能力、企业 CA、Windows/OpenCode 客户端均未完成实机验证。不能据此宣称 GPU 可用或企业生产验收通过。
