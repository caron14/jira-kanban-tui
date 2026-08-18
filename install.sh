#!/bin/sh

set -eu

REPOSITORY="caron14/jira-kanban-tui"
BINARY_NAME="jira-kanban-tui"
VERSION="${JIRA_KANBAN_TUI_VERSION:-latest}"
INSTALL_DIR="${JIRA_KANBAN_TUI_INSTALL_DIR:-${HOME:?HOME must be set}/.local/bin}"
TEMP_DIR=""
STAGED_BINARY=""

fail() {
    printf '%s\n' "error: $*" >&2
    exit 1
}

cleanup() {
    if [ -n "$STAGED_BINARY" ]; then
        rm -f "$STAGED_BINARY"
    fi
    if [ -n "$TEMP_DIR" ]; then
        rm -rf "$TEMP_DIR"
    fi
}

trap cleanup 0 HUP INT TERM

for command_name in curl tar mktemp uname mkdir cp chmod mv awk rm; do
    command -v "$command_name" >/dev/null 2>&1 || fail "required command not found: $command_name"
done

case "$(uname -s)" in
    Darwin) operating_system="apple-darwin" ;;
    Linux) operating_system="unknown-linux-musl" ;;
    *) fail "unsupported operating system: $(uname -s)" ;;
esac

case "$(uname -m)" in
    x86_64 | amd64) architecture="x86_64" ;;
    arm64 | aarch64) architecture="aarch64" ;;
    *) fail "unsupported architecture: $(uname -m)" ;;
esac

asset="${BINARY_NAME}-${architecture}-${operating_system}.tar.gz"

case "$VERSION" in
    latest)
        release_url="https://github.com/${REPOSITORY}/releases/latest/download"
        ;;
    *)
        valid_version=$(awk -v version="$VERSION" 'BEGIN {
            print version ~ /^v?[0-9]+\.[0-9]+\.[0-9]+$/
        }')
        [ "$valid_version" -eq 1 ] || fail "invalid version: ${VERSION} (expected latest or X.Y.Z)"
        version_number=${VERSION#v}
        release_url="https://github.com/${REPOSITORY}/releases/download/v${version_number}"
        ;;
esac

TEMP_DIR=$(mktemp -d "${TMPDIR:-/tmp}/${BINARY_NAME}.XXXXXX")
archive_path="${TEMP_DIR}/${asset}"
checksums_path="${TEMP_DIR}/checksums.txt"
unpack_dir="${TEMP_DIR}/unpacked"

printf '%s\n' "Downloading ${BINARY_NAME} (${architecture}-${operating_system})..."
curl -fsSL "${release_url}/${asset}" -o "$archive_path" || fail "could not download ${asset}"
curl -fsSL "${release_url}/checksums.txt" -o "$checksums_path" || fail "could not download checksums.txt"

expected_checksum=$(awk -v asset="$asset" '
    $2 == asset && length($1) == 64 && $1 ~ /^[[:xdigit:]]+$/ { print $1 }
' "$checksums_path")
checksum_count=$(awk -v asset="$asset" '
    $2 == asset && length($1) == 64 && $1 ~ /^[[:xdigit:]]+$/ { count += 1 }
    END { print count + 0 }
' "$checksums_path")
[ "$checksum_count" -eq 1 ] || fail "checksums.txt does not contain exactly one checksum for ${asset}"

if command -v sha256sum >/dev/null 2>&1; then
    actual_checksum=$(sha256sum "$archive_path" | awk '{ print $1 }')
elif command -v shasum >/dev/null 2>&1; then
    actual_checksum=$(shasum -a 256 "$archive_path" | awk '{ print $1 }')
else
    fail "sha256sum or shasum is required to verify the download"
fi

[ "$actual_checksum" = "$expected_checksum" ] || fail "checksum verification failed for ${asset}"

mkdir -p "$unpack_dir"
tar -xzf "$archive_path" -C "$unpack_dir"
downloaded_binary="${unpack_dir}/${BINARY_NAME}"
[ -f "$downloaded_binary" ] || fail "archive does not contain ${BINARY_NAME}"
chmod 0755 "$downloaded_binary"
reported_version=$("$downloaded_binary" --version 2>/dev/null) || fail "downloaded binary could not be executed"
if [ "$VERSION" != "latest" ]; then
    [ "$reported_version" = "${BINARY_NAME} ${version_number}" ] || \
        fail "downloaded binary reports an unexpected version: ${reported_version}"
fi

mkdir -p "$INSTALL_DIR" || fail "could not create install directory: ${INSTALL_DIR}"
[ -d "$INSTALL_DIR" ] || fail "install destination is not a directory: ${INSTALL_DIR}"
STAGED_BINARY="${INSTALL_DIR}/.${BINARY_NAME}.tmp.$$"
cp "$downloaded_binary" "$STAGED_BINARY" || fail "could not stage binary in ${INSTALL_DIR}"
chmod 0755 "$STAGED_BINARY"
mv -f "$STAGED_BINARY" "${INSTALL_DIR}/${BINARY_NAME}" || fail "could not install binary in ${INSTALL_DIR}"
STAGED_BINARY=""

printf '%s\n' "Installed ${BINARY_NAME} to ${INSTALL_DIR}/${BINARY_NAME}"
case ":${PATH:-}:" in
    *:"${INSTALL_DIR}":*) ;;
    *)
        printf '%s\n' "Add ${INSTALL_DIR} to PATH before running ${BINARY_NAME}:"
        printf '  export PATH="%s:$PATH"\n' "$INSTALL_DIR"
        ;;
esac
