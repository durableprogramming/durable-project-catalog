# Build and Test Scripts

This directory contains scripts for building and testing releases of the Durable Project Catalog.

## Scripts Overview

### `build-release.sh`
Builds release binaries for multiple platforms using cross-compilation with cargo-zigbuild.

**Features:**
- Builds for 7 platforms (Linux x86_64/ARM64/musl, macOS Intel/ARM, Windows x86_64)
- Uses cargo-zigbuild for cross-compilation (works better with Nix/devenv)
- Generates SHA256 checksums
- Creates distribution archives (.tar.gz and .zip)
- Automatic dependency checking (installs `cargo-zigbuild` if needed)
- Strips binaries to reduce size

**Usage:**
```bash
# Build all targets
./scripts/build-release.sh

# Build specific target
./scripts/build-release.sh --target x86_64-unknown-linux-gnu

# Build without checksums
./scripts/build-release.sh --skip-checksums

# Build without creating archive
./scripts/build-release.sh --skip-archive

# Show help
./scripts/build-release.sh --help
```

**Output:**
- Binaries: `dist/dpc-<platform>`
- Checksums: `dist/checksums.txt`
- Archives: `durable-project-catalog-<version>-binaries.tar.gz` and `.zip`

**Requirements:**
- Rust toolchain
- Zig toolchain (for cargo-zigbuild)
- `cargo-zigbuild` (installed automatically if missing)

**Note:** This project uses devenv. If you're in a devenv shell, zig is automatically available. Otherwise install zig from https://ziglang.org/download/

---

### `test-releases.sh`
Tests release binaries on multiple Linux distributions using Docker.

**Features:**
- Tests on 11 different distributions (Ubuntu, Debian, Fedora, Alpine, Arch, Amazon Linux, Rocky Linux)
- Tests multiple versions of each distribution
- Validates all core commands (--version, --help, scan, list, stats)
- Generates detailed test logs
- Tests Docker image functionality
- Tests native binaries when applicable

**Usage:**
```bash
# Test all distributions
./scripts/test-releases.sh

# Test specific distribution
./scripts/test-releases.sh --distro ubuntu:22.04

# Skip Docker image test
./scripts/test-releases.sh --skip-docker

# Keep test images (don't cleanup)
./scripts/test-releases.sh --no-cleanup

# Show help
./scripts/test-releases.sh --help
```

**Output:**
- Test logs: `test-results/<distro-version>.test.log`
- Build logs: `test-results/<distro-version>.build.log`
- Summary with pass/fail counts and success rate

**Requirements:**
- Docker
- Pre-built binaries in `dist/` (run `build-release.sh` first)

**Tested Distributions:**
- Ubuntu 20.04, 22.04
- Debian 11, 12
- Fedora 38, 39
- Alpine 3.18, 3.19
- Arch Linux (latest)
- Amazon Linux 2023
- Rocky Linux 9

---

### `test-installation.sh`
Tests installation methods on various platforms using Docker.

**Features:**
- Tests standalone binary installation
- Tests Cargo installation from source
- Tests Docker image installation
- Validates installation scripts
- Tests on multiple distributions
- Comprehensive installation workflow validation

**Usage:**
```bash
# Run all installation tests
./scripts/test-installation.sh
```

**Output:**
- Test logs: `test-results/installation/*.log`
- Summary with pass/fail counts

**Requirements:**
- Docker
- Pre-built binaries in `dist/` (for standalone tests)
- All project files (for Cargo and Docker tests)

**Tests Performed:**
1. Standalone binary installation on Ubuntu, Debian, Fedora, Alpine, Arch
2. Installation script syntax validation
3. Cargo installation from local path
4. Docker image build and functionality

---

## Typical Workflow

### 1. Local Development Testing
```bash
# Run tests
cargo test --workspace

# Check formatting and linting
cargo fmt --check
cargo clippy
```

### 2. Build Release Binaries
```bash
# Build all platforms
./scripts/build-release.sh

# Verify binaries were created
ls -lh dist/
```

### 3. Test Release Binaries
```bash
# Test on all distributions
./scripts/test-releases.sh

# Or test specific distribution
./scripts/test-releases.sh --distro alpine:3.19
```

