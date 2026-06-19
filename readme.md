# cc2-webcontrol

cc2-webcontrol is a standalone Web UI for the Centauri Carbon 2, designed to run directly on the device without requiring a Docker container.

The project is a fork of [cc2-openwebui](https://github.com/DimeusDev/cc2-openwebui) by DimeusDev, but takes a different approach by focusing on a web interface that runs natively on the CC2 hardware.
AI detection features from the original project are currently out of scope for this standalone/on-device approach. They may still be possible separately through Docker-based components or future adaptations.

## Goals

- Run directly on the Centauri Carbon 2
- Provide an onboard web interface without Docker overhead
- Focus on efficient monitoring and controlling of the CC2
- Keep the setup lightweight and device-oriented

## Current Status

This project is currently experimental and under active development.

### Known Issues (from upstream)
* **Start print feature unstable:** Watch your first layer when using it!
* Canvas filament edit (soon)
* Printer connection when PIN enabled (soon) 
* UI is not yet responsive for mobile devices

## Features (On-Device)

- Automatic printer recognition
- Full Web UI running natively
- Customisable AI print failures detection with Obico ML (optional using Docker)
- Notifications support (NTFY and Discord webhook)
- All features from ElegooSlicer UI

## License

This project is based on [cc2-openwebui](https://github.com/DimeusDev/cc2-openwebui), which is licensed under the **MIT License**. 

Accordingly, this modified standalone version is also available under the MIT License. Please preserve the original copyright notices and author attributions when modifying or redistributing this software.

## Contact & Credits
- Original project by **DimeusDev** (_dimeus on Discord)
- Standalone / On-Device adaptations by **stream2me**

