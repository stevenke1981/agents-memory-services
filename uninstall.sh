#!/usr/bin/env bash
set -euo pipefail

SERVER_NAME="opencode-memory"
CONFIG_DIR="${HOME}/.config/${SERVER_NAME}"
BIN_DIR="${CONFIG_DIR}/bin"
DATA_HOME="${XDG_DATA_HOME:-${HOME}/.local/share}/${SERVER_NAME}"
STATE_HOME="${XDG_STATE_HOME:-${HOME}/.local/state}/${SERVER_NAME}"

REMOVE_DATA=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --remove-data) REMOVE_DATA=true; shift ;;
        -h|--help)
            echo "Usage: uninstall.sh [options]"
            echo ""
            echo "Options:"
            echo "  --remove-data     Also remove all stored memory data (default: keep)"
            echo "  -h, --help        Show this help message"
            exit 0
            ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

echo "Uninstalling ${SERVER_NAME}..."

# 1. Remove MCP server entries from opencode config
for CONFIG_PATH in "${HOME}/.config/opencode/opencode.jsonc" "${HOME}/.config/opencode/opencode.json"; do
    if [[ -f "${CONFIG_PATH}" ]]; then
        echo "Removing ${SERVER_NAME} from ${CONFIG_PATH}..."
        if command -v python3 &>/dev/null; then
            python3 -c "
import json, sys
try:
    with open('${CONFIG_PATH}') as f:
        content = f.read()
    # Handle JSONC (strip comments)
    lines = []
    in_string = False
    for line in content.split('\n'):
        stripped = line.strip()
        if stripped.startswith('//') or stripped.startswith('/*'):
            continue
        lines.append(line)
    config = json.loads('\n'.join(lines))
except:
    config = json.loads(content) if content.strip() else {}

if 'mcp' in config and isinstance(config['mcp'], dict):
    removed = False
    for name in list(config['mcp'].keys()):
        if name == '${SERVER_NAME}' or name in ('memory-mcp-server', 'memory-mcp', 'memlong-memory'):
            del config['mcp'][name]
            removed = True
    if removed:
        with open('${CONFIG_PATH}', 'w') as f:
            json.dump(config, f, indent=2)
        print('  Removed entries')
    else:
        print('  No matching entries found')
" 2>&1 || echo "  Warning: Could not update ${CONFIG_PATH}"
        fi
    fi
done

# 2. Remove MCP server entries from codex config
CODEX_PATHS=(
    "${HOME}/.codex/config.toml"
    "${HOME}/.claude/.codex/config.toml"
)
for CONFIG_PATH in "${CODEX_PATHS[@]}"; do
    if [[ -f "${CONFIG_PATH}" ]]; then
        echo "Removing ${SERVER_NAME} from ${CONFIG_PATH}..."
        if command -v python3 &>/dev/null; then
            python3 -c "
import re
with open('${CONFIG_PATH}') as f:
    content = f.read()

names = ['${SERVER_NAME}', 'memory-mcp-server', 'memory-mcp', 'memlong-memory']
for name in names:
    pattern = re.compile(
        r'^\\[mcp_servers\\.' + re.escape(name) + r'(\\..*)?\\][^\\[]*',
        re.MULTILINE
    )
    content = pattern.sub('', content)

# Clean up blank lines
content = re.sub(r'\\n{3,}', '\\n\\n', content)
content = content.strip()

with open('${CONFIG_PATH}', 'w') as f:
    f.write(content + '\\n' if content else '')
print('  Removed entries')
" 2>&1 || echo "  Warning: Could not update ${CONFIG_PATH}"
        fi
    fi
done

# 3. Remove binary directory
if [[ -d "${BIN_DIR}" ]]; then
    echo "Removing binary directory: ${BIN_DIR}"
    rm -rf "${BIN_DIR}"
fi

# 4. Remove empty config directory
if [[ -d "${CONFIG_DIR}" ]]; then
    rmdir "${CONFIG_DIR}" 2>/dev/null || true
fi

# 5. Optionally remove data directories
if $REMOVE_DATA; then
    for DIR in "${DATA_HOME}" "${STATE_HOME}"; do
        if [[ -d "${DIR}" ]]; then
            echo "Removing data directory: ${DIR}"
            rm -rf "${DIR}"
        fi
    done
    echo "All memory data has been removed."
else
    echo "Memory data preserved at:"
    echo "  ${DATA_HOME}"
    echo "  ${STATE_HOME}"
    echo "Re-run with --remove-data to delete stored memories."
fi

echo ""
echo "${SERVER_NAME} has been uninstalled."
echo "Restart your OpenCode or Codex agent to apply changes."
