# cc2-webcontrol

cc2-webcontrol is a standalone Web UI for the Elegoo Centauri Carbon 2, designed to run directly on the printer without requiring a Docker container.

The project is a fork of https://github.com/DimeusDev/cc2-openwebui by DimeusDev, but takes a different approach by focusing on a web interface that runs natively on the CC2 hardware.

AI detection features from the original project are currently outside the scope of this standalone, on-device approach. They may still be possible through separate Docker-based components or future adaptations.

## Goals

- Run directly on the Elegoo Centauri Carbon 2
- Provide an onboard web interface without Docker overhead
- Focus on efficient monitoring and control of the printer
- Keep the installation lightweight and device-oriented

## Current Status

This project is experimental and under active development.

> [!WARNING]
> Use the software at your own risk and monitor the printer carefully, especially when starting a print through the web interface.

### Known Issues

- **The start-print feature may be unstable:** Monitor the first layer carefully when using it.
- Canvas filament editing is not yet available.
- Connecting to a printer with PIN protection is not yet fully supported.
- The user interface is not yet fully optimized for mobile devices.

## Features

- Standalone web interface running directly on the printer
- No Docker container required on the printer
- Svelte-based web interface available on port `8484`
- Native Rust backend for ARMv7
- Automatic printer recognition
- Customizable WebIF elements that can be shown or hidden
- Restart the WebControl server directly from the WebIF
- Restart the printer directly from the WebIF
- Notifications through NTFY and Discord webhooks
- Automatic startup through an init script
- Uninstall script included in the release archive
- Optional AI-based print-failure detection through separate Obico ML Docker components

## Requirements

- Elegoo Centauri Carbon 2
- SSH access to the printer
- Root access on the printer
- A computer with `curl` and `ssh` for direct installation
- Network access to the printer

## Installation

### Manual installation

Download the latest `webcontrol-v*.tar.gz` archive from the [Releases](https://github.com/stream2me/cc2-webcontrol/releases/latest) page and copy it to the printer.

Extract the archive into the root directory of the printer:

```sh
tar -xzf webcontrol-v*.tar.gz -C /
```

Enable automatic startup and start the service:

```
/etc/init.d/webif enable && /etc/init.d/webif start
```

After installation, open the following address in a web browser:

```text
http://<carbon2_ip>:8484
```

Replace `<carbon2_ip>` with the IP address of the printer.

### Direct installation via Terminal or SSH

The latest release can be streamed directly from GitHub to the printer without manually downloading and copying the archive.

Run the following commands on your local computer and replace `<carbon2_ip>` with the IP address of the printer:

```sh
CARBON2_IP="<carbon2_ip>"

DOWNLOAD_URL="$(
  curl -fsSL \
    https://api.github.com/repos/stream2me/cc2-webcontrol/releases/latest |
  sed -n 's/.*"browser_download_url": "\([^"]*\.tar\.gz\)".*/\1/p' |
  head -n 1
)"

if [ -z "${DOWNLOAD_URL}" ]; then
  echo "No release archive was found." >&2
  exit 1
fi

echo "Installing ${DOWNLOAD_URL}"

curl -fLsS "${DOWNLOAD_URL}" |
ssh "root@${CARBON2_IP}" '
  set -e
  tar -xzf - -C /
  /etc/init.d/webif enable
  /etc/init.d/webif restart
'
```

> [!NOTE]
> The direct installation command uses the latest published GitHub release.

After installation, open:

```text
http://<carbon2_ip>:8484
```

### Compact direct-installation command

The installation can alternatively be performed using a single command:

```sh
curl -fsSL https://api.github.com/repos/stream2me/cc2-webcontrol/releases/latest |
sed -n 's/.*"browser_download_url": "\([^"]*\.tar\.gz\)".*/\1/p' |
head -n 1 |
xargs curl -fLsS |
ssh root@<carbon2_ip> \
  'set -e
   tar -xzf - -C /
   /etc/init.d/webif enable
   /etc/init.d/webif restart'
```

## Updating

Download the latest release archive and copy it to the printer.

Stop the running service:

```sh
/etc/init.d/webif stop
```

Extract the new release over the existing installation:

```sh
tar -xzf webcontrol-v0.1.0.tar.gz -C /
```

Enable and restart the service:

```sh
/etc/init.d/webif enable && /etc/init.d/webif restart
```

The latest published release can also be installed using the direct installation command from the installation section.

## Uninstallation

Connect to the printer through SSH:

```sh
ssh root@<carbon2_ip>
```

Run the included uninstall script:

```sh
/opt/usr/.webcontrol/uninstall.sh
```

Alternatively, uninstall WebControl directly from the local computer:

```sh
ssh root@<carbon2_ip> '/opt/usr/.webcontrol/uninstall.sh'
```

## Installed Files

The release archive installs the following files and directories:

```text
/etc/init.d/webif
/opt/bin/webcontrol
/opt/usr/.webcontrol/.frontend/
/opt/usr/.webcontrol/uninstall.sh
```

The generated Svelte frontend, including `index.html`, JavaScript, CSS and other static assets, is installed under:

```text
/opt/usr/.webcontrol/.frontend/
```

## License

This project is based on https://github.com/DimeusDev/cc2-openwebui, which is licensed under the **MIT License**.

Accordingly, this modified standalone version is also available under the MIT License. Please preserve the original copyright notices and author attributions when modifying or redistributing the software.

## Contact & Credits

- Original project by **DimeusDev**
- Standalone / On-Device adaptations by **stream2me**
