#!/usr/bin/env sh
# =============================================================================
# Xergon Network Installer
# One-liner: curl -sSL https://xergon.network/install | sh
#
# Supports: install, update, uninstall
# Dependencies: curl, tar, sh (standard Unix)
# =============================================================================

set -e

# ── Cleanup on interrupt ─────────────────────────────────────────────────────

cleanup() {
    if [ -n "${TMPDIR:-}" ] && [ -d "${TMPDIR}" ]; then
        rm -rf "${TMPDIR}"
    fi
}
trap cleanup EXIT INT TERM

# ── Constants ──────────────────────────────────────────────────────────────────

XERGON_VERSION="${XERGON_VERSION:-latest}"
XERGON_REPO="n1ur0/Xergon-Network"
XERGON_BASE_URL="https://github.com/${XERGON_REPO}/releases"

# All binaries distributed in the tarball
ALL_BINARIES="xergon-agent xergon xergon-relay compile_contracts"

# Directory structure
XERGON_HOME="${HOME}/.xergon"
XERGON_BIN="${XERGON_HOME}/bin"
XERGON_DATA="${XERGON_HOME}/data"

# Parse --prefix flag
PREFIX=""
COMMAND=""

# ── Color Support ──────────────────────────────────────────────────────────────

if [ -t 1 ]; then
    SUPPORTS_COLOR=1
else
    SUPPORTS_COLOR=0
fi

if [ "$SUPPORTS_COLOR" -eq 1 ]; then
    RED='\033[0;31m'
    GREEN='\033[0;32m'
    YELLOW='\033[0;33m'
    BLUE='\033[0;34m'
    MAGENTA='\033[0;35m'
    CYAN='\033[0;36m'
    BOLD='\033[1m'
    DIM='\033[2m'
    RESET='\033[0m'
else
    RED=''
    GREEN=''
    YELLOW=''
    BLUE=''
    MAGENTA=''
    CYAN=''
    BOLD=''
    DIM=''
    RESET=''
fi

# ── Logging Helpers ────────────────────────────────────────────────────────────

info()    { printf "${GREEN}${BOLD}[INFO]${RESET}  %s\n" "$*"; }
warn()    { printf "${YELLOW}${BOLD}[WARN]${RESET}  %s\n" "$*"; }
error()   { printf "${RED}${BOLD}[ERROR]${RESET} %s\n" "$*" >&2; }
step()    { printf "${CYAN}${BOLD}  ->${RESET} %s\n" "$*"; }
success() { printf "${GREEN}${BOLD}  OK${RESET} %s\n" "$*"; }
banner() {
    printf "\n"
    printf "${BOLD}${MAGENTA}  ============================================${RESET}\n"
    printf "${BOLD}${MAGENTA}  |${RESET}${BOLD}     XERGON NETWORK INSTALLER          ${RESET}${BOLD}${MAGENTA}|${RESET}\n"
    printf "${BOLD}${MAGENTA}  |${RESET}${DIM}  Decentralized AI on Ergo Blockchain   ${RESET}${BOLD}${MAGENTA}|${RESET}\n"
    printf "${BOLD}${MAGENTA}  ============================================${RESET}\n"
    printf "\n"
}

# ── Argument Parsing ──────────────────────────────────────────────────────────

parse_args() {
    while [ $# -gt 0 ]; do
        case "$1" in
            --version)
                if [ -n "$2" ] && [ "${2#-}" = "$2" ]; then
                    XERGON_VERSION="$2"
                    shift 2
                else
                    error "--version requires an argument (e.g. --version v0.1.0)"
                    exit 1
                fi
                ;;
            --prefix)
                if [ -n "$2" ] && [ "${2#-}" = "$2" ]; then
                    PREFIX="$2"
                    shift 2
                else
                    error "--prefix requires an argument (e.g. --prefix /usr/local)"
                    exit 1
                fi
                ;;
            --uninstall)
                COMMAND="uninstall"
                shift
                ;;
            -h|--help|help)
                COMMAND="help"
                shift
                ;;
            update)
                COMMAND="update"
                shift
                ;;
            install|"")
                COMMAND="install"
                shift
                ;;
            *)
                # Unknown flag -- might be the old positional "update" or "uninstall"
                COMMAND="$1"
                shift
                ;;
        esac
    done

    COMMAND="${COMMAND:-install}"

    # If --prefix is set, override XERGON_BIN
    if [ -n "${PREFIX}" ]; then
        XERGON_BIN="${PREFIX}/bin"
    fi
}

