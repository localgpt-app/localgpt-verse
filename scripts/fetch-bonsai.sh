#!/usr/bin/env bash
# Fetch the LLM for the `llm` feature (PLAN.md M7, src/llm.rs).
#
# Default model: Bonsai-8B 1-bit GGUF (Q1_0) — Apache-2.0 (prism-ml), the
# largest text-only Bonsai. Q1_0 is merged into upstream llama.cpp, so
# mistral.rs (which tracks upstream) can load it. The ternary (Q2_0) Bonsai
# models are NOT supported (Prism fork only) — do not fetch those for this
# app.
#
# The GGUF repo ships no tokenizer.json, and mistral.rs's GgufModelBuilder
# requires one — it comes from the unpacked repo (same Apache-2.0 release).
#
# Fallback (documented, not automatic): if the 1-bit model underperforms or
# fails to load, drop any standard Q4_K_M GGUF (e.g. Qwen2.5-7B-Instruct,
# Llama-3.1-8B-Instruct) + its matching tokenizer.json into assets/llm/
# instead — src/llm.rs discovers the first .gguf it finds, so no code change
# is needed.
set -euo pipefail
cd "$(dirname "$0")/.."

OUT="assets/llm"
mkdir -p "$OUT"

# Defaults; override with BONSAI_REPO / BONSAI_FILE / BONSAI_TOKENIZER_REPO.
GGUF_REPO="${BONSAI_REPO:-prism-ml/Bonsai-8B-gguf}"
GGUF_FILE="${BONSAI_FILE:-Bonsai-8B-Q1_0.gguf}"
TOK_REPO="${BONSAI_TOKENIZER_REPO:-prism-ml/Bonsai-8B-unpacked}"

fetch() { # <url> <dest>
  local url="$1" dest="$2"
  if [ -s "$dest" ]; then
    echo "  have $dest"
  else
    echo "fetching $url"
    # -C - resumes a partial download; -L follows HF redirects.
    curl -L --fail -C - -o "$dest" "$url"
  fi
}

fetch "https://huggingface.co/${GGUF_REPO}/resolve/main/${GGUF_FILE}" \
  "$OUT/$(basename "$GGUF_FILE")"
fetch "https://huggingface.co/${TOK_REPO}/resolve/main/tokenizer.json" \
  "$OUT/tokenizer.json"

echo "done -> $OUT"
echo "license: Bonsai-8B weights + tokenizer are Apache-2.0 (prism-ml);"
echo "        still verify NOTICE.txt before distribution."
