#!/usr/bin/env bash
# Use the repo's hooks (tools/hooks) in this clone and its worktrees.
set -euo pipefail
cd "$(dirname "$0")/.."
git config core.hooksPath tools/hooks
echo "hooks: core.hooksPath = tools/hooks"
