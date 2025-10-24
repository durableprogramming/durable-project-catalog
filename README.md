# Durable Project Catalog

The Durable Project Catalog is a pragmatic system for discovering and cataloging software projects within deeply nested directory structures. It provides a reliable way to identify, organize, and manage software projects across complex file trees, prioritizing sustainability, clarity, and long-term maintainability.

## Overview

In large codebases or extensive directory hierarchies, keeping track of individual software projects can be challenging. The Durable Project Catalog addresses this by implementing a set of clear, consistent rules for project identification and exclusion, ensuring that developers and teams can efficiently navigate and manage their software ecosystems.

## Project Identification Rules

A directory is considered a software project if it contains any of the following indicators:

- `.git` directory (Git repository)
- `package.json` (Node.js project)
- `Gemfile` (Ruby project)
- `.gemspec` (Ruby gem specification)
- Other common project markers (configurable)

## Exclusion Rules

The following directories are automatically excluded from project scanning:

- `node_modules/` (Node.js dependencies)
- `vendor/` (Ruby/vendor dependencies)
- Other common dependency directories (configurable)

## Features

- **Deep Tree Scanning**: Efficiently traverses deeply nested directory structures
- **Configurable Rules**: Customize project identification and exclusion criteria
- **Modular Design**: Composable components for flexibility and maintainability
- **Performance Optimized**: Fast scanning with minimal resource usage
- **SQLite Database Storage**: Persists project catalog data in a local SQLite database
- **Terminal User Interface (TUI)**: Provides a TUI with fuzzy finder for quick directory access
- **Clear Documentation**: Comprehensive guides and examples
- **Open Standards**: Interoperable with existing tools and workflows

## Data Storage

The system stores project catalog data in a SQLite database located at `~/.local/durable/durable-project-catalog`. This ensures persistent storage of discovered projects and their metadata. The database tracks various "hints" indicating the type of project, such as the presence of:

- `.git` directory (Git repository)
- `Cargo.toml` (Rust project)
- `package.json` (Node.js project)
- `devenv.nix` (Nix development environment)
- And other configurable project indicators.

## Usage

### Basic Commands

```bash
# Scan directories for projects
dpc scan ~/projects ~/work

# List all cataloged projects
dpc list

# Search for projects by name/path
dpc search myproject

# Show catalog statistics
dpc stats

# Launch interactive TUI with fuzzy finder
dpc tui

# Generate documentation for cataloged projects
dpc docs --output-dir ./docs

# Clean old scan results (older than 30 days)
dpc clean --max-age-days 30

# Clean old Cargo target directories
dpc clean-old-cargo ~/projects --max-age-hours 48
```

### Advanced Usage

```bash
# Scan with custom configuration
dpc --config config.yaml scan /path/to/codebase

# Use a custom database location
dpc --database /path/to/custom.db scan ~/projects

# List projects of a specific type
dpc list --project-type rust

# Search with result limit
dpc search webapp --limit 5

# Generate report in JSON format
dpc report --output catalog.json --format json --stats

# Dry run clean operation
dpc clean --dry-run --max-age-days 7
```

## Installation

### Quick Start (Recommended)

**One-line installation for your platform:**

<details>
<summary><b>Linux / macOS</b></summary>

```bash
curl https://get.durableprogramming.com/durable-project-catalog | bash

```

Or download and run manually:

```bash
curl https://get.durableprogramming.com/durable-project-catalog | tee install.sh
chmod +x install.sh
./install.sh
```

</details>


Verify installation:
```bash
dpc --version
```

### Platform-Specific Installation

<details>
<summary><b>Linux</b></summary>

#### Download Pre-built Binary

**x86_64:**
```bash
curl -L -o dpc https://github.com/durableprogramming/durable-project-catalog/releases/latest/download/dpc-linux-x86_64
chmod +x dpc
sudo mv dpc /usr/local/bin/
```

**ARM64:**
```bash
curl -L -o dpc https://github.com/durableprogramming/durable-project-catalog/releases/latest/download/dpc-linux-aarch64
chmod +x dpc
sudo mv dpc /usr/local/bin/
```

**Static build (musl, no dependencies):**
```bash
curl -L -o dpc https://github.com/durableprogramming/durable-project-catalog/releases/latest/download/dpc-linux-x86_64-musl
chmod +x dpc
sudo mv dpc /usr/local/bin/
```

</details>

<details>
<summary><b>macOS</b></summary>

#### Download Pre-built Binary

**Intel Macs:**
```bash
curl -L -o dpc https://github.com/durableprogramming/durable-project-catalog/releases/latest/download/dpc-macos-intel
chmod +x dpc
sudo mv dpc /usr/local/bin/
```

**Apple Silicon (M1/M2/M3):**
```bash
curl -L -o dpc https://github.com/durableprogramming/durable-project-catalog/releases/latest/download/dpc-macos-arm64
chmod +x dpc
sudo mv dpc /usr/local/bin/
```

If you encounter Gatekeeper warnings:
```bash
sudo xattr -r -d com.apple.quarantine /usr/local/bin/dpc
```

</details>

<details>
<summary><b>Windows</b></summary>

#### Download Pre-built Binary

