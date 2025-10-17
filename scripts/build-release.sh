#!/bin/bash
# Local release build script for Durable Project Catalog
# Builds binaries for multiple platforms using cross-compilation

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Configuration
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILD_DIR="${PROJECT_ROOT}/dist"
BINARY_NAME="dpc"
VERSION="${VERSION:-$(grep '^version' "${PROJECT_ROOT}/Cargo.toml" | head -n1 | sed 's/.*"\(.*\)".*/\1/')}"

# Supported targets (format: target:artifact_name:use_zigbuild)
declare -a TARGETS=(
    "x86_64-unknown-linux-gnu:linux-x86_64:false"
    "x86_64-unknown-linux-musl:linux-x86_64-musl:true"
    "aarch64-unknown-linux-gnu:linux-aarch64:true"
    "aarch64-unknown-linux-musl:linux-aarch64-musl:true"
    "x86_64-apple-darwin:macos-intel:true"
    "aarch64-apple-darwin:macos-arm64:true"
    "x86_64-pc-windows-gnu:windows-x86_64.exe:true"
)

# Print header
print_header() {
    echo -e "${BLUE}========================================${NC}"
    echo -e "${BLUE}  Durable Project Catalog - Release Builder${NC}"
    echo -e "${BLUE}  Version: ${VERSION}${NC}"
    echo -e "${BLUE}========================================${NC}"
    echo ""
}

# Check dependencies
check_dependencies() {
    echo -e "${YELLOW}Checking dependencies...${NC}"

    if ! command -v cargo &> /dev/null; then
        echo -e "${RED}Error: cargo not found. Install Rust toolchain.${NC}"
        exit 1
    fi

    if ! command -v cargo-zigbuild &> /dev/null; then
        echo -e "${YELLOW}cargo-zigbuild not found. Installing...${NC}"
        cargo install cargo-zigbuild
    fi

    if ! command -v zig &> /dev/null; then
        echo -e "${RED}Error: zig not found. Install zig toolchain.${NC}"
        echo -e "${YELLOW}Visit: https://ziglang.org/download/${NC}"
        exit 1
    fi

    echo -e "${GREEN}✓ Dependencies verified${NC}"
    echo ""
}

# Clean build directory
clean_build_dir() {
    echo -e "${YELLOW}Cleaning build directory...${NC}"
    rm -rf "${BUILD_DIR}"
    mkdir -p "${BUILD_DIR}"
    echo -e "${GREEN}✓ Build directory ready: ${BUILD_DIR}${NC}"
    echo ""
}

# Build for a specific target
build_target() {
    local target=$1
    local artifact_name=$2
    local use_zigbuild=$3

    echo -e "${BLUE}Building for ${target}...${NC}"

    # Add target if not already installed (for non-zigbuild targets)
    if [ "$use_zigbuild" = "false" ]; then
        rustup target add "$target" 2>/dev/null || true
    fi

    # Build command
    if [ "$use_zigbuild" = "true" ]; then
        cargo zigbuild --release --target "$target" -p dprojc-cli
    else
        cargo build --release --target "$target" -p dprojc-cli
    fi

    # Determine binary name based on platform
    local binary_name="$BINARY_NAME"
    if [[ "$target" == *"windows"* ]]; then
        binary_name="${BINARY_NAME}.exe"
    fi

    local source_path="${PROJECT_ROOT}/target/${target}/release/${binary_name}"
    local dest_path="${BUILD_DIR}/${BINARY_NAME}-${artifact_name}"

    if [ -f "$source_path" ]; then
        # Strip binary (Unix-like systems only)
        if [[ "$target" != *"windows"* ]] && command -v strip &> /dev/null; then
            strip "$source_path" 2>/dev/null || true
        fi

        cp "$source_path" "$dest_path"
        echo -e "${GREEN}✓ Built: ${dest_path}${NC}"

        # Calculate size
        local size=$(du -h "$dest_path" | cut -f1)
        echo -e "${GREEN}  Size: ${size}${NC}"
    else
        echo -e "${RED}✗ Build failed: ${source_path} not found${NC}"
        return 1
    fi

    echo ""
}

# Build all targets
build_all_targets() {
    echo -e "${YELLOW}Building all targets...${NC}"
    echo ""

    local success_count=0
    local fail_count=0
    local failed_targets=()

    for target_spec in "${TARGETS[@]}"; do
        IFS=':' read -r target artifact_name use_zigbuild <<< "$target_spec"

        if build_target "$target" "$artifact_name" "$use_zigbuild"; then
            ((success_count++))
        else
            ((fail_count++))
            failed_targets+=("$target")
        fi
    done

    echo -e "${BLUE}========================================${NC}"
    echo -e "${GREEN}Successful builds: ${success_count}${NC}"
    if [ $fail_count -gt 0 ]; then
        echo -e "${RED}Failed builds: ${fail_count}${NC}"
        echo -e "${RED}Failed targets:${NC}"
        for target in "${failed_targets[@]}"; do
            echo -e "${RED}  - ${target}${NC}"
        done
    fi
    echo -e "${BLUE}========================================${NC}"
    echo ""
}

