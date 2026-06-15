#!/usr/bin/env bash
set -euo pipefail

VERSION="v0.1.0"
FROM_SOURCE=false
SKIP_BUILD=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        -v|--version) VERSION="v$2"; shift 2 ;;
        --version=*)  VERSION="${1#*=}" ;;
        -s|--from-source) FROM_SOURCE=true; shift ;;
        --skip-build) SKIP_BUILD=true; shift ;;
        -h|--help)
            echo "Usage: install.sh [options]"
            echo ""
            echo "Options:"
            echo "  -v, --version VERSION  Release version to install (default: v0.1.0)"
            echo "  -s, --from-source      Build from source instead of downloading"
            echo "      --skip-build       Skip cargo build (use with -s if already built)"
            echo "  -h, --help             Show this help message"
            exit 0
            ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

NORMALIZE_VERSION="${VERSION#v}"
TAG_VERSION="v$NORMALIZE_VERSION"

SERVER_NAME="opencode-memory"
BINARY_NAME="opencode-memory"
TARGET_DIR="${HOME}/.config/${SERVER_NAME}/bin"
STABLE_EXE="${TARGET_DIR}/${BINARY_NAME}"

ARCH="$(uname -m)"
case "$ARCH" in
    x86_64)  LINUX_ARCH="x86_64-unknown-linux-gnu" ;;
    aarch64) LINUX_ARCH="aarch64-unknown-linux-gnu" ;;
    *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

echo "Installing ${SERVER_NAME} (${LINUX_ARCH})..."

SOURCE_EXE=""

# --- Download release ---
if ! $FROM_SOURCE; then
    ARCHIVE_NAME="${SERVER_NAME}-${TAG_VERSION}-${LINUX_ARCH}.tar.gz"
    URL="https://github.com/stevenke1981/memlong/releases/download/${TAG_VERSION}/${ARCHIVE_NAME}"

    TEMP_DIR="$(mktemp -d "/tmp/${SERVER_NAME}-install-XXXXXX")"
    ARCHIVE_PATH="${TEMP_DIR}/${ARCHIVE_NAME}"

    echo "Attempting to download release version ${TAG_VERSION} from ${URL}..."
    if command -v curl &>/dev/null; then
        curl -fsSL -o "${ARCHIVE_PATH}" "${URL}" || true
    elif command -v wget &>/dev/null; then
        wget -q -O "${ARCHIVE_PATH}" "${URL}" || true
    else
        echo "Error: Neither curl nor wget found. Install one of them or use --from-source."
        exit 1
    fi

    if [[ -f "${ARCHIVE_PATH}" ]]; then
        echo "Extracting ${ARCHIVE_NAME}..."
        tar -xzf "${ARCHIVE_PATH}" -C "${TEMP_DIR}"
        FOUND_BINARY=$(find "${TEMP_DIR}" -type f -name "${BINARY_NAME}" 2>/dev/null | head -1)
        if [[ -n "${FOUND_BINARY}" ]]; then
            SOURCE_EXE="${FOUND_BINARY}"
            echo "Successfully downloaded and extracted binary: ${SOURCE_EXE}"
        else
            echo "Warning: Could not find ${BINARY_NAME} binary in downloaded archive."
        fi
    else
        echo "Warning: Failed to download release asset for ${TAG_VERSION}."
    fi

    if [[ -z "${SOURCE_EXE}" ]]; then
        echo "Error: Could not download release asset for ${TAG_VERSION}."
        echo "Re-run with --from-source on a machine with a Rust toolchain."
        exit 1
    fi
fi

# --- Build from source ---
if $FROM_SOURCE; then
    if ! $SKIP_BUILD; then
        echo "Building ${SERVER_NAME} from source..."
        cargo build --release
        if [[ $? -ne 0 ]]; then
            echo "Error: Cargo build failed."
            exit 1
        fi
    else
        echo "Skipping cargo build as requested (--skip-build)."
    fi

    BUILT_EXE="target/release/memory-mcp-server"
    if [[ ! -f "${BUILT_EXE}" ]]; then
        echo "Error: Built binary not found at ${BUILT_EXE}. Build it first or run without --skip-build."
        exit 1
    fi
    SOURCE_EXE="$(realpath "${BUILT_EXE}")"
    echo "Using compiled binary at ${SOURCE_EXE}"
