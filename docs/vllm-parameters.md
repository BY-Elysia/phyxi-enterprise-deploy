# vLLM 推理参数

推理参数方案提供基础参数、高级参数和受校验的扩展参数。表单与服务端校验共用 Rust 参数定义，旧方案没有 `vllm` 字段时等同于未设置附加参数。

## 基础与高级

基础参数保留上下文长度、最大并发、显存比例、解析器及验收选项，并增加计算精度与量化方法。高级选项按显存与缓存、批处理与调度、执行与并行、模型适配分组折叠。

- 所有新增字段可留空，未设置项不会写入启动参数；布尔选项有引擎默认、启用、关闭三态。
- 包含 KV 缓存精度/容量、CPU 卸载/交换空间、前缀缓存、分块预填充、批次 Token 上限、预填充并发、调度策略、流水线/专家并行、Eager、随机种子、Tokenizer、聊天模板、多模态输入上限、RoPE、编译及结构化输出配置。
- TP 由部署时分配的 GPU 数除以 PP（默认 1）推导；要求整除且 TP × PP 等于明确分配的 GPU 数，只支持单机单实例。
- Tokenizer 与模板仅允许 `/model` 挂载内的容器路径，不增加主机挂载，不自动下载。
- JSON 选项按支持的键、类型和范围校验。编译配置不允许指定任意代码后端。具体键可查看 `console/src/vllm.rs`。
- 检查批次 Token 与上下文/并发的约束、预填充并发关系，以及 Eager 与编译配置冲突。部分未指定值取决于引擎版本，最终兼容性由实际镜像和硬件决定。

## 扩展参数

从白名单选择名称并填写有类型的值；当前支持 `block-size`、`max-logprobs`、`max-parallel-loading-workers`、`disable-log-stats`、`disable-sliding-window`、`disable-cascade-attn`。不接受自由 shell 文本或未知开关，不能重复或覆盖基础/高级字段，不能更改模型、端口、GPU、鉴权或远程代码策略。

API 数据示例：

```json
{
  "vllm": {
    "parameters": {
      "dtype": "bfloat16",
      "enable-prefix-caching": false,
      "pipeline-parallel-size": 2,
      "limit-mm-per-prompt": {"image": 4}
    },
    "extra": {"block-size": 16}
  }
}
```

配置保存时校验；部署预览包含完整参数；任务和运行快照保留当时的值。修改方案不改变运行实例，需重新部署。所有参数通过独立 argv 项传入进程，不使用 shell 拼接。

非 vLLM 引擎拒绝非空 vLLM 参数。表单切换引擎时隐藏专属字段并不提交这些字段。

## 版本与验证边界

选项以预设 vLLM 0.11.0 为主要文档基线：[官方引擎参数](https://docs.vllm.ai/en/v0.11.0/configuration/engine_args.html)。本控制台做类型、范围及已知组合校验，不声称覆盖 vLLM 全部 CLI，不会仅凭镜像标签宣称兼容。自定义镜像及特殊模型需在目标服务器确认支持；没有进行真实 GPU 推理验收。

## 每次请求的生成参数

`temperature`、`top_p`、`max_tokens`、`stop` 等由客户端随请求发送。客户端工作区已提供“生成预设”，导出客户端配置时携带请求默认值；详见 [客户端生成预设](generation-presets.md)。未来的在线调试页可提供按请求覆盖。若将来需要限制最大输出或请求预算，应归入网关策略，并明确是强制约束而非客户端默认值。