# ── OS / Arch Detection ────────────────────────────────────────────────────────

detect_os() {
    OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
    case "$OS" in
        linux)  OS="linux" ;;
        darwin) OS="darwin" ;;
        *)
            error "Unsupported operating system: ${OS}"
            error "Xergon supports Linux and macOS."
            exit 1
            ;;
    esac
}

detect_arch() {
    ARCH="$(uname -m)"
    case "$ARCH" in
        x86_64|amd64) ARCH="amd64" ;;
        aarch64|arm64) ARCH="arm64" ;;
        *)
            error "Unsupported architecture: ${ARCH}"
            error "Xergon supports amd64 and arm64."
            exit 1
            ;;
    esac
}

# ── Prerequisite Checks ────────────────────────────────────────────────────────

check_prereqs() {
    step "Checking prerequisites..."

    for cmd in curl tar sh; do
        if ! command -v "$cmd" >/dev/null 2>&1; then
            error "Required command not found: ${cmd}"
            error "Please install ${cmd} and try again."
            exit 1
        fi
    done

    # sha256sum or shasum (macOS)
    if command -v sha256sum >/dev/null 2>&1; then
        SHA256_CMD="sha256sum"
    elif command -v shasum >/dev/null 2>&1; then
        SHA256_CMD="shasum -a 256"
    else
        warn "Neither sha256sum nor shasum found -- checksum verification will be skipped"
        SHA256_CMD=""
    fi

    success "All prerequisites met"
}

# ── Directory Structure ────────────────────────────────────────────────────────

create_dirs() {
    step "Creating directory structure..."
    mkdir -p "${XERGON_BIN}"
    mkdir -p "${XERGON_DATA}"
    success "~/.xergon/bin/   (binaries)"
    success "~/.xergon/data/  (ledger, peers)"
}

# ── PATH Setup ─────────────────────────────────────────────────────────────────

path_entry="export PATH=\"${XERGON_BIN}:\$PATH\""
path_marker="# XERGON-NETWORK-PATH"

add_to_path() {
    local added=0

    _add_to_file() {
        local target_file="$1"
        if [ -f "$target_file" ]; then
            if grep -qF "${path_marker}" "$target_file" 2>/dev/null; then
                step "PATH already in ${target_file}, skipping"
            else
                printf '\n%s\n%s\n' "${path_marker}" "${path_entry}" >> "$target_file"
                success "Added PATH to ${target_file}"
                added=1
            fi
        fi
    }

    step "Configuring PATH in shell profile..."

    # Skip PATH modification when --prefix is used (system install)
    if [ -n "${PREFIX}" ]; then
        warn "Skipping PATH setup (--prefix used). Add ${XERGON_BIN} to your PATH manually."
        return 0
    fi

    _add_to_file "${HOME}/.zshrc"
    _add_to_file "${HOME}/.bashrc"
    _add_to_file "${HOME}/.bash_profile"
    _add_to_file "${HOME}/.profile"

    if [ "$added" -eq 0 ]; then
        if [ ! -f "${HOME}/.profile" ]; then
            printf '%s\n%s\n' "${path_marker}" "${path_entry}" > "${HOME}/.profile"
            success "Created ${HOME}/.profile with PATH entry"
        else
            step "No shell profile needed PATH update"
        fi
    fi

    # Make binaries available in current session
    export PATH="${XERGON_BIN}:$PATH"
}

remove_from_path() {
    step "Removing PATH entry from shell profiles..."

    for target_file in "${HOME}/.zshrc" "${HOME}/.bashrc" "${HOME}/.bash_profile" "${HOME}/.profile"; do
        if [ -f "$target_file" ]; then
            if grep -qF "${path_marker}" "$target_file" 2>/dev/null; then
                tmp_file="$(mktemp)"
                grep -vF "${path_marker}" "$target_file" | grep -vF "${path_entry}" > "$tmp_file" || true
                mv "$tmp_file" "$target_file"
                success "Cleaned ${target_file}"
            fi
        fi
    done
}

# ── Checksum Verification ─────────────────────────────────────────────────────

