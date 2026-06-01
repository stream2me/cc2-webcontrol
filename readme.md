# cc2-webcontrol

cc2-webcontrol is a standalone Web UI for the Centauri Carbon 2, designed to run directly on the device without requiring a Docker container.

The project is based on [cc2-openwebui](https://github.com/DimeusDev/cc2-openwebui), but takes a different approach by focusing on an onboard web interface that runs directly on the CC2.

AI detection features from the original project are currently out of scope for this standalone/on-device approach. They may still be possible separately through Docker-based components or future adaptations.

## Goals

- Run directly on the Centauri Carbon 2
- Provide an onboard web interface
- Avoid the need for a Docker container for the core Web UI
- Focus on monitoring and controlling the CC2
- Keep the setup lightweight and device-oriented

## Current Status

This project is currently experimental and under active development.

## Based on

This project is based on [cc2-openwebui](https://github.com/DimeusDev/cc2-openwebui) by DimeusDev.

The original project provides a LAN Web UI for the Centauri Carbon 2 and includes Docker-based deployment and AI print failure detection via Obico ML.

cc2-webcontrol focuses on running the core Web UI directly on the printer without Docker. AI detection is currently not part of this approach, but may be added later through separate Docker-based components or adapted integration.

## License

The original repository currently does not include an explicit license.

Until the licensing situation is clarified, this project should be considered experimental/private and not a clearly licensed open-source redistribution.
