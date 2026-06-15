#!/usr/bin/env bash
set -euo pipefail

VERSION=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        -v|--version) VERSION="$2"; shift 2 ;;
        --version=*)  VERSION="${1#*=}" ;;
        -h|--help)
            echo "Usage: package-release.sh [--version <semver>]"
            exit 0
            ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

# Determine version from Cargo.toml if not provided
if [[ -z "${VERSION}" ]]; then
    if [[ -f "Cargo.toml" ]]; then
        VERSION=$(sed -n 's/^version\s*=\s*"\([^"]*\)"/\1/p' Cargo.toml | head -1)
    fi
fi

if [[ -z "${VERSION}" ]]; then
    VERSION="0.1.0"
    echo "Warning: Could not determine version, defaulting to ${VERSION}" >&2
fi

RAW_VERSION="${VERSION#v}"
TAG_VERSION="v${RAW_VERSION}"

echo "Packaging opencode-memory version ${TAG_VERSION} (${RAW_VERSION})..."

# Detect architecture
ARCH="$(uname -m)"
case "$ARCH" in
    x86_64)  TARGET_TRIPLE="x86_64-unknown-linux-gnu" ;;
    aarch64) TARGET_TRIPLE="aarch64-unknown-linux-gnu" ;;
    *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

# 1. Build release binaries
echo "Building release binaries (${TARGET_TRIPLE})..."
cargo build --release
echo "Cargo build succeeded."

# 2. Setup packaging directories
PACKAGING_DIR="target/packaging"
SUB_DIR="${PACKAGING_DIR}/opencode-memory"

rm -rf "${PACKAGING_DIR}"
mkdir -p "${SUB_DIR}/skills"

# 3. Copy files
echo "Copying binary..."
cp "target/release/memory-mcp-server" "${SUB_DIR}/opencode-memory"
chmod +x "${SUB_DIR}/opencode-memory"

if [[ -f "skills/memory-extraction.md" ]]; then
    echo "Copying skill file..."
    cp "skills/memory-extraction.md" "${SUB_DIR}/skills/"
fi

# 4. Create tar.gz archive
ARCHIVE_NAME="opencode-memory-${TAG_VERSION}-${TARGET_TRIPLE}.tar.gz"
ARCHIVE_PATH="target/${ARCHIVE_NAME}"

rm -f "${ARCHIVE_PATH}"

echo "Creating archive ${ARCHIVE_PATH}..."
tar -czf "${ARCHIVE_PATH}" -C "${PACKAGING_DIR}" opencode-memory/

# 5. Compute SHA256 checksum
echo "Computing SHA256 checksum..."
HASH=$(sha256sum "${ARCHIVE_PATH}" | cut -d' ' -f1)
echo "${HASH}  ${ARCHIVE_NAME}" > "${ARCHIVE_PATH}.sha256"

echo "========================================================="
echo "Packaging completed successfully!"
echo "Archive: ${ARCHIVE_PATH}"
echo "SHA256:  ${HASH}"
echo "Checksum File: ${ARCHIVE_PATH}.sha256"
echo "========================================================="