verify_checksum() {
    local tarball_path="$1"
    local checksum_url="$2"

    if [ -z "${SHA256_CMD}" ]; then
        warn "Skipping checksum verification (no sha256sum/shasum available)"
        return 0
    fi

    step "Verifying SHA256 checksum..."

    # Download checksums file
    local checksum_file="${TMPDIR}/checksums.txt"
    if ! curl --fail --silent --show-error --location \
        -o "${checksum_file}" "${checksum_url}" 2>/dev/null; then
        warn "Could not download checksums file -- skipping verification"
        return 0
    fi

    # Get the expected hash for our tarball
    local tarball_name
    tarball_name="$(basename "${tarball_path}")"
    local expected_hash
    expected_hash="$(grep "${tarball_name}" "${checksum_file}" 2>/dev/null | awk '{print $1}')"

    if [ -z "${expected_hash}" ]; then
        warn "No checksum found for ${tarball_name} in checksums file -- skipping verification"
        return 0
    fi

    # Compute actual hash
    local actual_hash
    actual_hash="$(eval "${SHA256_CMD} \"${tarball_path}\"" | awk '{print $1}')"

    if [ "${expected_hash}" = "${actual_hash}" ]; then
        success "Checksum verified: ${actual_hash}"
        return 0
    else
        error "Checksum mismatch!"
        error "  Expected: ${expected_hash}"
        error "  Actual:   ${actual_hash}"
        error "The downloaded file may be corrupted or tampered with."
        exit 1
    fi
}

# ── Download and Install Binaries ─────────────────────────────────────────────

download_and_install() {
    local os="$1"
    local arch="$2"

    local tarball_name="xergon-${os}-${arch}.tar.gz"
    local download_url="${XERGON_BASE_URL}/${XERGON_VERSION}/download/${tarball_name}"
    local checksum_url="${XERGON_BASE_URL}/${XERGON_VERSION}/download/checksums.txt"

    step "Downloading ${tarball_name}..."

    # Create temp directory for download
    TMPDIR="$(mktemp -d)"

    # Download with retry
    local retries=3
    local attempt=1

    while [ "$attempt" -le "$retries" ]; do
        info "Download attempt ${attempt}/${retries}..."

        if curl --fail --silent --show-error --location --progress-bar \
            -o "${TMPDIR}/${tarball_name}" "${download_url}"; then
            break
        fi

        attempt=$((attempt + 1))

        if [ "$attempt" -le "$retries" ]; then
            warn "Download failed, retrying in 3 seconds..."
            sleep 3
        fi
    done

    if [ "$attempt" -gt "$retries" ]; then
        error "Failed to download ${tarball_name} after ${retries} attempts."
        error "URL: ${download_url}"
        error ""
        error "This is expected if no release has been published yet."
        error "To build locally:"
        error "  cd xergon-agent  && cargo build --release"
        error "  cd xergon-relay  && cargo build --release"
        exit 1
    fi

    # Verify checksum
    verify_checksum "${TMPDIR}/${tarball_name}" "${checksum_url}"

    # Extract
    step "Extracting tarball..."
    tar -xzf "${TMPDIR}/${tarball_name}" -C "${TMPDIR}"

    # Install binaries
    step "Installing binaries to ${XERGON_BIN}/"
    for bin in ${ALL_BINARIES}; do
        if [ -f "${TMPDIR}/${bin}" ]; then
            chmod +x "${TMPDIR}/${bin}"
            mv "${TMPDIR}/${bin}" "${XERGON_BIN}/${bin}"
            success "Installed ${bin}"
        else
            warn "Binary ${bin} not found in tarball (may not be included in this release)"
        fi
    done

    success "Installation complete (${os}/${arch})"
}

# ── First-Time Setup Check ─────────────────────────────────────────────────────

check_first_time_setup() {
    local config_path="${XERGON_HOME}/config.toml"

    if [ ! -f "${config_path}" ]; then
        # Check if there's a config.toml.example in the xergon-agent directory
        # or use the embedded default
        step "No config found. First-time setup recommended."
        if [ -f "${XERGON_BIN}/xergon-agent" ]; then
            printf "\n"
            printf "${YELLOW}${BOLD}  IMPORTANT: Run 'xergon-agent setup' to configure your node.${RESET}\n"
            printf "${YELLOW}${BOLD}  This will generate config.toml and set up your provider identity.${RESET}\n"
        fi
    else
        step "Existing config found at ${config_path}"
    fi
}

# ── Install Command ────────────────────────────────────────────────────────────

