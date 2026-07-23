#!/usr/bin/env bash
# Fetch the LLM for the `llm` feature (PLAN.md M7, src/llm.rs).
#
# Default model: Bonsai-8B 1-bit GGUF (Q1_0, g128) — the largest text-only
# Bonsai, ~3.5 GB. Q1_0 is merged into upstream llama.cpp, so mistral.rs (which
# tracks upstream) can load it. The ternary (Q2_0) Bonsai models are NOT
# supported (Prism fork only) — do not fetch those for this app.
#
# Repository: https://huggingface.co/prism-ml/Bonsai-8B-gguf
# License: CHECK the model card before any distribution — verify terms against
#   your distribution model (the CLAP weights used by the `ml` feature are
#   CC-BY-NC; Bonsai terms differ and must be confirmed). The app runs fully
#   without this model (falls back to rule-derived recipes).
#
# Fallback (documented, not automatic): if the 1-bit model underperforms or
#   fails to load, drop any standard Q4_K_M GGUF (e.g. Qwen2.5-7B-Instruct,
#   Llama-3.1-8B-Instruct) + its matching tokenizer.json into assets/llm/
#   instead — src/llm.rs discovers the first .gguf it finds, so no code change
#   is needed.
set -euo pipefail
cd "$(dirname "$0")/.."

OUT="assets/llm"
mkdir -p "$OUT"

# Default: Bonsai-8B 1-bit. Override with BONSAI_FILE / BONSAI_REPO.
REPO="${BONSAI_REPO:-prism-ml/Bonsai-8B-gguf}"
FILE="${BONSAI_FILE:-bonsai-8b-1bit-q1_0.gguf}"
URL="https://huggingface.co/${REPO}/resolve/main/${FILE}"
DEST="$OUT/$(basename "$FILE")"

if [ -s "$DEST" ]; then
  echo "  have $DEST"
else
  echo "fetching $URL"
  echo "  (~3.5 GB; resumable — re-run if interrupted)"
  # -C - resumes a partial download; -L follows HF redirects.
  curl -L --fail -C - -o "$DEST" "$URL"
fi

echo "done -> $DEST"
echo "note: a matching tokenizer.json is also required in $OUT/. Bonsai GGUF"
echo "      embeds the chat template; the tokenizer file ships alongside the"
echo "      model card — copy it into $OUT/ if src/llm.rs reports it missing."
