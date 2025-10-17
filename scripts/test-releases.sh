#!/bin/bash
# Docker-based release testing script for Durable Project Catalog
# Tests binaries on multiple Linux distributions and environments

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

# Configuration
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="${PROJECT_ROOT}/dist"
TEST_RESULTS_DIR="${PROJECT_ROOT}/test-results"
DOCKER_DIR="${PROJECT_ROOT}/scripts/docker-tests"

# Test distributions
declare -a TEST_DISTROS=(
    "ubuntu:22.04:x86_64:linux-x86_64"
    "ubuntu:20.04:x86_64:linux-x86_64"
    "debian:12:x86_64:linux-x86_64"
    "debian:11:x86_64:linux-x86_64"
    "fedora:39:x86_64:linux-x86_64"
    "fedora:38:x86_64:linux-x86_64"
    "alpine:3.19:x86_64:linux-x86_64-musl"
    "alpine:3.18:x86_64:linux-x86_64-musl"
    "archlinux:latest:x86_64:linux-x86_64"
    "amazonlinux:2023:x86_64:linux-x86_64"
    "rockylinux:9:x86_64:linux-x86_64"
)

# Test commands to run
declare -a TEST_COMMANDS=(
    "--version:Check version"
    "--help:Check help output"
    "scan /tmp:Test scan command"
    "list:Test list command"
    "stats:Test stats command"
)

# Print header
print_header() {
    echo -e "${BLUE}========================================${NC}"
    echo -e "${BLUE}  Durable Project Catalog - Release Tester${NC}"
    echo -e "${BLUE}========================================${NC}"
    echo ""
}

# Check dependencies
check_dependencies() {
    echo -e "${YELLOW}Checking dependencies...${NC}"

    if ! command -v docker &> /dev/null; then
        echo -e "${RED}Error: docker not found. Install Docker.${NC}"
        exit 1
    fi

    if ! docker info &> /dev/null; then
        echo -e "${RED}Error: Docker daemon not running.${NC}"
        exit 1
    fi

    echo -e "${GREEN}✓ Dependencies verified${NC}"
    echo ""
}

# Check if binaries exist
check_binaries() {
    echo -e "${YELLOW}Checking for release binaries...${NC}"

    if [ ! -d "$DIST_DIR" ]; then
        echo -e "${RED}Error: Distribution directory not found: ${DIST_DIR}${NC}"
        echo -e "${YELLOW}Run ./scripts/build-release.sh first${NC}"
        exit 1
    fi

    local binary_count=$(ls -1 "${DIST_DIR}"/dpc-* 2>/dev/null | grep -v checksums | wc -l)

    if [ "$binary_count" -eq 0 ]; then
        echo -e "${RED}Error: No binaries found in ${DIST_DIR}${NC}"
        echo -e "${YELLOW}Run ./scripts/build-release.sh first${NC}"
        exit 1
    fi

    echo -e "${GREEN}✓ Found ${binary_count} binaries${NC}"
    echo ""
}