do_install() {
    banner
    info "Installing Xergon Network..."
    printf "  ${DIM}Version: ${XERGON_VERSION}${RESET}\n"
    printf "  ${DIM}Target:  ${OS}/${ARCH}${RESET}\n"
    if [ -n "${PREFIX}" ]; then
        printf "  ${DIM}Prefix:  ${PREFIX}${RESET}\n"
    fi
    printf "\n"

    detect_os
    detect_arch
    check_prereqs
    create_dirs
    download_and_install "$OS" "$ARCH"
    add_to_path
    check_first_time_setup

    printf "\n"
    info "Installation complete!"
    printf "\n"
    printf "${DIM}  Install dir:  ${XERGON_BIN}/${RESET}\n"
    printf "${DIM}  Config:       ${XERGON_HOME}/config.toml${RESET}\n"
    printf "${DIM}  Data:         ${XERGON_HOME}/data/${RESET}\n"
    printf "\n"

    if [ -n "${PREFIX}" ]; then
        printf "${CYAN}  Note: Make sure ${XERGON_BIN} is in your PATH.${RESET}\n"
    else
        printf "${CYAN}  To activate in current shell:${RESET}\n"
        printf "    ${BOLD}source ~/.bashrc${RESET}    (or ~/.zshrc)\n"
    fi

    printf "\n"
    printf "${CYAN}  Quick start:${RESET}\n"
    printf "    ${BOLD}xergon-agent setup${RESET}       -- interactive first-run config\n"
    printf "    ${BOLD}xergon-agent start${RESET}        -- start the agent\n"
    printf "    ${BOLD}xergon --help${RESET}             -- CLI for querying models, balance\n"
    printf "    ${BOLD}xergon-relay --help${RESET}       -- relay server for routing requests\n"
    printf "\n"
    printf "${DIM}  Docs: https://docs.xergon.network${RESET}\n"
    printf "\n"
}

# ── Update Command ─────────────────────────────────────────────────────────────

do_update() {
    banner
    info "Updating Xergon Network..."
    printf "\n"

    detect_os
    detect_arch
    check_prereqs

    # Check for existing binaries
    local found_any=0
    for bin in ${ALL_BINARIES}; do
        if [ -f "${XERGON_BIN}/${bin}" ]; then
            found_any=1
            break
        fi
    done

    if [ "$found_any" -eq 0 ]; then
        error "No Xergon binaries found at ${XERGON_BIN}/"
        error "Run install first: curl -sSL https://xergon.network/install | sh"
        exit 1
    fi

    # Show current version if available
    if [ -f "${XERGON_BIN}/xergon-agent" ] && "${XERGON_BIN}/xergon-agent" --version >/dev/null 2>&1; then
        local current_version
        current_version="$("${XERGON_BIN}/xergon-agent" --version 2>/dev/null | head -1)"
        info "Current version: ${current_version}"
    fi

    # Backup existing binaries
    step "Backing up existing binaries..."
    for bin in ${ALL_BINARIES}; do
        if [ -f "${XERGON_BIN}/${bin}" ]; then
            cp "${XERGON_BIN}/${bin}" "${XERGON_BIN}/${bin}.bak"
        fi
    done
    success "Backup created"

    download_and_install "$OS" "$ARCH"

    # Clean up backups
    step "Cleaning up backups..."
    for bin in ${ALL_BINARIES}; do
        if [ -f "${XERGON_BIN}/${bin}.bak" ]; then
            rm -f "${XERGON_BIN}/${bin}.bak"
        fi
    done

    printf "\n"
    info "Update complete!"
    printf "\n"
    printf "${CYAN}  Run 'xergon-agent --version' to confirm.${RESET}\n"
    printf "\n"
}

# ── Uninstall Command ──────────────────────────────────────────────────────────

