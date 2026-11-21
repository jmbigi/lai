#!/usr/bin/env bash
# Version management script for lai project
# Single source of truth: pyproject.toml [project].version

set -euo pipefail

PYPROJECT="pyproject.toml"
CHANGELOG="CHANGELOG.md"
VERSION_FILE="VERSION"  # kept for backwards compat, generated from pyproject.toml

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Extract version from pyproject.toml
get_version() {
    grep '^version = ' "$PYPROJECT" | head -1 | sed -E 's/version = "([^"]+)"/\1/' | sed 's/^/v/'
}

# Validate semver (v?MAJOR.MINOR.PATCH[-PRERELEASE][+BUILD])
validate_version() {
    local v=$1
    [[ $v =~ ^v?[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.-]+)?(\+[a-zA-Z0-9.-]+)?$ ]]
}

# Increment version
increment_version() {
    local version=$(get_version)
    local major minor patch
    IFS='.' read -r major minor patch <<< "${version#v}"
    
    case "$1" in
        major) major=$((major + 1)); minor=0; patch=0 ;;
        minor) minor=$((minor + 1)); patch=0 ;;
        patch) patch=$((patch + 1)) ;;
        *) echo "Usage: $0 bump {major|minor|patch}"; exit 1 ;;
    esac
    echo "v${major}.${minor}.${patch}"
}

# Update pyproject.toml version
update_pyproject_version() {
    local new_version=${1#v}
    sed -i -E "s/^version = \".*\"/version = \"${new_version}\"/" "$PYPROJECT"
}

# Update VERSION file (generated)
update_version_file() {
    echo "$1" > "$VERSION_FILE"
}

# Update gateway-go/config.yaml version
update_gateway_version() {
    local new_version=${1#v}
    local config="gateway-go/config.yaml"
    if [[ -f "$config" ]]; then
        sed -i -E "s/^(  version: \").*(\".*)$/\\1v${new_version}\\2/" "$config" 2>/dev/null || true
    fi
}

# Create git commit + tag
create_release() {
    local new_version=$1
    local dry_run=$2
    
    if ! validate_version "$new_version"; then
        echo -e "${RED}Invalid version format: $new_version${NC}" >&2
        exit 1
    fi
    
    local current=$(get_version)
    echo -e "${BLUE}Current: ${current}${NC}"
    echo -e "${GREEN}New:     ${new_version}${NC}"
    
    if [[ "$dry_run" == "true" ]]; then
        echo -e "${YELLOW}[DRY-RUN] Would:${NC}"
        echo "  1. Update $PYPROJECT version to ${new_version#v}"
        echo "  2. Update $VERSION_FILE to $new_version"
        echo "  3. Update gateway-go/config.yaml version"
        echo "  4. git commit -am \"chore: release $new_version\""
        echo "  5. git tag -a $new_version -m \"Release $new_version\""
        return 0
    fi
    
    update_pyproject_version "$new_version"
    update_version_file "$new_version"
    update_gateway_version "$new_version"
    
    git add "$PYPROJECT" "$VERSION_FILE" gateway-go/config.yaml 2>/dev/null || true
    git commit -m "chore: release $new_version"
    git tag -a "$new_version" -m "Release $new_version"
    
    echo -e "${GREEN}Released $new_version${NC}"
    echo -e "${YELLOW}Push with: git push origin master --tags${NC}"
}

# Show help
show_help() {
    cat <<EOF
Usage: $0 {version|bump|tag|validate|help}

Commands:
  version                Show current version (from pyproject.toml)
  bump <major|minor|patch>  Bump version, commit, and tag
  tag <version>          Tag specific version (validates semver)
  validate <version>     Validate version format
  --dry-run              Show what would happen (use with bump/tag)
  help                   Show this help

Examples:
  $0 version
  $0 bump patch          # v0.0.2 -> v0.0.3
  $0 bump minor --dry-run
  $0 tag v1.0.0
  $0 validate v2.3.4-beta.1
EOF
}

# Main
DRY_RUN=false
ARGS=()
for arg in "$@"; do
    case $arg in
        --dry-run) DRY_RUN=true ;;
        *) ARGS+=("$arg") ;;
    esac
done

case "${ARGS[0]:-help}" in
    version)   get_version ;;
    bump)
        [[ -z "${ARGS[1]}" ]] && { echo "Error: bump requires major|minor|patch" >&2; exit 1; }
        NEW=$(increment_version "${ARGS[1]}")
        create_release "$NEW" "$DRY_RUN"
        ;;
    tag)
        [[ -z "${ARGS[1]}" ]] && { echo "Error: tag requires version" >&2; exit 1; }
        create_release "${ARGS[1]}" "$DRY_RUN"
        ;;
    validate)
        [[ -z "${ARGS[1]}" ]] && { echo "Error: validate requires version" >&2; exit 1; }
        if validate_version "${ARGS[1]}"; then echo "Valid: ${ARGS[1]}"; else echo "Invalid: ${ARGS[1]}"; exit 1; fi
        ;;
    help|--help|-h) show_help ;;
    *) show_help; exit 1 ;;
esac