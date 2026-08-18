#!/bin/sh

set -eu

REPOSITORY_ROOT=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
TEST_ROOT=$(mktemp -d "${TMPDIR:-/tmp}/jira-kanban-tui-installer-test.XXXXXX")

cleanup() {
    rm -rf "$TEST_ROOT"
}

trap cleanup 0 HUP INT TERM

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

checksum() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{ print $1 }'
    else
        shasum -a 256 "$1" | awk '{ print $1 }'
    fi
}

FIXTURES="$TEST_ROOT/fixtures"
FAKE_BIN="$TEST_ROOT/fake-bin"
mkdir -p "$FIXTURES" "$FAKE_BIN"

cat >"$FAKE_BIN/uname" <<'EOF'
#!/bin/sh
case "$1" in
    -s) printf '%s\n' "${TEST_UNAME_S}" ;;
    -m) printf '%s\n' "${TEST_UNAME_M}" ;;
    *) exit 1 ;;
esac
EOF

cat >"$FAKE_BIN/curl" <<'EOF'
#!/bin/sh
url=""
output=""
while [ "$#" -gt 0 ]; do
    case "$1" in
        -o)
            output="$2"
            shift 2
            ;;
        -*) shift ;;
        *)
            url="$1"
            shift
            ;;
    esac