do_uninstall() {
    banner
    warn "This will remove Xergon Network from your system."
    printf "\n"

    # Determine which bin dir to clean
    local target_bin="${XERGON_BIN}"
    local target_home="${XERGON_HOME}"

    # If --prefix was used, only remove binaries from that prefix
    if [ -n "${PREFIX}" ]; then
        target_bin="${PREFIX}/bin"
    fi

    # Confirm uninstall
    printf "${YELLOW}${BOLD}  Are you sure you want to uninstall? [y/N] ${RESET}"
    read -r confirmation </dev/tty 2>/dev/null || confirmation="n"

    case "$confirmation" in
        y|Y|yes|YES) ;;
        *)
            info "Uninstall cancelled."
            exit 0
            ;;
    esac

    printf "\n"

    # Remove PATH entries (only if not using --prefix)
    if [ -z "${PREFIX}" ]; then
        remove_from_path
    fi

    # Remove binaries
    local removed=0
    for bin in ${ALL_BINARIES}; do
        if [ -f "${target_bin}/${bin}" ]; then
            step "Removing binary: ${target_bin}/${bin}"
            rm -f "${target_bin}/${bin}"
            rm -f "${target_bin}/${bin}.bak"
            removed=1
        fi
    done

    if [ "$removed" -eq 1 ]; then
        success "Binaries removed"
    else
        warn "No binaries found at ${target_bin}/"
    fi

    # Ask about data removal (only for non-prefix installs)
    if [ -z "${PREFIX}" ]; then
        printf "\n"
        printf "${YELLOW}${BOLD}  Remove all data including config and wallet? [y/N] ${RESET}"
        read -r remove_data </dev/tty 2>/dev/null || remove_data="n"

        case "$remove_data" in
            y|Y|yes|YES)
                if [ -d "${target_home}" ]; then
                    step "Removing directory: ${target_home}"
                    rm -rf "${target_home}"
                    success "All data removed"
                fi
                ;;
            *)
                info "Keeping ${target_home}/ (config, wallet, data preserved)"
                ;;
        esac
    fi

    printf "\n"
    info "Xergon Network has been uninstalled."
    printf "\n"
    printf "${DIM}  To reinstall: curl -sSL https://xergon.network/install | sh${RESET}\n"
    printf "\n"
}

# ── Help Command ───────────────────────────────────────────────────────────────

do_help() {
    banner
    printf "  ${BOLD}USAGE${RESET}\n"
    printf "    curl -sSL https://xergon.network/install | sh                  # install latest\n"
    printf "    curl -sSL https://xergon.network/install | sh -s -- update      # update\n"
    printf "    curl -sSL https://xergon.network/install | sh -s -- uninstall   # uninstall\n"
    printf "\n"
    printf "  ${BOLD}OPTIONS${RESET}\n"
    printf "    --version <tag>     Install specific version (default: latest)\n"
    printf "                        Example: --version v0.1.0\n"
    printf "    --prefix <dir>      Install to custom prefix (default: ~/.xergon)\n"
    printf "                        Example: --prefix /usr/local\n"
    printf "                        Binaries go to <prefix>/bin/\n"
    printf "    --uninstall         Remove Xergon from system\n"
    printf "    -h, --help          Show this help message\n"
    printf "\n"
    printf "  ${BOLD}ENVIRONMENT VARIABLES${RESET}\n"
    printf "    XERGON_VERSION=<tag>  Same as --version\n"
    printf "\n"
    printf "  ${BOLD}COMMANDS (positional, after --)${RESET}\n"
    printf "    install              Install Xergon (default)\n"
    printf "    update               Update to latest version\n"
    printf "    uninstall            Remove Xergon from system\n"
    printf "    help                 Show this help message\n"
    printf "\n"
    printf "  ${BOLD}WHAT GETS INSTALLED${RESET}\n"
    printf "    xergon-agent         -- main agent binary (PoNW, peer discovery, settlement)\n"
    printf "    xergon               -- CLI tool (query models, manage balance)\n"
    printf "    xergon-relay         -- relay server (route inference requests)\n"
    printf "    compile_contracts    -- developer tool (compile ErgoScript contracts)\n"
    printf "\n"
    printf "  ${BOLD}FILE LAYOUT (default ~/.xergon)${RESET}\n"
    printf "    ~/.xergon/bin/              -- installed binaries\n"
    printf "    ~/.xergon/config.toml       -- agent configuration\n"
    printf "    ~/.xergon/data/             -- settlement ledger, peer data\n"
    printf "\n"
    printf "  ${BOLD}REQUIREMENTS${RESET}\n"
    printf "    - macOS or Linux\n"
    printf "    - amd64 or arm64 architecture\n"
    printf "    - curl, tar, sh\n"
    printf "\n"
    printf "  ${BOLD}CHECKSUM VERIFICATION${RESET}\n"
    printf "    The installer verifies SHA256 checksums when a checksums.txt is\n"
    printf "    present in the GitHub Release. Requires sha256sum or shasum.\n"
    printf "\n"
}

# ── Main ───────────────────────────────────────────────────────────────────────

main() {
    parse_args "$@"

    case "$COMMAND" in
        install)   do_install   ;;
        update)    do_update    ;;
        uninstall) do_uninstall ;;
        help)      do_help      ;;
        *)
            error "Unknown command: ${COMMAND}"
            error "Run with --help for usage information."
            exit 1
            ;;
    esac
}

main "$@"
