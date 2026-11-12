#!/usr/bin/env bash
set -euo pipefail
export VLLM_API_KEY="$(cat /backend-key)"
exec vllm serve /model \
 --served-model-name "$SERVED_MODEL_NAME" --host 0.0.0.0 --port 8000 \
 --tokenizer-mode auto --dtype bfloat16 --kv-cache-dtype auto \
 --tensor-parallel-size "$TP_SIZE" \
 --max-model-len "$MAX_MODEL_LEN" --gpu-memory-utilization "$GPU_MEMORY_UTILIZATION" \
 --max-num-seqs "$MAX_NUM_SEQS" --max-num-batched-tokens "$MAX_NUM_BATCHED_TOKENS" \
 --enable-chunked-prefill --enable-prefix-caching \
 --disable-flashinfer-autotune \
 --limit-mm-per-prompt "{\"image\":${MAX_IMAGES},\"video\":0}" \
 --enable-auto-tool-choice --tool-call-parser glm47 --reasoning-parser glm47 \
 --default-chat-template-kwargs "{\"reasoning_effort\":\"${REASONING_EFFORT}\"}" \
 --generation-config vllm --disable-fastapi-docs
