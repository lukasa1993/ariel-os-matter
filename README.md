# Ariel OS Matter

This repository adds a chip-independent Matter subsystem to Ariel OS.

## Architecture plan

The design has four layers.

1. `ariel-os-matter-core` is a `no_std` crate. It contains typed device APIs, endpoint registration, device state machines, network control traits, storage traits, bridge control, OTA control, and factory reset control. It does not depend on a chip crate or on `rs-matter`.
2. `ariel-os-matter-rs` is the only layer that uses `rs-matter`. It converts the Ariel OS typed model to Matter attributes, commands, events, subscriptions, and endpoint metadata.
3. `ariel-os-matter-ariel` connects the core to Ariel OS Embassy networking, BLE, identity, time, random data, and storage.
4. Platform crates contain chip code. The first platform crate is for ESP32-S3. A different chip changes only this layer.

## Dependency plan

The first integration pins these source revisions:

- Ariel OS: `90f41cfe52c86c96b09b276b44e1a40e14733058`
- `rs-matter`: `ce693b88c5827428ad30990308470738d892b570` (`0.3.0` source, Matter 1.6 model)

The core uses Rust language async traits. It does not use Tokio, an operating-system thread API, POSIX, ESP-IDF, FreeRTOS, or a chip SDK.

Normal data and secret data use different storage traits. A secret backend can keep an opaque hardware key handle, or it can keep an encrypted value.

The device catalog comes from the Matter 1.6 device type model. CI permits only these three exclusions:

- Camera
- Video Doorbell
- Robotic Vacuum Cleaner

All other device types remain in the catalog. Cargo features remove unused device families and services from a build.

## Repository rules

Applications use typed commands and state objects. Application code does not read or write Matter TLV.

The Matter core does not contain `esp-*`, `nrf-*`, `stm32-*`, or `rp-*` dependencies.

Each state change has a defined result and test. Unsupported Matter paths return a Matter error. They do not return a false success.