done
printf '%s\n' "$url" >>"$FAKE_CURL_LOG"
name=${url##*/}
if [ "${FAKE_DOWNLOAD_FAILURE:-}" = "$name" ]; then
    exit 22
elif [ "$name" = "checksums.txt" ] && [ "${FAKE_BAD_CHECKSUM:-0}" = "1" ]; then
    awk '{ printf "%064d  %s\n", 0, $2 }' "$FIXTURE_DIR/checksums.txt" >"$output"
elif [ "$name" = "checksums.txt" ] && [ "${FAKE_DUPLICATE_CHECKSUM:-0}" = "1" ]; then
    cp "$FIXTURE_DIR/checksums.txt" "$output"
    awk 'NR == 1 { print }' "$FIXTURE_DIR/checksums.txt" >>"$output"
else
    cp "$FIXTURE_DIR/$name" "$output"
fi
EOF

chmod +x "$FAKE_BIN/uname" "$FAKE_BIN/curl"

for target in \
    x86_64-apple-darwin \
    aarch64-apple-darwin \
    x86_64-unknown-linux-musl \
    aarch64-unknown-linux-musl
do
    package_dir="$TEST_ROOT/package-$target"
    mkdir -p "$package_dir"
    cat >"$package_dir/jira-kanban-tui" <<'EOF'
#!/bin/sh
printf '%s\n' 'jira-kanban-tui 0.1.1'
EOF
    chmod +x "$package_dir/jira-kanban-tui"
    cp "$REPOSITORY_ROOT/README.md" "$REPOSITORY_ROOT/LICENSE" "$package_dir/"
    archive="jira-kanban-tui-${target}.tar.gz"
    tar -czf "$FIXTURES/$archive" -C "$package_dir" jira-kanban-tui README.md LICENSE
    printf '%s  %s\n' "$(checksum "$FIXTURES/$archive")" "$archive" >>"$FIXTURES/checksums.txt"
done

run_installer() {
    test_home="$1"
    shift
    env \
        HOME="$test_home" \
        PATH="$FAKE_BIN:/usr/bin:/bin" \
        FIXTURE_DIR="$FIXTURES" \
        FAKE_CURL_LOG="$TEST_ROOT/curl.log" \
        "$@" \
        sh "$REPOSITORY_ROOT/install.sh"
}

assert_installs_target() {
    os="$1"
    architecture="$2"
    expected_target="$3"
    home="$TEST_ROOT/home-$expected_target"
    output=$(run_installer "$home" TEST_UNAME_S="$os" TEST_UNAME_M="$architecture")
    [ -x "$home/.local/bin/jira-kanban-tui" ] || fail "did not install $expected_target"
    "$home/.local/bin/jira-kanban-tui" --version | grep -q 'jira-kanban-tui 0.1.1' || \
        fail "installed binary for $expected_target did not run"
    printf '%s\n' "$output" | grep -q 'Add .*\.local/bin to PATH' || \
        fail "did not print PATH guidance for $expected_target"
}

assert_installs_target Darwin x86_64 x86_64-apple-darwin
assert_installs_target Darwin arm64 aarch64-apple-darwin
assert_installs_target Linux amd64 x86_64-unknown-linux-musl
assert_installs_target Linux aarch64 aarch64-unknown-linux-musl

custom_dir="$TEST_ROOT/custom bin"
run_installer "$TEST_ROOT/custom-home" \
    TEST_UNAME_S=Darwin \
    TEST_UNAME_M=arm64 \
    JIRA_KANBAN_TUI_INSTALL_DIR="$custom_dir" \
    JIRA_KANBAN_TUI_VERSION=0.1.1 >/dev/null
[ -x "$custom_dir/jira-kanban-tui" ] || fail "custom install directory was ignored"
tail -n 2 "$TEST_ROOT/curl.log" | grep -q '/releases/download/v0.1.1/' || \
    fail "version without v prefix was not normalized"

run_installer "$TEST_ROOT/version-home" \
    TEST_UNAME_S=Darwin \
    TEST_UNAME_M=arm64 \
    JIRA_KANBAN_TUI_VERSION=v0.1.1 >/dev/null
tail -n 2 "$TEST_ROOT/curl.log" | grep -q '/releases/download/v0.1.1/' || \
    fail "version with v prefix was changed"

existing_home="$TEST_ROOT/existing-home"
mkdir -p "$existing_home/.local/bin"
printf '%s\n' 'existing binary' >"$existing_home/.local/bin/jira-kanban-tui"
if run_installer "$existing_home" \
    TEST_UNAME_S=Linux \
    TEST_UNAME_M=x86_64 \
    FAKE_BAD_CHECKSUM=1 >/dev/null 2>&1
then
    fail "installer accepted an invalid checksum"
fi
grep -q '^existing binary$' "$existing_home/.local/bin/jira-kanban-tui" || \
    fail "failed install replaced the existing binary"

if run_installer "$existing_home" \
    TEST_UNAME_S=Linux \
    TEST_UNAME_M=x86_64 \
    FAKE_DOWNLOAD_FAILURE=jira-kanban-tui-x86_64-unknown-linux-musl.tar.gz >/dev/null 2>&1
then
    fail "installer ignored a download failure"
fi
grep -q '^existing binary$' "$existing_home/.local/bin/jira-kanban-tui" || \
    fail "download failure replaced the existing binary"

if run_installer "$TEST_ROOT/duplicate-home" \
    TEST_UNAME_S=Darwin \
    TEST_UNAME_M=x86_64 \
    FAKE_DUPLICATE_CHECKSUM=1 >/dev/null 2>&1
then
    fail "installer accepted duplicate checksums"
fi

blocked_destination="$TEST_ROOT/not-a-directory"
printf '%s\n' 'file' >"$blocked_destination"
if run_installer "$TEST_ROOT/blocked-home" \
    TEST_UNAME_S=Darwin \
    TEST_UNAME_M=x86_64 \
    JIRA_KANBAN_TUI_INSTALL_DIR="$blocked_destination" >/dev/null 2>&1
then
    fail "installer accepted an invalid install directory"
fi

if run_installer "$TEST_ROOT/unsupported-home" \
    TEST_UNAME_S=FreeBSD \
    TEST_UNAME_M=x86_64 >/dev/null 2>&1
then
    fail "installer accepted an unsupported operating system"
fi

if run_installer "$TEST_ROOT/invalid-version-home" \
    TEST_UNAME_S=Linux \
    TEST_UNAME_M=x86_64 \
    JIRA_KANBAN_TUI_VERSION=main >/dev/null 2>&1
then
    fail "installer accepted an invalid version"
fi

printf '%s\n' 'installer tests passed'
