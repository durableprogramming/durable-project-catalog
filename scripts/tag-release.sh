#!/usr/bin/env bash
# Tag a new release for durable-project-catalog
#
# Usage:
#   scripts/tag-release.sh <version>
#
# Example:
#   scripts/tag-release.sh 0.2.0
#
# This script:
# 1. Validates version format
# 2. Checks git status is clean
# 3. Creates an annotated tag
# 4. Pushes the tag to origin

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Helper functions
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

# Check if version argument is provided
if [ $# -ne 1 ]; then
    error "Usage: $0 <version>\nExample: $0 0.2.0"
fi

VERSION="$1"

# Validate version format (semantic versioning)
if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.-]+)?(\+[a-zA-Z0-9.-]+)?$ ]]; then
    error "Invalid version format: $VERSION\nExpected format: X.Y.Z (e.g., 0.2.0, 1.0.0-rc.1)"
fi

# Check if we're in a git repository
if ! git rev-parse --git-dir > /dev/null 2>&1; then
    error "Not in a git repository"
fi

# Check if working directory is clean
if ! git diff-index --quiet HEAD --; then
    error "Working directory is not clean. Please commit or stash changes first."
fi

# Check if there are untracked files that should be committed
UNTRACKED=$(git ls-files --others --exclude-standard)
if [ -n "$UNTRACKED" ]; then
    warn "Warning: There are untracked files:"
    echo "$UNTRACKED"
    read -p "Continue anyway? (y/N) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        exit 1
    fi
fi

TAG_NAME="v${VERSION}"

# Check if tag already exists
if git rev-parse "$TAG_NAME" >/dev/null 2>&1; then
    error "Tag $TAG_NAME already exists"
fi

# Get current version from Cargo.toml
CURRENT_VERSION=$(grep '^version = ' Cargo.toml | head -1 | cut -d'"' -f2)

if [ "$CURRENT_VERSION" != "$VERSION" ]; then
    warn "Warning: Cargo.toml version ($CURRENT_VERSION) doesn't match tag version ($VERSION)"
    read -p "Continue anyway? (y/N) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        error "Please update Cargo.toml version first"
    fi
fi

# Create annotated tag
info "Creating tag $TAG_NAME..."
git tag -a "$TAG_NAME" -m "Release $VERSION"

info "Tag $TAG_NAME created successfully"

# Ask to push
read -p "Push tag to origin? (y/N) " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    info "Pushing tag to origin..."
    git push origin "$TAG_NAME"
    info "Tag pushed successfully"
    info ""
    info "Release $TAG_NAME is now available"
    info "Create a GitHub release at: https://github.com/durableprogramming/durable-project-catalog/releases/new?tag=$TAG_NAME"
else
    info "Tag created locally but not pushed"
    info "To push later, run: git push origin $TAG_NAME"
fi
