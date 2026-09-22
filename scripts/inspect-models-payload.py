"""看各家 /models 端点到底报了哪些能力字段。

这决定了「三层回退」是不是真的必要：如果各家都报得很全，静态兜底就是多余的；
如果报得参差不齐，静态兜底就是刚需。不猜，直接看真实响应。

用法：python scripts/inspect-models-payload.py <某个 /models 响应的 json 文件>
"""
import io
import json
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace")

path = sys.argv[1]
d = json.load(io.open(path, encoding="utf-8"))
arr = d.get("data", d if isinstance(d, list) else [])
print(f"共 {len(arr)} 条\n")

# 统计每个字段的出现率 —— 出现率低的字段不能当依赖
from collections import Counter

top = Counter()
for m in arr:
    if isinstance(m, dict):
        top.update(m.keys())

print("顶层字段出现率：")
for k, n in top.most_common():
    print(f"  {n:5d}/{len(arr):<5d} ({n * 100 // max(len(arr), 1):3d}%)  {k}")

# 关心的能力字段（各家命名不一，全都找一遍）
CARE = [
    "context_length", "context_window", "max_context_length",
    "max_input_tokens", "max_output_tokens", "max_completion_tokens",
    "top_provider", "architecture", "pricing", "supported_parameters",
]
print("\n能力相关字段的实际取值（取第一条有值的）：")
for key in CARE:
    for m in arr:
        if isinstance(m, dict) and m.get(key) is not None:
            v = json.dumps(m[key], ensure_ascii=False)
            print(f"  {key:24} = {v[:120]}")
            break
    else:
        print(f"  {key:24} = （全部为空/不存在）")