fi

# --- Ensure target directory exists ---
mkdir -p "${TARGET_DIR}"

# --- Install binary ---
INSTALLED_EXE="${STABLE_EXE}"
IS_STABLE_COPIED=false

echo "Installing executable to target path..."
if cp "${SOURCE_EXE}" "${STABLE_EXE}" 2>/dev/null; then
    chmod +x "${STABLE_EXE}"
    IS_STABLE_COPIED=true
    echo "Copied binary to stable path: ${STABLE_EXE}"
else
    echo "Warning: Could not copy to stable path ${STABLE_EXE} (locked or permission issue)."
    VERSIONED_NAME="${SERVER_NAME}-${TAG_VERSION}"
    VERSIONED_EXE="${TARGET_DIR}/${VERSIONED_NAME}"
    if cp "${SOURCE_EXE}" "${VERSIONED_EXE}" 2>/dev/null; then
        chmod +x "${VERSIONED_EXE}"
        INSTALLED_EXE="${VERSIONED_EXE}"
        echo "Installing versioned binary side-by-side: ${INSTALLED_EXE}"
    else
        TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
        TIMESTAMPED_NAME="${SERVER_NAME}-${TAG_VERSION}-${TIMESTAMP}"
        TIMESTAMPED_EXE="${TARGET_DIR}/${TIMESTAMPED_NAME}"
        cp "${SOURCE_EXE}" "${TIMESTAMPED_EXE}"
        chmod +x "${TIMESTAMPED_EXE}"
        INSTALLED_EXE="${TIMESTAMPED_EXE}"
        echo "Installing timestamped binary side-by-side: ${INSTALLED_EXE}"
    fi
fi

# --- Run install ---
echo "Running configuration installer from ${INSTALLED_EXE}..."
INSTALL_OUTPUT=$("${INSTALLED_EXE}" install --json 2>&1 || true)
INSTALL_EXIT_CODE=$?
if [[ "${INSTALL_EXIT_CODE}" -ne 0 ]]; then
    echo "Error: Installed binary failed to configure system settings (exit code ${INSTALL_EXIT_CODE}): ${INSTALL_OUTPUT}"
    exit 1
fi

INSTALL_REPORT="$(echo "${INSTALL_OUTPUT}" | head -1)"
if ! echo "${INSTALL_REPORT}" | python3 -c "import json,sys;json.load(sys.stdin)" 2>/dev/null; then
    echo "Error: Installed binary did not return a valid JSON install report: ${INSTALL_REPORT}"
    exit 1
fi

REPORTED_EXE="$(echo "${INSTALL_REPORT}" | python3 -c "import json,sys;print(json.load(sys.stdin)['binary_path'])")"
RESOLVED_INSTALLED_EXE="$(realpath "${INSTALLED_EXE}")"
if [[ "${REPORTED_EXE}" != "${RESOLVED_INSTALLED_EXE}" ]]; then
    echo "Error: Install report binary path mismatch. Expected ${RESOLVED_INSTALLED_EXE} but got ${REPORTED_EXE}."
    exit 1
fi

echo "========================================================="
echo "${SERVER_NAME} has been installed successfully!"
echo "Installed Executable Path: ${REPORTED_EXE}"
CONFIGURED_CLIENTS=$(echo "${INSTALL_REPORT}" | python3 -c "
import json,sys
report=json.load(sys.stdin)
for c in report['configured_clients']:
    print(f\"  {c['client']}: {c['path']} [{c['status']}]\")
")
echo "${CONFIGURED_CLIENTS}"
if ! $IS_STABLE_COPIED; then
    echo "Note: Installed as a side-by-side versioned binary because the stable executable was in use."
fi
echo "Please restart your OpenCode or Codex agent to reload changes."
echo "========================================================="
