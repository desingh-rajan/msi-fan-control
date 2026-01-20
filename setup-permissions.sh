#!/bin/bash

# Define the sidecar path - trying to detect if running from source or installed
SIDECAR_NAME="msi-sidecar-x86_64-unknown-linux-gnu"
SIDECAR_PATH=""

# Check potential locations
if [ -f "src-tauri/target/debug/$SIDECAR_NAME" ]; then
    SIDECAR_PATH="$(pwd)/src-tauri/target/debug/$SIDECAR_NAME"
elif [ -f "src-tauri/target/release/$SIDECAR_NAME" ]; then
    SIDECAR_PATH="$(pwd)/src-tauri/target/release/$SIDECAR_NAME"
else
    # Fallback for installed version (if applicable, though usually handles itself)
    echo "Could not find local sidecar binary. Ensure you have built the project."
    exit 1
fi

echo "Detected Sidecar Path: $SIDECAR_PATH"

POLICY_FILE="com.msi.fancontrol.policy"

cat <<EOF > $POLICY_FILE
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE policyconfig PUBLIC
 "- //freedesktop//DTD PolicyKit Policy Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/PolicyKit/1/policyconfig.dtd">
<policyconfig>
  <action id="com.msi.fancontrol.run-sidecar">
    <description>Run MSI Fan Control Sidecar</description>
    <message>Authentication is required to run the MSI Fan Control sidecar</message>
    <defaults>
      <allow_any>yes</allow_any>
      <allow_inactive>yes</allow_inactive>
      <allow_active>yes</allow_active>
    </defaults>
    <annotate key="org.freedesktop.policykit.exec.path">$SIDECAR_PATH</annotate>
  </action>
</policyconfig>
EOF

echo "Installing Polkit policy..."
sudo cp $POLICY_FILE /usr/share/polkit-1/actions/
sudo rm $POLICY_FILE

echo "Done! Password prompt should be gone for this build."