# Generate checksums
generate_checksums() {
    echo -e "${YELLOW}Generating checksums...${NC}"

    cd "${BUILD_DIR}"

    if command -v sha256sum &> /dev/null; then
        sha256sum ${BINARY_NAME}-* > checksums.txt
    elif command -v shasum &> /dev/null; then
        shasum -a 256 ${BINARY_NAME}-* > checksums.txt
    else
        echo -e "${RED}Warning: No checksum utility found${NC}"
        cd "${PROJECT_ROOT}"
        return
    fi

    echo -e "${GREEN}✓ Checksums generated: ${BUILD_DIR}/checksums.txt${NC}"
    cat checksums.txt
    echo ""

    cd "${PROJECT_ROOT}"
}

# Create archive
create_archive() {
    echo -e "${YELLOW}Creating release archive...${NC}"

    local archive_name="durable-project-catalog-${VERSION}-binaries"

    cd "${BUILD_DIR}/.."

    if command -v tar &> /dev/null; then
        tar -czf "${archive_name}.tar.gz" -C dist .
        echo -e "${GREEN}✓ Archive created: ${archive_name}.tar.gz${NC}"

        local size=$(du -h "${archive_name}.tar.gz" | cut -f1)
        echo -e "${GREEN}  Size: ${size}${NC}"
    fi

    if command -v zip &> /dev/null; then
        (cd dist && zip -r "../${archive_name}.zip" .)
        echo -e "${GREEN}✓ Archive created: ${archive_name}.zip${NC}"

        local size=$(du -h "${archive_name}.zip" | cut -f1)
        echo -e "${GREEN}  Size: ${size}${NC}"
    fi

    echo ""
    cd "${PROJECT_ROOT}"
}

# List artifacts
list_artifacts() {
    echo -e "${BLUE}========================================${NC}"
    echo -e "${BLUE}Release Artifacts:${NC}"
    echo -e "${BLUE}========================================${NC}"

    echo ""
    echo -e "${YELLOW}Binaries:${NC}"
    ls -lh "${BUILD_DIR}/${BINARY_NAME}-"* 2>/dev/null || echo "  None"

    echo ""
    echo -e "${YELLOW}Checksums:${NC}"
    [ -f "${BUILD_DIR}/checksums.txt" ] && echo "  ${BUILD_DIR}/checksums.txt" || echo "  None"

    echo ""
    echo -e "${YELLOW}Archives:${NC}"
    ls -lh "${PROJECT_ROOT}"/durable-project-catalog-*-binaries.* 2>/dev/null || echo "  None"

    echo ""
    echo -e "${BLUE}========================================${NC}"
}

# Parse arguments
SKIP_CHECKSUMS=false
SKIP_ARCHIVE=false
SPECIFIC_TARGET=""

while [[ $# -gt 0 ]]; do
    case $1 in
        --skip-checksums)
            SKIP_CHECKSUMS=true
            shift
            ;;
        --skip-archive)
            SKIP_ARCHIVE=true
            shift
            ;;
        --target)
            SPECIFIC_TARGET="$2"
            shift 2
            ;;
        --help)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --skip-checksums    Skip checksum generation"
            echo "  --skip-archive      Skip archive creation"
            echo "  --target TARGET     Build only specific target"
            echo "  --help              Show this help message"
            echo ""
            echo "Supported targets:"
            for target_spec in "${TARGETS[@]}"; do
                IFS=':' read -r target artifact_name use_cross <<< "$target_spec"
                echo "  - $target"
            done
            exit 0
            ;;
        *)
            echo -e "${RED}Unknown option: $1${NC}"
            exit 1
            ;;
    esac
done

# Main execution
main() {
    print_header
    check_dependencies
    clean_build_dir

    if [ -n "$SPECIFIC_TARGET" ]; then
        # Build specific target
        echo -e "${YELLOW}Building specific target: ${SPECIFIC_TARGET}${NC}"
        echo ""

        found=false
        for target_spec in "${TARGETS[@]}"; do
            IFS=':' read -r target artifact_name use_zigbuild <<< "$target_spec"
            if [ "$target" = "$SPECIFIC_TARGET" ]; then
                build_target "$target" "$artifact_name" "$use_zigbuild"
                found=true
                break
            fi
        done

        if [ "$found" = false ]; then
            echo -e "${RED}Error: Target ${SPECIFIC_TARGET} not found${NC}"
            exit 1
        fi
    else
        # Build all targets
        build_all_targets
    fi

    if [ "$SKIP_CHECKSUMS" = false ]; then
        generate_checksums
    fi

    if [ "$SKIP_ARCHIVE" = false ] && [ -z "$SPECIFIC_TARGET" ]; then
        create_archive
    fi

    list_artifacts

    echo -e "${GREEN}✓ Release build complete!${NC}"
}

# Run main function
main