**x64:**
Download [dpc-windows-x86_64.exe](https://github.com/durableprogramming/durable-project-catalog/releases/latest/download/dpc-windows-x86_64.exe)

**ARM64:**
Download [dpc-windows-aarch64.exe](https://github.com/durableprogramming/durable-project-catalog/releases/latest/download/dpc-windows-aarch64.exe)

**Installation steps:**
1. Create folder: `C:\Tools\dpc`
2. Move downloaded file to `C:\Tools\dpc\dpc.exe`
3. Add to PATH:
   - Open System Properties (Win + Pause)
   - Click "Environment Variables"
   - Under "User variables", select "Path"
   - Click "Edit" → "New"
   - Add: `C:\Tools\dpc`
   - Click OK and restart terminal

</details>

### Language Package Managers

<details>
<summary><b>Cargo (Rust)</b></summary>

```bash
# Install from crates.io
cargo install dprojc-cli

# Install from source
cargo install --git https://github.com/durableprogramming/durable-project-catalog dprojc-cli
```

**Prerequisites:**
- Rust toolchain (1.70 or later)
- SQLite3 development libraries

</details>

### Container / Docker

```bash
# Pull from GitHub Container Registry
docker pull ghcr.io/durableprogramming/durable-project-catalog:latest

# Or from Docker Hub
docker pull durableprogramming/durable-project-catalog:latest

# Run with volume mount
docker run --rm -v ~/projects:/projects ghcr.io/durableprogramming/durable-project-catalog:latest scan /projects
```

See [Docker documentation](docs/docker.md) for detailed usage.

### From Source

For development or customization:

```bash
# Clone repository
git clone https://github.com/durableprogramming/durable-project-catalog.git
cd durable-project-catalog

# Build release binary
cargo build --release

# Binary location
./target/release/dpc

# Install to ~/.cargo/bin
cargo install --path lib/dprojc-cli
```

**Development with devenv:**
```bash
devenv shell
cargo build
```

### Shell Integration

The Durable Project Catalog includes powerful shell integration for quick navigation to cataloged projects, similar to tools like `zoxide` or `autojump`.

#### Installation Steps

1. **First, scan your directories** to populate the project catalog:
   ```bash
   dpc scan ~/projects ~/work
   ```

2. **Generate the shell integration script** for your shell:

   **For Bash:**
   ```bash
   # Add to ~/.bashrc
   eval "$(dpc shell init bash)"
   ```

   **For Zsh:**
   ```bash
   # Add to ~/.zshrc
   eval "$(dpc shell init zsh)"
   ```

   **For Fish:**
   ```bash
   # Add to ~/.config/fish/config.fish
   dpc shell init fish | source
   ```

3. **Reload your shell configuration:**
   ```bash
   # Bash
   source ~/.bashrc

   # Zsh
   source ~/.zshrc

   # Fish
   source ~/.config/fish/config.fish
   ```

#### Shell Commands

Once installed, you have access to these commands:

- **`j <pattern>`** - Jump to a project matching the pattern
  ```bash
  j myproject        # Jump to project containing "myproject"
  j durable/catalog  # Jump to project matching path pattern
  ```

- **`ji`** - Interactive project selector (launches TUI)
  ```bash
  ji  # Opens fuzzy finder to select and navigate to a project
  ```

- **`dpc-cd <pattern>`** - Explicit version of the `j` command
  ```bash
  dpc-cd webapp  # Same as 'j webapp'
  ```

#### Frecency Tracking

The shell integration automatically tracks your directory access patterns using a "frecency" algorithm (frequency + recency). This means:

- Projects you visit often appear higher in search results
- Recently accessed projects are prioritized
- The ranking improves over time as you use the tool

For Zsh and Fish, automatic tracking is enabled via shell hooks. For Bash, tracking happens when you use the `j` command.

## Configuration

Create a `config.yaml` file to customize scanning behavior:

```yaml
project_indicators:
  - .git
  - package.json
  - Gemfile
  - pyproject.toml

exclude_patterns:
  - node_modules
  - vendor
  - .git
  - __pycache__

max_depth: 10
```

## Contributing

We welcome contributions that align with our philosophy of pragmatic, sustainable software development. Please see our [Contributing Guide](CONTRIBUTING.md) for details.

### Development Setup
This project uses [devenv](https://devenv.sh/) for development environment management. To get started:

```bash
# Install devenv if not already available
# Then:
devenv shell
```

## Troubleshooting

### Installation Issues

**Linux: Permission Denied**
```bash
chmod +x dpc
```

**macOS: "dpc cannot be opened because the developer cannot be verified"**
```bash
sudo xattr -r -d com.apple.quarantine /usr/local/bin/dpc
```

**Windows: "dpc is not recognized as an internal or external command"**
Ensure the installation directory is in your PATH (see Windows installation instructions above).

### Runtime Issues

**Database Errors**
```bash
# Reset database
rm ~/.local/durable/durable-project-catalog

# Scan again
dpc scan ~/projects
```

**Performance Issues**
For large directory trees, consider:
- Excluding unnecessary directories
- Adjusting `max_depth` in configuration
- Using the `--exclude` flag

## Updating

To update to the latest version:

**Standalone Binary:**
```bash
# Re-run installation script
curl -sSL https://raw.githubusercontent.com/durableprogramming/durable-project-catalog/master/install.sh | bash
```

**Cargo:**
```bash
cargo install dprojc-cli --force
```

**Docker:**
```bash
docker pull ghcr.io/durableprogramming/durable-project-catalog:latest
```

## License

Dual-licensed under MIT OR Apache-2.0

## Support

For questions, issues, or contributions, please:

- Open an issue on GitHub
- Check our documentation
- Contact our support team

---

*Built with a focus on durability, clarity, and practical value.*
