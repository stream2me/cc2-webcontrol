#!/bin/sh
echo "Uninstalling webcontrol..."
cd /

# 1. Stop and disable the service
if [ -f /etc/init.d/webif ]; then
    /etc/init.d/webif stop || true
    /etc/init.d/webif disable || true
    rm -f /etc/init.d/webif
fi

# 2. Remove binaries and app dir
rm -f /opt/bin/webcontrol
rm -rf /opt/usr/.webcontrol

echo "Uninstallation complete! All files removed."
