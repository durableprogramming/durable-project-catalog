#!/usr/bin/env bash
# Bump major version in workspace Cargo.toml and all member crates
#
# Usage: scripts/versions/major_bump.sh
#
# Example: 0.1.0 -> 1.0.0

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

error() {
    echo -e "${RED}Error: $1${NC}" >&2
    exit 1
}

info() {
    echo -e "${GREEN}$1${NC}"
}

warn() {
    echo -e "${YELLOW}$1${NC}"
}

cd "$REPO_ROOT"

# Get current version from workspace Cargo.toml
CURRENT_VERSION=$(grep '^\[workspace.package\]' -A 20 Cargo.toml | grep '^version = ' | head -1 | cut -d'"' -f2)

if [ -z "$CURRENT_VERSION" ]; then
    error "Could not find version in Cargo.toml"
fi

# Parse version
IFS='.' read -r MAJOR MINOR PATCH <<< "$CURRENT_VERSION"

# Remove any pre-release or build metadata
PATCH="${PATCH%%-*}"
PATCH="${PATCH%%+*}"

# Bump major version
NEW_MAJOR=$((MAJOR + 1))
NEW_VERSION="${NEW_MAJOR}.0.0"

info "Current version: $CURRENT_VERSION"
info "New version: $NEW_VERSION"

# Confirm
read -p "Proceed with major version bump? (y/N) " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    exit 0
fi

# Update workspace Cargo.toml
info "Updating workspace Cargo.toml..."
sed -i "s/^version = \"$CURRENT_VERSION\"/version = \"$NEW_VERSION\"/" Cargo.toml

# Update all member crate Cargo.toml files
info "Updating member crates..."
for crate_dir in lib/dprojc-*; do
    if [ -f "$crate_dir/Cargo.toml" ]; then
        CRATE_NAME=$(basename "$crate_dir")
        sed -i "s/^version = \"$CURRENT_VERSION\"/version = \"$NEW_VERSION\"/" "$crate_dir/Cargo.toml"
        info "  Updated $CRATE_NAME"
    fi
done

# Update Cargo.lock
info "Updating Cargo.lock..."
cargo check --quiet 2>/dev/null || true

info ""
info "Version bumped from $CURRENT_VERSION to $NEW_VERSION"
info ""
warn "Don't forget to:"
echo "  1. Review the changes: git diff"
echo "  2. Commit the changes: git add -A && git commit -m 'Bump version to $NEW_VERSION'"
echo "  3. Create a tag: scripts/tag-release.sh $NEW_VERSION"
