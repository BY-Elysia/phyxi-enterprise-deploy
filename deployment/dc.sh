#!/usr/bin/env bash
set -euo pipefail
cd /srv/phyxi/current
unset DOCKER_HOST DOCKER_CONTEXT DOCKER_TLS_VERIFY DOCKER_TLS DOCKER_API_VERSION DOCKER_CERT_PATH DOCKER_CONFIG
unset MAX_NUM_BATCHED_TOKENS MAX_IMAGES REASONING_EFFORT
unset MAX_MODEL_LEN MAX_NUM_SEQS GPU_MEMORY_UTILIZATION
unset VLLM_IMAGE NGINX_IMAGE PROMETHEUS_IMAGE GPU_UUID MODEL_DIR INTRANET_IP SERVED_MODEL_NAME
unset COMPOSE_FILE COMPOSE_PROJECT_NAME COMPOSE_PROFILES COMPOSE_ENV_FILES
for arg in "$@"; do
 case "$arg" in -f|--file|--file=*|-p|--project-name|--project-name=*|--env-file|--env-file=*|--project-directory|--project-directory=*) exit 2;; esac
done
exec /usr/bin/docker --host unix:///var/run/docker.sock compose \
 --project-name phyxi-enterprise --env-file .env -f compose.yaml "$@"
