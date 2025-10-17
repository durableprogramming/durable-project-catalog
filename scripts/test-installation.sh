#!/bin/bash
# Installation testing script for Durable Project Catalog
# Tests installation methods on various platforms using Docker

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
TEST_RESULTS_DIR="${PROJECT_ROOT}/test-results/installation"
DOCKER_DIR="${PROJECT_ROOT}/scripts/docker-tests"

# Installation test scenarios
declare -a INSTALL_SCENARIOS=(
    "ubuntu:22.04:apt:Test APT installation"
    "ubuntu:22.04:standalone:Test standalone binary"
    "alpine:3.19:standalone:Test musl binary on Alpine"
    "fedora:39:standalone:Test on Fedora"
    "debian:12:standalone:Test on Debian"
    "archlinux:latest:standalone:Test on Arch Linux"
)

print_header() {
    echo -e "${BLUE}========================================${NC}"
    echo -e "${BLUE}  Installation Method Tester${NC}"
    echo -e "${BLUE}========================================${NC}"
    echo ""
}

check_dependencies() {
    echo -e "${YELLOW}Checking dependencies...${NC}"

    if ! command -v docker &> /dev/null; then
        echo -e "${RED}Error: docker not found${NC}"
        exit 1
    fi

    echo -e "${GREEN}✓ Dependencies verified${NC}"
    echo ""
}

prepare_test_env() {
    echo -e "${YELLOW}Preparing test environment...${NC}"
    mkdir -p "$TEST_RESULTS_DIR"
    mkdir -p "$DOCKER_DIR"
    echo -e "${GREEN}✓ Test environment ready${NC}"
    echo ""
}

