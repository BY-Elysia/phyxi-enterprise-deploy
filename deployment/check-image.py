"""Run inside the pinned image; no model weights are loaded."""
import importlib.metadata as md
import json
from packaging.version import Version
from vllm.model_executor.models import ModelRegistry
from vllm.tool_parsers import ToolParserManager
from vllm.reasoning import ReasoningParserManager
versions={x:md.version(x) for x in ['vllm','torch','transformers','flashinfer-python'] if x!='flashinfer-python'}
try:versions['flashinfer']=md.version('flashinfer-python')
except md.PackageNotFoundError:versions['flashinfer']=md.version('flashinfer')
assert Version(versions['vllm'])>=Version('0.29.0'), versions
assert Version(versions['flashinfer'])>=Version('0.6.17'), versions
assert 'Glm5NextForConditionalGeneration' in ModelRegistry.get_supported_archs(), 'GLM5Next architecture missing'
ToolParserManager.get_tool_parser('glm47')
ReasoningParserManager.get_reasoning_parser('glm47')
print(json.dumps({'versions':versions,'architecture':'Glm5NextForConditionalGeneration','tool_parser':'glm47','reasoning_parser':'glm47','frontend':'Python'},indent=2))