# Prepare test environment
prepare_test_env() {
    echo -e "${YELLOW}Preparing test environment...${NC}"

    mkdir -p "$TEST_RESULTS_DIR"
    mkdir -p "$DOCKER_DIR"

    # Create base test Dockerfile template
    cat > "${DOCKER_DIR}/Dockerfile.test-template" << 'EOF'
ARG BASE_IMAGE
FROM ${BASE_IMAGE}

# Install minimal dependencies based on distro
RUN if command -v apt-get > /dev/null; then \
        apt-get update && apt-get install -y ca-certificates libsqlite3-0 && rm -rf /var/lib/apt/lists/*; \
    elif command -v dnf > /dev/null; then \
        dnf install -y ca-certificates sqlite-libs && dnf clean all; \
    elif command -v yum > /dev/null; then \
        yum install -y ca-certificates sqlite && yum clean all; \
    elif command -v apk > /dev/null; then \
        apk add --no-cache ca-certificates sqlite-libs; \
    elif command -v pacman > /dev/null; then \
        pacman -Sy --noconfirm ca-certificates sqlite && pacman -Scc --noconfirm; \
    fi

# Create test user
RUN if ! id testuser > /dev/null 2>&1; then \
        if command -v useradd > /dev/null; then \
            useradd -m -u 1000 testuser; \
        elif command -v adduser > /dev/null; then \
            adduser -D -u 1000 testuser; \
        fi; \
    fi

# Copy binary
COPY dpc /usr/local/bin/dpc
RUN chmod +x /usr/local/bin/dpc

# Create test directories
RUN mkdir -p /home/testuser/.local/durable && \
    mkdir -p /tmp/test-projects/project1 && \
    mkdir -p /tmp/test-projects/project2 && \
    echo '{"name": "test"}' > /tmp/test-projects/project1/package.json && \
    echo '[package]' > /tmp/test-projects/project2/Cargo.toml && \
    chown -R testuser:testuser /home/testuser /tmp/test-projects || true

USER testuser
WORKDIR /home/testuser

CMD ["/bin/sh"]
EOF

    echo -e "${GREEN}✓ Test environment ready${NC}"
    echo ""
}

# Test binary on a specific distribution
test_on_distro() {
    local distro=$1
    local arch=$2
    local binary_name=$3
    local test_name="${distro//:/-}"

    echo -e "${CYAN}Testing on ${distro} (${arch})...${NC}"

    local binary_path="${DIST_DIR}/dpc-${binary_name}"

    if [ ! -f "$binary_path" ]; then
        echo -e "${YELLOW}  ⚠ Binary not found: ${binary_path}${NC}"
        echo -e "${YELLOW}  Skipping ${distro}${NC}"
        echo ""
        return 1
    fi

    # Build test image
    local image_tag="dpc-test:${test_name}"

    echo -e "${YELLOW}  Building test image...${NC}"

    docker build \
        --build-arg BASE_IMAGE="$distro" \
        -f "${DOCKER_DIR}/Dockerfile.test-template" \
        -t "$image_tag" \
        --platform "linux/${arch}" \
        --build-context binaries="${DIST_DIR}" \
        "${DIST_DIR}" > "${TEST_RESULTS_DIR}/${test_name}.build.log" 2>&1

    if [ $? -ne 0 ]; then
        echo -e "${RED}  ✗ Failed to build test image${NC}"
        echo -e "${RED}    See: ${TEST_RESULTS_DIR}/${test_name}.build.log${NC}"
        echo ""
        return 1
    fi

    echo -e "${GREEN}  ✓ Test image built${NC}"

    # Run tests
    local all_passed=true
    local test_log="${TEST_RESULTS_DIR}/${test_name}.test.log"

    echo "" > "$test_log"
    echo "=== Test Results for ${distro} ===" >> "$test_log"
    echo "" >> "$test_log"

    for test_spec in "${TEST_COMMANDS[@]}"; do
        IFS=':' read -r command description <<< "$test_spec"

        echo -e "${YELLOW}  Testing: ${description}...${NC}"
        echo "Test: ${description}" >> "$test_log"
        echo "Command: dpc ${command}" >> "$test_log"

        if docker run --rm --platform "linux/${arch}" "$image_tag" dpc $command >> "$test_log" 2>&1; then
            echo -e "${GREEN}    ✓ ${description}${NC}"
            echo "Result: PASS" >> "$test_log"
        else
            echo -e "${RED}    ✗ ${description}${NC}"
            echo "Result: FAIL" >> "$test_log"
            all_passed=false
        fi

        echo "" >> "$test_log"
    done

    # Cleanup test image
    docker rmi "$image_tag" > /dev/null 2>&1 || true

    if [ "$all_passed" = true ]; then
        echo -e "${GREEN}  ✓ All tests passed on ${distro}${NC}"
        echo ""
        return 0
    else
        echo -e "${RED}  ✗ Some tests failed on ${distro}${NC}"
        echo -e "${RED}    See: ${test_log}${NC}"
        echo ""
        return 1
    fi
}

# Test Docker image
test_docker_image() {
    echo -e "${CYAN}Testing Docker image...${NC}"

    if [ ! -f "${PROJECT_ROOT}/Dockerfile" ]; then
        echo -e "${YELLOW}  ⚠ Dockerfile not found, skipping Docker test${NC}"
        echo ""
        return 0
    fi

    echo -e "${YELLOW}  Building Docker image...${NC}"

    docker build -t dpc-test:local "${PROJECT_ROOT}" > "${TEST_RESULTS_DIR}/docker-build.log" 2>&1

    if [ $? -ne 0 ]; then
        echo -e "${RED}  ✗ Failed to build Docker image${NC}"
        echo -e "${RED}    See: ${TEST_RESULTS_DIR}/docker-build.log${NC}"
        echo ""
        return 1
    fi

    echo -e "${GREEN}  ✓ Docker image built${NC}"

    # Test Docker image
    local docker_test_log="${TEST_RESULTS_DIR}/docker.test.log"
    echo "" > "$docker_test_log"

    echo -e "${YELLOW}  Testing Docker image...${NC}"

    if docker run --rm dpc-test:local --version >> "$docker_test_log" 2>&1; then
        echo -e "${GREEN}  ✓ Docker image test passed${NC}"
        docker rmi dpc-test:local > /dev/null 2>&1 || true
        echo ""
        return 0
    else
        echo -e "${RED}  ✗ Docker image test failed${NC}"
        echo -e "${RED}    See: ${docker_test_log}${NC}"
        docker rmi dpc-test:local > /dev/null 2>&1 || true
        echo ""
        return 1
    fi
}

# Test standalone binaries
test_standalone_binaries() {
    echo -e "${CYAN}Testing standalone binaries directly...${NC}"

    local test_log="${TEST_RESULTS_DIR}/standalone.test.log"
    echo "" > "$test_log"

    # Test native Linux binary if on Linux
    if [[ "$OSTYPE" == "linux-gnu"* ]]; then
        local arch=$(uname -m)
        local binary_name="linux-x86_64"

        if [ "$arch" = "aarch64" ]; then
            binary_name="linux-aarch64"
        fi

        local binary_path="${DIST_DIR}/dpc-${binary_name}"

        if [ -f "$binary_path" ]; then
            echo -e "${YELLOW}  Testing native binary: ${binary_name}${NC}"

            if "$binary_path" --version >> "$test_log" 2>&1; then
                echo -e "${GREEN}  ✓ Native binary test passed${NC}"
            else
                echo -e "${RED}  ✗ Native binary test failed${NC}"
            fi
        fi
    fi

    echo ""
}

# Generate test report
generate_report() {
    local passed=$1
    local failed=$2
    local skipped=$3

    echo -e "${BLUE}========================================${NC}"
    echo -e "${BLUE}Test Summary${NC}"
    echo -e "${BLUE}========================================${NC}"
    echo ""
    echo -e "${GREEN}Passed:  ${passed}${NC}"
    echo -e "${RED}Failed:  ${failed}${NC}"
    echo -e "${YELLOW}Skipped: ${skipped}${NC}"
    echo ""

    local total=$((passed + failed + skipped))
    local success_rate=0

    if [ $total -gt 0 ]; then
        success_rate=$(awk "BEGIN {printf \"%.1f\", ($passed / $total) * 100}")
    fi

    echo -e "Success Rate: ${success_rate}%"
    echo ""
    echo -e "${BLUE}Test results saved to: ${TEST_RESULTS_DIR}${NC}"
    echo -e "${BLUE}========================================${NC}"
}

# Parse arguments
RUN_DOCKER_TEST=true
SPECIFIC_DISTRO=""
CLEANUP=true

while [[ $# -gt 0 ]]; do
    case $1 in
        --skip-docker)
            RUN_DOCKER_TEST=false
            shift
            ;;
        --distro)
            SPECIFIC_DISTRO="$2"
            shift 2
            ;;
        --no-cleanup)
            CLEANUP=false
            shift
            ;;
        --help)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --skip-docker       Skip Docker image test"
            echo "  --distro DISTRO     Test only specific distro (e.g., ubuntu:22.04)"
            echo "  --no-cleanup        Don't cleanup test images"
            echo "  --help              Show this help message"
            echo ""
            echo "Supported distributions:"
            for distro_spec in "${TEST_DISTROS[@]}"; do
                IFS=':' read -r distro version arch binary <<< "$distro_spec"
                echo "  - ${distro}:${version}"
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
    check_binaries
    prepare_test_env

    local passed=0
    local failed=0
    local skipped=0

    # Test on distributions
    if [ -n "$SPECIFIC_DISTRO" ]; then
        echo -e "${YELLOW}Testing specific distribution: ${SPECIFIC_DISTRO}${NC}"
        echo ""

        found=false
        for distro_spec in "${TEST_DISTROS[@]}"; do
            IFS=':' read -r distro version arch binary_name <<< "$distro_spec"
            local full_name="${distro}:${version}"

            if [ "$full_name" = "$SPECIFIC_DISTRO" ]; then
                if test_on_distro "$full_name" "$arch" "$binary_name"; then
                    ((passed++))
                else
                    ((failed++))
                fi
                found=true
                break
            fi
        done

        if [ "$found" = false ]; then
            echo -e "${RED}Error: Distribution ${SPECIFIC_DISTRO} not found${NC}"
            exit 1
        fi
    else
        # Test all distributions
        for distro_spec in "${TEST_DISTROS[@]}"; do
            IFS=':' read -r distro version arch binary_name <<< "$distro_spec"
            local full_name="${distro}:${version}"

            if test_on_distro "$full_name" "$arch" "$binary_name"; then
                ((passed++))
            elif [ $? -eq 1 ] && grep -q "Binary not found" "${TEST_RESULTS_DIR}/${full_name//:/-}.test.log" 2>/dev/null; then
                ((skipped++))
            else
                ((failed++))
            fi
        done
    fi

    # Test Docker image
    if [ "$RUN_DOCKER_TEST" = true ]; then
        if test_docker_image; then
            ((passed++))
        else
            ((failed++))
        fi
    fi

    # Test standalone binaries
    test_standalone_binaries

    # Generate report
    generate_report "$passed" "$failed" "$skipped"

    if [ $failed -gt 0 ]; then
        exit 1
    fi
}

# Run main function
main