# Test standalone binary installation
test_standalone_install() {
    local distro=$1
    local test_name="${distro//:/-}-standalone"

    echo -e "${CYAN}Testing standalone installation on ${distro}...${NC}"

    # Create test Dockerfile
    cat > "${DOCKER_DIR}/Dockerfile.${test_name}" << EOF
FROM ${distro}

# Install dependencies
RUN if command -v apt-get > /dev/null; then \\
        apt-get update && apt-get install -y curl ca-certificates libsqlite3-0 && rm -rf /var/lib/apt/lists/*; \\
    elif command -v dnf > /dev/null; then \\
        dnf install -y curl ca-certificates sqlite-libs && dnf clean all; \\
    elif command -v yum > /dev/null; then \\
        yum install -y curl ca-certificates sqlite && yum clean all; \\
    elif command -v apk > /dev/null; then \\
        apk add --no-cache curl ca-certificates sqlite-libs bash; \\
    elif command -v pacman > /dev/null; then \\
        pacman -Sy --noconfirm curl ca-certificates sqlite && pacman -Scc --noconfirm; \\
    fi

# Copy binary from dist
COPY dist/dpc-* /tmp/

# Simulate installation
RUN set -e && \\
    if [ -f /tmp/dpc-linux-x86_64-musl ]; then \\
        cp /tmp/dpc-linux-x86_64-musl /usr/local/bin/dpc; \\
    elif [ -f /tmp/dpc-linux-x86_64 ]; then \\
        cp /tmp/dpc-linux-x86_64 /usr/local/bin/dpc; \\
    else \\
        echo "No suitable binary found" && exit 1; \\
    fi && \\
    chmod +x /usr/local/bin/dpc

# Verify installation
RUN dpc --version

# Create test user
RUN if ! id testuser > /dev/null 2>&1; then \\
        if command -v useradd > /dev/null; then \\
            useradd -m testuser; \\
        elif command -v adduser > /dev/null; then \\
            adduser -D testuser; \\
        fi; \\
    fi

USER testuser
WORKDIR /home/testuser

CMD ["dpc", "--help"]
EOF

    local log_file="${TEST_RESULTS_DIR}/${test_name}.log"

    if docker build -f "${DOCKER_DIR}/Dockerfile.${test_name}" -t "dpc-install-test:${test_name}" "${PROJECT_ROOT}" > "$log_file" 2>&1; then
        echo -e "${GREEN}  ✓ Build successful${NC}"

        # Run tests
        if docker run --rm "dpc-install-test:${test_name}" dpc --version >> "$log_file" 2>&1; then
            echo -e "${GREEN}  ✓ Version check passed${NC}"
        else
            echo -e "${RED}  ✗ Version check failed${NC}"
            return 1
        fi

        if docker run --rm "dpc-install-test:${test_name}" dpc --help >> "$log_file" 2>&1; then
            echo -e "${GREEN}  ✓ Help check passed${NC}"
        else
            echo -e "${RED}  ✗ Help check failed${NC}"
            return 1
        fi

        # Cleanup
        docker rmi "dpc-install-test:${test_name}" > /dev/null 2>&1 || true

        echo -e "${GREEN}  ✓ Installation test passed on ${distro}${NC}"
        echo ""
        return 0
    else
        echo -e "${RED}  ✗ Build failed on ${distro}${NC}"
        echo -e "${RED}    See: ${log_file}${NC}"
        echo ""
        return 1
    fi
}

# Test installation script
test_install_script() {
    local distro=$1
    local test_name="${distro//:/-}-script"

    echo -e "${CYAN}Testing installation script on ${distro}...${NC}"

    cat > "${DOCKER_DIR}/Dockerfile.${test_name}" << 'EOF'
ARG BASE_IMAGE
FROM ${BASE_IMAGE}

# Install dependencies
RUN if command -v apt-get > /dev/null; then \
        apt-get update && apt-get install -y curl ca-certificates libsqlite3-0 bash && rm -rf /var/lib/apt/lists/*; \
    elif command -v dnf > /dev/null; then \
        dnf install -y curl ca-certificates sqlite-libs bash && dnf clean all; \
    elif command -v yum > /dev/null; then \
        yum install -y curl ca-certificates sqlite bash && yum clean all; \
    elif command -v apk > /dev/null; then \
        apk add --no-cache curl ca-certificates sqlite-libs bash; \
    fi

# Copy installation script
COPY install.sh /tmp/install.sh
RUN chmod +x /tmp/install.sh

# Set environment to install to /usr/local/bin
ENV INSTALL_DIR=/usr/local/bin

# Note: In real test, this would download from GitHub
# For local testing, we'll mock it
RUN mkdir -p /tmp/mock-releases

USER root
WORKDIR /root

CMD ["/bin/bash"]
EOF

    local log_file="${TEST_RESULTS_DIR}/${test_name}.log"

    echo -e "${YELLOW}  Note: Full script test requires network access${NC}"
    echo -e "${YELLOW}  Performing script validation only${NC}"

    # Validate script syntax
    if bash -n "${PROJECT_ROOT}/install.sh" > "$log_file" 2>&1; then
        echo -e "${GREEN}  ✓ Installation script syntax valid${NC}"
        echo ""
        return 0
    else
        echo -e "${RED}  ✗ Installation script has syntax errors${NC}"
        echo -e "${RED}    See: ${log_file}${NC}"
        echo ""
        return 1
    fi
}

# Test cargo installation
test_cargo_install() {
    echo -e "${CYAN}Testing Cargo installation...${NC}"

    cat > "${DOCKER_DIR}/Dockerfile.cargo-test" << 'EOF'
FROM rust:1.75-slim

RUN apt-get update && \
    apt-get install -y libsqlite3-dev ca-certificates && \
    rm -rf /var/lib/apt/lists/*

COPY . /build
WORKDIR /build

# Test cargo install from local path
RUN cargo install --path lib/dprojc-cli

# Verify installation
RUN dpc --version

USER nobody
CMD ["dpc", "--help"]
EOF

    local log_file="${TEST_RESULTS_DIR}/cargo-install.log"

    if docker build -f "${DOCKER_DIR}/Dockerfile.cargo-test" -t "dpc-cargo-test" "${PROJECT_ROOT}" > "$log_file" 2>&1; then
        echo -e "${GREEN}  ✓ Cargo installation successful${NC}"

        if docker run --rm "dpc-cargo-test" > /dev/null 2>&1; then
            echo -e "${GREEN}  ✓ Cargo-installed binary works${NC}"
        else
            echo -e "${RED}  ✗ Cargo-installed binary failed${NC}"
            return 1
        fi

        docker rmi "dpc-cargo-test" > /dev/null 2>&1 || true
        echo ""
        return 0
    else
        echo -e "${RED}  ✗ Cargo installation failed${NC}"
        echo -e "${RED}    See: ${log_file}${NC}"
        echo ""
        return 1
    fi
}

# Test Docker image
test_docker_install() {
    echo -e "${CYAN}Testing Docker image installation...${NC}"

    local log_file="${TEST_RESULTS_DIR}/docker-install.log"

    if docker build -f Dockerfile -t "dpc-docker-test" "${PROJECT_ROOT}" > "$log_file" 2>&1; then
        echo -e "${GREEN}  ✓ Docker build successful${NC}"

        # Test various commands
        local tests_passed=true

        if docker run --rm "dpc-docker-test" --version >> "$log_file" 2>&1; then
            echo -e "${GREEN}  ✓ Version command works${NC}"
        else
            echo -e "${RED}  ✗ Version command failed${NC}"
            tests_passed=false
        fi

        if docker run --rm "dpc-docker-test" --help >> "$log_file" 2>&1; then
            echo -e "${GREEN}  ✓ Help command works${NC}"
        else
            echo -e "${RED}  ✗ Help command failed${NC}"
            tests_passed=false
        fi

        docker rmi "dpc-docker-test" > /dev/null 2>&1 || true

        if [ "$tests_passed" = true ]; then
            echo ""
            return 0
        else
            echo ""
            return 1
        fi
    else
        echo -e "${RED}  ✗ Docker build failed${NC}"
        echo -e "${RED}    See: ${log_file}${NC}"
        echo ""
        return 1
    fi
}

generate_report() {
    local passed=$1
    local failed=$2

    echo -e "${BLUE}========================================${NC}"
    echo -e "${BLUE}Installation Test Summary${NC}"
    echo -e "${BLUE}========================================${NC}"
    echo ""
    echo -e "${GREEN}Passed: ${passed}${NC}"
    echo -e "${RED}Failed: ${failed}${NC}"
    echo ""

    local total=$((passed + failed))
    if [ $total -gt 0 ]; then
        local success_rate=$(awk "BEGIN {printf \"%.1f\", ($passed / $total) * 100}")
        echo -e "Success Rate: ${success_rate}%"
    fi

    echo ""
    echo -e "${BLUE}Test results: ${TEST_RESULTS_DIR}${NC}"
    echo -e "${BLUE}========================================${NC}"
}

main() {
    print_header
    check_dependencies
    prepare_test_env

    local passed=0
    local failed=0

    # Test standalone installations on various distros
    for scenario in "${INSTALL_SCENARIOS[@]}"; do
        IFS=':' read -r distro version method description <<< "$scenario"
        local full_distro="${distro}:${version}"

        if [ "$method" = "standalone" ]; then
            if test_standalone_install "$full_distro"; then
                ((passed++))
            else
                ((failed++))
            fi
        fi
    done

    # Test installation script
    if test_install_script "ubuntu:22.04"; then
        ((passed++))
    else
        ((failed++))
    fi

    # Test Cargo installation
    if test_cargo_install; then
        ((passed++))
    else
        ((failed++))
    fi

    # Test Docker installation
    if test_docker_install; then
        ((passed++))
    else
        ((failed++))
    fi

    generate_report "$passed" "$failed"

    if [ $failed -gt 0 ]; then
        exit 1
    fi
}

main
