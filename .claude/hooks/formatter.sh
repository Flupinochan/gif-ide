#!/bin/bash
input=$(cat)
file_path=$(jq -r '.tool_input.file_path // empty' <<<"$input")

case "$file_path" in
  *.rs|*.slint) ;;
  *) exit 0 ;;
esac

cd "$CLAUDE_PROJECT_DIR" || exit 0

combined=""

fmt_output=$(cargo fmt --all 2>&1)
fmt_status=$?
[ "$fmt_status" -ne 0 ] && combined+="$fmt_output"$'\n'

clippy_output=$(cargo clippy --all-targets -- -D warnings 2>&1)
clippy_status=$?
[ "$clippy_status" -ne 0 ] && combined+="$clippy_output"$'\n'

check_output=$(cargo check 2>&1)
check_status=$?
[ "$check_status" -ne 0 ] && combined+="$check_output"$'\n'

if [ -n "$combined" ]; then
  jq -n --arg reason "$combined" '{decision: "block", reason: $reason}'
fi

exit 0
