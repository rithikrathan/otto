#!/bin/bash

SESSION="otto"
TARGET_DIR=$(pwd)

tmux kill-session -t "$SESSION" 2>/dev/null

# editor window
tmux new-session -d -s "$SESSION" -n "editor" -c "$TARGET_DIR"
tmux send-keys -t "$SESSION:editor" "nvim" C-m

# ollama server window (guard: only start if port 11434 is not answering yet)
tmux new-window -t "$SESSION" -n "[ollama-server]" -c "/mnt/sda4/OllamaLive"
if ! curl -sf --max-time 2 http://localhost:11434/api/version >/dev/null 2>&1; then
  tmux send-keys -t "$SESSION:[ollama-server]" "./run-linux.sh" C-m
fi

tmux new-window -t "$SESSION" -n "[otto]" -c "$TARGET_DIR/target/debug"
tmux split-window -h -t "$SESSION:[otto]"   -p 80 -c "$TARGET_DIR"
tmux split-window -t "$SESSION:[otto].2"    -p 75 -c "$TARGET_DIR"
tmux split-window -h -t "$SESSION:[otto].2" -p 80 -c "$TARGET_DIR"

tmux send-keys -t "$SESSION:[otto].1" "./otto" C-m
tmux send-keys -t "$SESSION:[otto].2" "vim TODO.md" C-m
tmux send-keys -t "$SESSION:[otto].3" "./ollama-mon.sh" C-m
tmux send-keys -t "$SESSION:[otto].4" "btop" C-m

# lazygit window
tmux new-window -t "$SESSION" -n "lazygit" -c "$TARGET_DIR"
tmux send-keys -t "$SESSION:lazygit" "lazygit" C-m

tmux select-window -t "$SESSION:editor"
tmux attach-session -t "$SESSION"
