#!/bin/bash

# This script sets up the Polkit policy to allow the MSI Sidecar to run without a password.
# It detects if you are running in a development environment or if the app is installed.

# 1. Detect Sidecar Path
SIDECAR_PATH=""

# Check for installed version first
if [ -f "/usr/bin/msi-sidecar" ]; then
    SIDECAR_PATH="/usr/bin/msi-sidecar"
    echo "Detected Installed Sidecar: $SIDECAR_PATH"
# Check for local debug build
elif [ -f "src-tauri/target/debug/msi-sidecar-x86_64-unknown-linux-gnu" ]; then
    SIDECAR_PATH="$(pwd)/src-tauri/target/debug/msi-sidecar-x86_64-unknown-linux-gnu"
    echo "Detected Local Debug Sidecar: $SIDECAR_PATH"
# Check for local release build
elif [ -f "src-tauri/target/release/msi-sidecar-x86_64-unknown-linux-gnu" ]; then
    SIDECAR_PATH="$(pwd)/src-tauri/target/release/msi-sidecar-x86_64-unknown-linux-gnu"
    echo "Detected Local Release Sidecar: $SIDECAR_PATH"
else
    echo "Error: Could not find any sidecar binary."
    echo "  - If developing: Run 'npm run tauri build' or 'dev' first."
    echo "  - If installed: Ensure the package is installed correctly."
    exit 1
fi

# 2. Create Policy File
POLICY_NAME="com.msi.fancontrol.run-sidecar.policy"
TEMP_POLICY="/tmp/$POLICY_NAME"

cat <<EOF > $TEMP_POLICY
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE policyconfig PUBLIC
 "-//freedesktop//DTD PolicyKit Policy Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/PolicyKit/1/policyconfig.dtd">
<policyconfig>
  <action id="com.msi.fancontrol.run-sidecar">
    <description>Run MSI Fan Control Sidecar</description>
    <message>Authentication is required to run the MSI Fan Control helper</message>
    <defaults>
      <allow_any>yes</allow_any>
      <allow_inactive>yes</allow_inactive>
      <allow_active>yes</allow_active>
    </defaults>
    <annotate key="org.freedesktop.policykit.exec.path">$SIDECAR_PATH</annotate>
  </action>
</policyconfig>
EOF

# 3. Install Policy
echo "Installing Polkit policy to /usr/share/polkit-1/actions/..."
if sudo cp $TEMP_POLICY /usr/share/polkit-1/actions/$POLICY_NAME; then
    echo "Success: Policy installed for $SIDECAR_PATH"
    echo "You can now run the app without a password."
    
    # Restart polkit just in case
    if command -v systemctl >/dev/null 2>&1; then
        sudo systemctl restart polkit
    fi
else
    echo "Failed to install policy."
    exit 1
fi

rm $TEMP_POLICY