### 4. Test Installation Methods
```bash
# Test all installation methods
./scripts/test-installation.sh
```

### 5. Review Test Results
```bash
# View test results
ls -lh test-results/

# Check specific test log
cat test-results/ubuntu-22.04.test.log
```

---

## CI/CD Integration

These scripts are complemented by GitHub Actions workflows:

- `.github/workflows/ci.yml` - Continuous integration (tests, linting, coverage)
- `.github/workflows/release.yml` - Automated releases (uses similar logic to `build-release.sh`)

The local scripts allow you to:
1. Build and test before pushing
2. Debug CI failures locally
3. Verify releases work on target platforms
4. Test installation methods end-to-end

---

## Troubleshooting

### Build Issues

**Problem:** `cargo-zigbuild` fails to build
```bash
# Solution: Update cargo-zigbuild
cargo install cargo-zigbuild --force
```

**Problem:** Zig not found
```bash
# Solution: Install zig or enter devenv shell
devenv shell  # If using devenv
# Or install zig: https://ziglang.org/download/
```

**Problem:** Missing target
```bash
# Solution: Install target manually (only needed for non-zigbuild targets)
rustup target add x86_64-unknown-linux-gnu
```

### Test Issues

**Problem:** Docker daemon not running
```bash
# Solution: Start Docker
sudo systemctl start docker
```

**Problem:** Tests fail on specific distribution
```bash
# Solution: Check the log file
cat test-results/<distro-name>.test.log

# Run with just that distribution to debug
./scripts/test-releases.sh --distro <distro:version>
```

**Problem:** Out of disk space
```bash
# Solution: Clean up Docker
docker system prune -a

# Remove old test images
docker images | grep dpc-test | awk '{print $3}' | xargs docker rmi
```

---

## Platform-Specific Notes

### Linux
- All scripts should work out of the box
- Ensure Docker is installed and running
- May need to add user to `docker` group

### macOS
- Can build native macOS binaries (Intel and ARM)
- Cross-compilation to Linux/Windows requires Docker
- Use Homebrew to install Docker Desktop

### Windows
- Use WSL2 for best compatibility
- Alternatively, use Git Bash or PowerShell
- Docker Desktop required for cross-compilation

---

## Advanced Usage

### Building for Specific Platforms Only

```bash
# Build only Linux x86_64
./scripts/build-release.sh --target x86_64-unknown-linux-gnu

# Build only macOS ARM
./scripts/build-release.sh --target aarch64-apple-darwin
```

### Custom Build Directory

```bash
# Set custom output directory
BUILD_DIR=/tmp/my-release ./scripts/build-release.sh
```

### Testing with Custom Binaries

```bash
# Place binaries in dist/
mkdir -p dist
cp /path/to/dpc-linux-x86_64 dist/

# Run tests
./scripts/test-releases.sh
```

### Parallel Testing

```bash
# Test multiple distributions in parallel (requires GNU parallel)
parallel -j4 ./scripts/test-releases.sh --distro ::: \
    ubuntu:22.04 \
    debian:12 \
    fedora:39 \
    alpine:3.19
```

---

## Script Maintenance

When adding new platforms or distributions:

1. **build-release.sh**: Add to `TARGETS` array
2. **test-releases.sh**: Add to `TEST_DISTROS` array
3. **test-installation.sh**: Add to `INSTALL_SCENARIOS` array
4. Update this README with new platforms
5. Update GitHub Actions workflows if needed

---

## Performance Tips

### Faster Builds
- Use local cache: Docker builds cache layers automatically
- Build incrementally: Use `--target` for specific platforms
- Use `sccache` for Rust compilation caching

### Faster Tests
- Test specific distributions during development
- Use `--skip-docker` to skip Docker image tests
- Keep test images with `--no-cleanup` for repeated runs
- Run tests in parallel (see Advanced Usage)

---

## Contributing

When modifying these scripts:
1. Maintain POSIX compatibility where possible
2. Add color-coded output for better UX
3. Generate detailed logs for debugging
4. Handle errors gracefully with clear messages
5. Update this README with any changes
6. Test on multiple platforms before committing

---

For questions or issues, see the main project documentation or open an issue on GitHub.
