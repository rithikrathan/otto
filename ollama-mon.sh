#!/usr/bin/env bash
# ollama-mon.sh — live tokens/s per loaded model.
# line per model from /api/ps, each probed (round-robin) via /api/generate.
# env: OLLAMA_MON_HOST / _INTERVAL (2) / _TOKENS (16) / _PROBE_TIMEOUT (30)
#      OLLAMA_MON_SEEN (~/.cache/ollama-seen-models)
# keys: q quit · space pause · r now · +/− poll speed
set -euo pipefail

HOST="${OLLAMA_MON_HOST:-http://localhost:11434}"
IV="${OLLAMA_MON_INTERVAL:-2}"
TOK="${OLLAMA_MON_TOKENS:-16}"
TMO="${OLLAMA_MON_PROBE_TIMEOUT:-30}"
SEENF="${OLLAMA_MON_SEEN:-${XDG_CACHE_HOME:-$HOME/.cache}/ollama-seen-models}"
MAXW=44

ONCE=0
for a in "$@"; do [[ "$a" == "--once" ]] && ONCE=1; done

TTY=0; [[ -t 1 ]] && TTY=1
if [[ $TTY -eq 1 ]]; then
  C_RST=$'\e[0m' C_BLD=$'\e[1m' C_GRN=$'\e[32m' C_RED=$'\e[31m' C_DIM=$'\e[2m'
else
  C_RST='' C_BLD='' C_GRN='' C_RED='' C_DIM=''
fi

declare -A TPS
models=()
seen=()
fail=0
rot=0
prevlist=""

mkdir -p "$(dirname "$SEENF")" 2>/dev/null || true
if [[ -f "$SEENF" ]]; then while IFS= read -r s; do [[ -n "$s" ]] && seen+=("$s"); done < "$SEENF"; fi

track() { # $1 name -> append if not already seen
  local s
  for s in "${seen[@]:-}"; do [[ "$s" == "$1" ]] && return 0; done
  seen+=("$1")
  printf '%s\n' "$1" >> "$SEENF"
  if (( ${#seen[@]} > 60 )); then
    seen=("${seen[@]: -40}")
    printf '%s\n' "${seen[@]}" > "$SEENF"
  fi
}

probe() { # $1 name -> TPS[$1]
  local r v
  r=$(curl -m "$TMO" -sf "$HOST/api/generate" -X POST \
      -d "$(jq -nc --arg m "$1" --argjson n "$TOK" \
            '{model:$m,prompt:"hi",stream:false,options:{num_predict:$n,temperature:0}}')") \
    || { TPS[$1]="…"; return 1; }
  v=$(jq -r 'if (.eval_duration // 0) > 0 then .eval_count * 1e9 / .eval_duration else empty end' <<<"$r") || v=""
  if [[ "$v" =~ ^[0-9]+([.][0-9]+)?$ ]]; then TPS[$1]=$(printf '%.1f' "$v"); else TPS[$1]="…"; fi
}

poll() {
  local names n m resp
  resp=$(curl -m 8 -sf "$HOST/api/ps" 2>/dev/null) || resp=""
  if [[ -n "$resp" ]]; then
    fail=0
    names=$(jq -r '.models[].name' <<<"$resp" 2>/dev/null) || names=""
    mapfile -t models <<< "$names"
    local i
    for i in "${!models[@]}"; do
      if [[ -z "${models[$i]}" ]]; then unset 'models[$i]'; fi
    done
    if (( ${#models[@]} )); then
      if [[ "$names" != "$prevlist" ]]; then
        for n in "${models[@]}"; do track "$n"; done
        for n in "${!TPS[@]}"; do
          local keep=0
          for m in "${models[@]}"; do [[ "$m" == "$n" ]] && keep=1 && break; done
          if (( keep == 0 )); then unset 'TPS[$n]'; fi
        done
        for n in "${models[@]}"; do [[ -n "${TPS[$n]:-}" ]] || TPS[$n]="…"; done
        prevlist="$names"
      fi
      rot=$(( (rot + 1) % ${#models[@]} ))
      probe "${models[$rot]}" || true
    else
      models=(); prevlist=""
    fi
  else
    fail=$(( fail + 1 ))
    if (( fail >= 2 )); then models=(); prevlist=""; fi
  fi
}

draw() {
  local m out=() s
  if (( fail >= 2 )); then
    out+=("${C_RED}server unreachable — retrying${C_RST}")
  elif (( ${#models[@]} == 0 )); then
    out+=("${C_DIM}no model loaded${C_RST}")
  else
    for m in "${models[@]}"; do
      out+=("${C_BLD}${m:0:38}${C_RST}  ${C_GRN}${TPS[$m]:-…}${C_RST} t/s")
    done
    if (( ${#seen[@]} )); then
      s="${seen[*]}"; s="seen: ${s:0:$((MAXW - 6))}"
      out+=("${C_DIM}$s${C_RST}")
    fi
  fi
  if [[ $TTY -eq 1 ]]; then
    printf '\e[H'
    for m in "${out[@]:0:11}"; do printf '\e[K%s\n' "$m"; done
    printf '\e[J'
  else
    for m in "${out[@]}"; do printf '%s\n' "$m"; done
  fi
}

[[ $TTY -eq 1 ]] && { printf '\e[?25l'; trap 'printf "\e[0m\e[?25h\n" >&2; exit 0' INT TERM EXIT; }

if (( ONCE )); then poll; draw; exit 0; fi

while :; do
  poll
  draw
  if [[ -t 0 ]]; then
    k=""
    IFS= read -r -s -n1 -t "$IV" k || true
    case "$k" in
      q|Q) break ;;
      ' ') while :; do IFS= read -r -s -n1 k || true; case "$k" in q|Q) exit 0 ;; ' ') break ;; esac; done ;;
      r|R) poll ;;
      +|=) IV=$(( IV > 1 ? IV - 1 : 1 )) ;;
      -)  IV=$(( IV + 1 )) ;;
    esac
  else
    sleep "$IV"
  fi
done