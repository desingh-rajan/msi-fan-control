#!/bin/bash

# Check if version argument is provided
if [ -z "$1" ]; then
    echo "Usage: $0 <new_version>"
    echo "Example: $0 0.3.5"
    exit 1
fi

NEW_VERSION=$1
PROJECT_ROOT=$(pwd)

echo "Bumping version to $NEW_VERSION..."

# 1. Update package.json
if [ -f "$PROJECT_ROOT/package.json" ]; then
    sed -i "s/\"version\": \".*\"/\"version\": \"$NEW_VERSION\"/" "$PROJECT_ROOT/package.json"
    echo "Updated package.json"
fi

# 2. Update src-tauri/tauri.conf.json
if [ -f "$PROJECT_ROOT/src-tauri/tauri.conf.json" ]; then
    sed -i "s/\"version\": \".*\"/\"version\": \"$NEW_VERSION\"/" "$PROJECT_ROOT/src-tauri/tauri.conf.json"
    echo "Updated src-tauri/tauri.conf.json"
fi

# 3. Update src-tauri/Cargo.toml
if [ -f "$PROJECT_ROOT/src-tauri/Cargo.toml" ]; then
    # specifically match the version line in [package] section
    sed -i "0,/version = \".*\"/s//version = \"$NEW_VERSION\"/" "$PROJECT_ROOT/src-tauri/Cargo.toml"
    echo "Updated src-tauri/Cargo.toml"
fi

echo "Done! All files updated to $NEW_VERSION."
echo "Note: Cargo.lock will be updated next time you run cargo or npm run tauri dev/build."
