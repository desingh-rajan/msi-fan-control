#!/bin/bash

# MSI Fan Control - Setup Permissions
# This script configures Polkit to allow the sidecar to run as root without password prompts.

set -e

APP_NAME="com.msi.fancontrol"
POLKIT_DIR="/usr/share/polkit-1/actions"
SIDECAR_NAME="msi-sidecar"

# Determine paths
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEV_SIDECAR_PATH="$PROJECT_ROOT/src-tauri/target/debug/$SIDECAR_NAME"
RELEASE_SIDECAR_PATH="/usr/bin/$SIDECAR_NAME"

echo "Setting up Polkit permissions for MSI Fan Control..."


# Generate Policy for Release Binary
cat <<EOF | sudo tee /usr/share/polkit-1/actions/com.msi.fancontrol.policy > /dev/null
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE policyconfig PUBLIC
 "-//freedesktop//DTD PolicyKit Policy Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/PolicyKit/1/policyconfig.dtd">
<policyconfig>
  <action id="com.msi.fancontrol.run-sidecar">
    <description>Run MSI Fan Control Sidecar</description>
    <message>Authentication is required to control fan speeds</message>
    <defaults>
      <allow_any>auth_admin</allow_any>
      <allow_inactive>auth_admin</allow_inactive>
      <allow_active>auth_admin</allow_active>
    </defaults>
    <annotate key="org.freedesktop.policykit.exec.path">$RELEASE_SIDECAR_PATH</annotate>
    <annotate key="org.freedesktop.policykit.exec.allow_gui">true</annotate>
  </action>
</policyconfig>
EOF

# Generate Policy for Dev Binary
cat <<EOF | sudo tee /usr/share/polkit-1/actions/com.msi.fancontrol.dev.policy > /dev/null
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE policyconfig PUBLIC
 "-//freedesktop//DTD PolicyKit Policy Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/PolicyKit/1/policyconfig.dtd">
<policyconfig>
  <action id="com.msi.fancontrol.run-sidecar-dev">
    <description>Run MSI Fan Control Sidecar (Dev)</description>
    <message>Authentication is required to control fan speeds (Dev)</message>
    <defaults>
      <allow_any>auth_admin</allow_any>
      <allow_inactive>auth_admin</allow_inactive>
      <allow_active>auth_admin</allow_active>
    </defaults>
    <annotate key="org.freedesktop.policykit.exec.path">$DEV_SIDECAR_PATH</annotate>
    <annotate key="org.freedesktop.policykit.exec.allow_gui">true</annotate>
  </action>
</policyconfig>
EOF

# Grant specific permission to current user group (likely sudo)
# Using .pkla for localauthority (older Polkit/Ubuntu standard)
PKLA_FILE="/etc/polkit-1/localauthority/50-local.d/com.msi.fancontrol.pkla"

echo "Creating PKLA override at $PKLA_FILE..."

cat <<EOF | sudo tee $PKLA_FILE > /dev/null
[Allow MSI Sidecar]
Identity=unix-group:sudo;unix-group:wheel;unix-user:$USER
Action=com.msi.fancontrol.run-sidecar;com.msi.fancontrol.run-sidecar-dev
ResultAny=yes
ResultInactive=yes
ResultActive=yes
EOF

echo "Done! Policy files installed. You might need to restart."

