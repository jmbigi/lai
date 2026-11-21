#!/bin/bash
# Version management script for lai project

set -e

VERSION_FILE="VERSION"
CHANGELOG="CHANGELOG.md"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Get current version
get_version() {
    if [ -f "$VERSION_FILE" ]; then
        cat "$VERSION_FILE"
    else
        echo "v0.0.0"
    fi
}

# Parse version string
parse_version() {
    local version=$1
    # Remove leading 'v' if present
    version=${version#v}
    IFS='.' read -r major minor patch <<< "$version"
    echo "$major $minor $patch"
}

# Increment version
increment_version() {
    local version=$(get_version)
    local major minor patch
    read -r major minor patch <<< "$(parse_version "$version")"
    
    case "$1" in
        major)
            major=$((major + 1))
            minor=0
            patch=0
            ;;
        minor)
            minor=$((minor + 1))
            patch=0
            ;;
        patch)
            patch=$((patch + 1))
            ;;
        *)
            echo "Usage: $0 {major|minor|patch}"
            exit 1
            ;;
    esac
    
    echo "v${major}.${minor}.${patch}"
}

# Create new version tag
create_version() {
    local new_version=$(increment_version "$1")
    local current=$(get_version)
    
    echo -e "${BLUE}Current version: ${current}${NC}"
    echo -e "${GREEN}New version: ${new_version}${NC}"
    
    # Update VERSION file
    echo "$new_version" > "$VERSION_FILE"
    
    # Create git tag
    git tag -a "$new_version" -m "Release $new_version"
    
    echo -e "${GREEN}Created tag: $new_version${NC}"
    echo -e "${YELLOW}Don't forget to push: git push origin $new_version${NC}"
}

# Show current version
show_version() {
    echo -e "${BLUE}Current version: $(get_version)${NC}"
}

# Show help
show_help() {
    echo "Usage: $0 {version|bump|tag|help}"
    echo ""
    echo "Commands:"
    echo "  version          Show current version"
    echo "  bump [major|minor|patch]  Bump version and create tag"
    echo "  tag [version]    Create tag for specific version"
    echo "  help             Show this help"
    echo ""
    echo "Examples:"
    echo "  $0 version"
    echo "  $0 bump patch    # v0.0.1 -> v0.0.2"
    echo "  $0 bump minor    # v0.0.1 -> v0.1.0"
    echo "  $0 bump major    # v0.0.1 -> v1.0.0"
    echo "  $0 tag v1.0.0    # Create specific tag"
}

# Main
case "${1:-help}" in
    version)
        show_version
        ;;
    bump)
        if [ -z "$2" ]; then
            echo -e "${RED}Error: bump requires major|minor|patch${NC}"
            exit 1
        fi
        create_version "$2"
        ;;
    tag)
        if [ -z "$2" ]; then
            echo -e "${RED}Error: tag requires version${NC}"
            exit 1
        fi
        echo "$2" > "$VERSION_FILE"
        git tag -a "$2" -m "Release $2"
        echo -e "${GREEN}Created tag: $2${NC}"
        ;;
    help|--help|-h)
        show_help
        ;;
    *)
        show_help
        exit 1
        ;;
esac